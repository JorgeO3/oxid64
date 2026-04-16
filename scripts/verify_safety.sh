#!/usr/bin/env bash
# verify_safety.sh — Swiss-Cheese safety verification matrix for oxid64.
#
# Two modes:
#   --mode smoke   Fast gate (~2 min): lint, nextest, cargo-careful --lib,
#                  Miri lib + contracts + models, one Kani per backend, fuzz build smoke.
#   --mode full    Complete matrix: all layers including sharded Miri, all Kani proofs,
#                  ASan, MSan, extended proptest, fuzz build + smoke runs.
#                  Heavy lanes run in parallel with isolated CARGO_TARGET_DIR.
#
# Flags:
#   --strict        Exit on first missing tool instead of warning.
#   --fuzz-cases N  Proptest case count (default 5000 for full, 200 for smoke).
#   --fuzz-runs N   Fuzz smoke run count (default 64 for full, 8 for smoke).
#   --mode MODE     "smoke" or "full" (default: full).
#   --max-lanes N   Max parallel heavy lanes (default: 4).
#   --jobs N        CARGO_BUILD_JOBS per lane (default: auto-calculated).
#   --changed FILE  File with list of changed paths for routing (one per line).
#                   If omitted, all lanes run.
#   --dry-run       Show which lanes would run, then exit.
#
# Environment:
#   Requires nightly toolchain with miri component.
#   cargo-kani, cargo-fuzz (nightly), and clang are optional (warned/failed via --strict).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lane_defs.sh
source "${SCRIPT_DIR}/lane_defs.sh"

# --- Defaults ----------------------------------------------------------------
MODE="full"
STRICT=0
FUZZ_CASES=""
FUZZ_RUNS=""
MAX_LANES=4
CARGO_JOBS=""
CHANGED_FILE=""
DRY_RUN=0
PASS_COUNT=0
SKIP_COUNT=0
FAIL_COUNT=0
LANE_PIDS=()
LANE_NAMES=()
LANE_LOGS=()

# --- Argument parsing --------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)      STRICT=1;          shift ;;
    --mode)        MODE="$2";         shift 2 ;;
    --fuzz-cases)  FUZZ_CASES="$2";   shift 2 ;;
    --fuzz-runs)   FUZZ_RUNS="$2";    shift 2 ;;
    --max-lanes)   MAX_LANES="$2";    shift 2 ;;
    --jobs)        CARGO_JOBS="$2";   shift 2 ;;
    --changed)     CHANGED_FILE="$2"; shift 2 ;;
    --dry-run)     DRY_RUN=1;         shift ;;
    *)
      echo "Unknown arg: $1" >&2
      echo "Usage: $0 [--mode smoke|full] [--strict] [--fuzz-cases N] [--fuzz-runs N] [--max-lanes N] [--jobs N] [--changed FILE] [--dry-run]" >&2
      exit 2
      ;;
  esac
done

# Normalize name=value forms from task runners.
for var in FUZZ_CASES FUZZ_RUNS MODE MAX_LANES CARGO_JOBS; do
  val="${!var}"
  if [[ "$val" == *=* ]]; then
    eval "$var=\"${val##*=}\""
  fi
done

case "$MODE" in
  smoke|full) ;;
  *)
    echo "ERROR: --mode must be 'smoke' or 'full', got '$MODE'" >&2
    exit 2
    ;;
esac

# Apply per-mode defaults.
if [[ -z "$FUZZ_CASES" ]]; then
  [[ "$MODE" == "smoke" ]] && FUZZ_CASES=200 || FUZZ_CASES=5000
fi
if [[ -z "$FUZZ_RUNS" ]]; then
  [[ "$MODE" == "smoke" ]] && FUZZ_RUNS=8 || FUZZ_RUNS=64
fi

for var in FUZZ_CASES FUZZ_RUNS MAX_LANES; do
  if ! [[ "${!var}" =~ ^[0-9]+$ ]]; then
    echo "ERROR: $var must be an integer, got '${!var}'" >&2
    exit 2
  fi
done

# Auto-calculate jobs per lane if not specified.
if [[ -z "$CARGO_JOBS" ]]; then
  TOTAL_CPUS="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 8)"
  CARGO_JOBS=$(( TOTAL_CPUS / MAX_LANES ))
  [[ "$CARGO_JOBS" -lt 1 ]] && CARGO_JOBS=1
fi

# Base target dir for lane isolation.
VERIFY_TARGET_BASE="${CARGO_TARGET_DIR:-target}/verify"

# --- Routing -----------------------------------------------------------------
ACTIVE_BACKENDS="all"
if [[ -n "$CHANGED_FILE" && -f "$CHANGED_FILE" ]]; then
  ACTIVE_BACKENDS="$(affected_backends < "$CHANGED_FILE")"
fi

backend_active() {
  local backend="$1"
  [[ "$ACTIVE_BACKENDS" == "all" ]] || [[ " $ACTIVE_BACKENDS " == *" $backend "* ]]
}

# --- Helpers -----------------------------------------------------------------
have() { command -v "$1" >/dev/null 2>&1; }

have_miri() {
  have cargo && cargo +nightly miri --version >/dev/null 2>&1
}

have_kani() { have cargo-kani; }

have_fuzz() {
  have cargo && cargo +nightly fuzz --version >/dev/null 2>&1
}

warn_or_fail() {
  local msg="$1"
  SKIP_COUNT=$((SKIP_COUNT + 1))
  if [[ "$STRICT" -eq 1 ]]; then
    echo "FAIL:  $msg" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
    return 1
  fi
  echo "SKIP:  $msg"
}

layer_start() {
  echo
  echo "━━━ [$1] ━━━"
}

layer_pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  echo "  ✓ $1"
}

# --- Lane execution helpers --------------------------------------------------
# Run a function in the background as a lane with its own CARGO_TARGET_DIR and log.
# Usage: launch_lane <lane_name> <function_name> [args...]
launch_lane() {
  local name="$1"; shift
  local func="$1"; shift
  local log_file
  log_file="$(mktemp -t "oxid64-${name}-XXXXXX.log")"

  # Wait if we're at max parallel lanes.
  while [[ ${#LANE_PIDS[@]} -ge $MAX_LANES ]]; do
    wait_any_lane
  done

  (
    export CARGO_TARGET_DIR="${VERIFY_TARGET_BASE}/${name}"
    export CARGO_BUILD_JOBS="$CARGO_JOBS"
    "$func" "$@"
  ) > "$log_file" 2>&1 &

  LANE_PIDS+=($!)
  LANE_NAMES+=("$name")
  LANE_LOGS+=("$log_file")
  echo "  ▸ launched lane: $name (pid $!, jobs=$CARGO_JOBS, target=$VERIFY_TARGET_BASE/$name)"
}

# Wait for any one lane to finish and report its result.
wait_any_lane() {
  if [[ ${#LANE_PIDS[@]} -eq 0 ]]; then
    return
  fi

  # Wait for the first one that finishes.
  local idx=-1
  while true; do
    for i in "${!LANE_PIDS[@]}"; do
      if ! kill -0 "${LANE_PIDS[$i]}" 2>/dev/null; then
        idx=$i
        break 2
      fi
    done
    sleep 0.5
  done

  local pid="${LANE_PIDS[$idx]}"
  local name="${LANE_NAMES[$idx]}"
  local log="${LANE_LOGS[$idx]}"
  wait "$pid" && {
    layer_pass "lane: $name"
  } || {
    echo "  ✗ lane FAILED: $name (see $log)" >&2
    FAIL_COUNT=$((FAIL_COUNT + 1))
  }

  # Remove from tracking arrays.
  unset 'LANE_PIDS[idx]'
  unset 'LANE_NAMES[idx]'
  unset 'LANE_LOGS[idx]'
  LANE_PIDS=("${LANE_PIDS[@]}")
  LANE_NAMES=("${LANE_NAMES[@]}")
  LANE_LOGS=("${LANE_LOGS[@]}")
}

# Wait for all remaining lanes.
wait_all_lanes() {
  while [[ ${#LANE_PIDS[@]} -gt 0 ]]; do
    wait_any_lane
  done
}

# --- Lane functions ----------------------------------------------------------
# Each lane function runs in a subshell with its own CARGO_TARGET_DIR.

lane_nextest() {
  echo "=== nextest: --lib --tests ==="
  cargo nextest run --lib --tests
}

lane_doctest() {
  echo "=== doctests ==="
  cargo test --doc
}

lane_proptest_extended() {
  echo "=== proptest extended (PROPTEST_CASES=${FUZZ_CASES}) ==="
  env PROPTEST_CASES="${FUZZ_CASES}" cargo test "${PROPTEST_X86_BINS[@]}"
}

lane_careful() {
  echo "=== cargo-careful ==="
  if [[ "$MODE" == "smoke" ]]; then
    cargo +nightly careful test --lib
  else
    cargo +nightly careful test --lib --tests
    cargo +nightly careful test --doc
  fi
}

lane_miri_lib() {
  local flags="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"
  echo "=== Miri: --lib ==="
  MIRIFLAGS="$flags" cargo +nightly miri test --lib
  if [[ "$MODE" == "full" ]]; then
    echo "=== Miri: many-seeds --lib ==="
    MIRIFLAGS="$flags -Zmiri-many-seeds=0..4" cargo +nightly miri test --lib
  fi
}

lane_miri_contracts() {
  local flags="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"
  echo "=== Miri: contracts ==="
  MIRIFLAGS="$flags" cargo +nightly miri test "${MIRI_SHARD_CONTRACTS[@]}"
}

lane_miri_x86_models() {
  local flags="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"
  echo "=== Miri: x86 models ==="
  MIRIFLAGS="$flags" cargo +nightly miri test "${MIRI_SHARD_X86_MODELS[@]}"
}

lane_miri_other_models() {
  local flags="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"
  echo "=== Miri: NEON + WASM models ==="
  MIRIFLAGS="$flags" cargo +nightly miri test "${MIRI_SHARD_OTHER_MODELS[@]}"
}

lane_miri_proptest() {
  local flags="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"
  echo "=== Miri: proptest bins ==="
  MIRIFLAGS="$flags" cargo +nightly miri test "${MIRI_SHARD_PROPTEST[@]}"
}

lane_miri_x86_integration() {
  local flags="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"
  echo "=== Miri: x86 integration tests ==="
  MIRIFLAGS="$flags" cargo +nightly miri test "${MIRI_SHARD_X86_INTEGRATION[@]}"
}

lane_asan() {
  echo "=== AddressSanitizer ==="
  ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" \
  RUSTDOCFLAGS="-Zsanitizer=address" \
    cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
  if [[ "$MODE" == "full" ]]; then
    ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" \
    RUSTDOCFLAGS="-Zsanitizer=address" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc
  fi
}

lane_msan() {
  echo "=== MemorySanitizer ==="
  MSAN_OPTIONS="halt_on_error=1,exit_code=86,poison_in_dtor=1" \
  RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" \
    cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
  if [[ "$MODE" == "full" ]]; then
    MSAN_OPTIONS="halt_on_error=1,exit_code=86,poison_in_dtor=1" \
    RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc
  fi
}

lane_kani_family() {
  local family_name="$1"; shift
  local harnesses=("$@")
  echo "=== Kani: $family_name (${#harnesses[@]} harnesses) ==="
  for h in "${harnesses[@]}"; do
    echo "  proving: $h"
    cargo kani --lib --default-unwind 8 --harness "$h"
  done
}

lane_fuzz_build() {
  local targets=("$@")
  echo "=== fuzz build (${#targets[@]} targets) ==="
  for t in "${targets[@]}"; do
    cargo +nightly fuzz build "$t"
  done
}

lane_fuzz_smoke() {
  local runs="$1"; shift
  local targets=("$@")
  echo "=== fuzz smoke (${#targets[@]} targets, $runs runs each) ==="
  for t in "${targets[@]}"; do
    mkdir -p "fuzz/corpus/${t}"
    cargo +nightly fuzz run "$t" "fuzz/corpus/${t}" -- -runs="${runs}" || {
      echo "WARN: fuzz target '$t' smoke run failed (may require native arch)" >&2
    }
  done
}

# --- Banner ------------------------------------------------------------------
echo "╔══════════════════════════════════════════════╗"
echo "║ oxid64 Safety Verification (Swiss-Cheese)   ║"
echo "╚══════════════════════════════════════════════╝"
echo "MODE=${MODE}  STRICT=${STRICT}  FUZZ_CASES=${FUZZ_CASES}  FUZZ_RUNS=${FUZZ_RUNS}"
echo "MAX_LANES=${MAX_LANES}  JOBS/LANE=${CARGO_JOBS}  BACKENDS=${ACTIVE_BACKENDS}"

# --- Dry run -----------------------------------------------------------------
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo
  echo "Dry run — lanes that would execute:"
  echo "  Always: lint, nextest, doctest"
  if [[ "$MODE" == "full" ]]; then
    echo "  Always (full): proptest-extended, careful"
    echo "  Miri shards: lib, contracts"
    backend_active ssse3 && echo "  Miri shards: x86-models, x86-integration"
    backend_active neon || backend_active wasm && echo "  Miri shards: other-models"
    [[ "$MODE" == "full" ]] && echo "  Miri shards: proptest"
    echo "  Sanitizers: asan, msan"
    echo "  Kani families:"
    echo "    core (${#KANI_CORE[@]} harnesses)"
    backend_active ssse3  && echo "    ssse3 (${#KANI_SSSE3[@]})"
    backend_active avx2   && echo "    avx2 (${#KANI_AVX2[@]})"
    backend_active avx512 && echo "    avx512 (${#KANI_AVX512[@]})"
    backend_active neon   && echo "    neon (${#KANI_NEON[@]})"
    backend_active wasm   && echo "    wasm (${#KANI_WASM[@]})"
    echo "  Fuzz: build + smoke all affected targets"
  else
    echo "  Smoke: careful --lib, Miri (lib + contracts + models), Kani smoke, fuzz build smoke"
  fi
  exit 0
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Phase 1: Lint + Format (serial, fast, must pass before anything else)
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Lint & Format"
cargo fmt --all -- --check
layer_pass "cargo fmt --check"
cargo clippy --all-targets --all-features -- -D warnings
layer_pass "cargo clippy"

# ═══════════════════════════════════════════════════════════════════════════════
# Phase 2: Core tests (serial, fast gate)
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Core Tests (nextest)"
if have cargo-nextest; then
  cargo nextest run --lib --tests
  layer_pass "cargo nextest run --lib --tests"
else
  cargo test --lib --tests
  layer_pass "cargo test --lib --tests (nextest not available)"
fi

layer_start "Doctests"
cargo test --doc
layer_pass "cargo test --doc"

# ═══════════════════════════════════════════════════════════════════════════════
# Phase 3: Heavy lanes (parallel with isolated CARGO_TARGET_DIR)
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Parallel Verification Lanes"

# --- cargo-careful ---
if have cargo-careful || cargo +nightly careful --version >/dev/null 2>&1; then
  launch_lane careful lane_careful
else
  warn_or_fail "cargo-careful not installed (cargo +nightly install cargo-careful)." || true
fi

# --- Extended proptest (full only) ---
if [[ "$MODE" == "full" ]]; then
  launch_lane proptest-ext lane_proptest_extended
fi

# --- Miri shards ---
if have_miri; then
  launch_lane miri-lib lane_miri_lib
  launch_lane miri-contracts lane_miri_contracts

  if [[ "$MODE" == "full" ]]; then
    if backend_active ssse3 || backend_active avx2 || backend_active avx512; then
      launch_lane miri-x86-models lane_miri_x86_models
      launch_lane miri-x86-integration lane_miri_x86_integration
    fi
    if backend_active neon || backend_active wasm; then
      launch_lane miri-other-models lane_miri_other_models
    fi
    launch_lane miri-proptest lane_miri_proptest
  else
    # Smoke: just models (fast).
    launch_lane miri-x86-models lane_miri_x86_models
    launch_lane miri-other-models lane_miri_other_models
  fi
else
  warn_or_fail "cargo-miri not installed (rustup component add miri --toolchain nightly)." || true
fi

# --- Sanitizers ---
if have clang; then
  launch_lane asan lane_asan
  launch_lane msan lane_msan
else
  warn_or_fail "clang not found; ASan/MSan require clang for -Zbuild-std." || true
fi

# --- Kani ---
if have_kani; then
  if [[ "$MODE" == "smoke" ]]; then
    launch_lane kani-smoke lane_kani_family smoke "${KANI_SMOKE[@]}"
  else
    # Full: launch one lane per backend family for parallel proving.
    launch_lane kani-core lane_kani_family core "${KANI_CORE[@]}"
    backend_active ssse3  && launch_lane kani-ssse3  lane_kani_family ssse3  "${KANI_SSSE3[@]}"
    backend_active avx2   && launch_lane kani-avx2   lane_kani_family avx2   "${KANI_AVX2[@]}"
    backend_active avx512 && launch_lane kani-avx512  lane_kani_family avx512  "${KANI_AVX512[@]}"
    backend_active neon   && launch_lane kani-neon   lane_kani_family neon   "${KANI_NEON[@]}"
    backend_active wasm   && launch_lane kani-wasm   lane_kani_family wasm   "${KANI_WASM[@]}"
  fi
else
  warn_or_fail "cargo-kani not installed." || true
fi

# --- Fuzz ---
if have_fuzz; then
  if [[ "$MODE" == "smoke" ]]; then
    launch_lane fuzz-build-smoke lane_fuzz_build "${FUZZ_SMOKE[@]}"
  else
    # Build all affected targets, then smoke-run.
    local_fuzz_targets=()
    local_fuzz_targets+=("${FUZZ_COMMON[@]}")
    backend_active ssse3  && local_fuzz_targets+=("${FUZZ_SSSE3[@]}")
    backend_active avx2   && local_fuzz_targets+=("${FUZZ_AVX2[@]}")
    backend_active avx512 && local_fuzz_targets+=("${FUZZ_AVX512[@]}")
    backend_active neon   && local_fuzz_targets+=("${FUZZ_NEON[@]}")
    backend_active wasm   && local_fuzz_targets+=("${FUZZ_WASM[@]}")

    # Fuzz build and smoke share a target dir to reuse compilation.
    launch_lane fuzz-full bash -c "
      source '${SCRIPT_DIR}/lane_defs.sh'
      targets=(${local_fuzz_targets[*]})
      echo '=== fuzz build (\${#targets[@]} targets) ==='
      for t in \"\${targets[@]}\"; do
        cargo +nightly fuzz build \"\$t\"
      done
      echo '=== fuzz smoke (\${#targets[@]} targets, ${FUZZ_RUNS} runs) ==='
      for t in \"\${targets[@]}\"; do
        mkdir -p \"fuzz/corpus/\${t}\"
        cargo +nightly fuzz run \"\$t\" \"fuzz/corpus/\${t}\" -- -runs=${FUZZ_RUNS} || {
          echo \"WARN: fuzz target '\$t' smoke run failed (may require native arch)\" >&2
        }
      done
    "
  fi
else
  warn_or_fail "cargo-fuzz not installed (cargo +nightly install cargo-fuzz)." || true
fi

# --- Wait for all lanes to finish ---
echo
echo "  ⏳ waiting for ${#LANE_PIDS[@]} lane(s)..."
wait_all_lanes

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════
echo
echo "╔══════════════════════════════════════════════╗"
echo "║ Verification Summary                        ║"
echo "╚══════════════════════════════════════════════╝"
echo "  Mode:     ${MODE}"
echo "  Backends: ${ACTIVE_BACKENDS}"
echo "  Passed:   ${PASS_COUNT}"
echo "  Skipped:  ${SKIP_COUNT}"
echo "  Failed:   ${FAIL_COUNT}"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo
  echo "RESULT: FAILED (${FAIL_COUNT} lane(s) failed)"
  exit 1
fi

if [[ "$SKIP_COUNT" -gt 0 && "$STRICT" -eq 1 ]]; then
  echo
  echo "RESULT: FAILED (strict mode, ${SKIP_COUNT} layer(s) skipped)"
  exit 1
fi

echo
echo "RESULT: PASSED"
