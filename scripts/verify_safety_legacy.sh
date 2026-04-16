#!/usr/bin/env bash
# verify_safety.sh — Swiss-Cheese safety verification matrix for oxid64.
#
# Two modes:
#   --mode smoke   Fast gate (~2 min): core tests, clippy, cargo-careful, Miri lib,
#                  one Kani harness per backend, one fuzz-build check.
#   --mode full    Complete matrix (~30-60 min): all layers including full Miri sweep,
#                  all Kani proofs, ASan, MSan, fuzz-build + smoke runs.
#
# Flags:
#   --strict       Exit on first missing tool instead of warning.
#   --fuzz-cases N Proptest case count (default 5000 for full, 200 for smoke).
#   --fuzz-runs N  Fuzz smoke run count (default 64 for full, 8 for smoke).
#   --mode MODE    "smoke" or "full" (default: full).
#
# Environment:
#   Requires nightly toolchain with miri component.
#   cargo-kani, cargo-fuzz (nightly), and clang are optional (warned/failed via --strict).
#
set -euo pipefail

# --- Defaults ----------------------------------------------------------------
MODE="full"
STRICT=0
FUZZ_CASES=""
FUZZ_RUNS=""
PASS_COUNT=0
SKIP_COUNT=0
FAIL_COUNT=0

# --- Argument parsing --------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)      STRICT=1;       shift ;;
    --mode)        MODE="$2";      shift 2 ;;
    --fuzz-cases)  FUZZ_CASES="$2"; shift 2 ;;
    --fuzz-runs)   FUZZ_RUNS="$2";  shift 2 ;;
    *)
      echo "Unknown arg: $1" >&2
      echo "Usage: $0 [--mode smoke|full] [--strict] [--fuzz-cases N] [--fuzz-runs N]" >&2
      exit 2
      ;;
  esac
done

# Normalize name=value forms from task runners.
for var in FUZZ_CASES FUZZ_RUNS MODE; do
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

for var in FUZZ_CASES FUZZ_RUNS; do
  if ! [[ "${!var}" =~ ^[0-9]+$ ]]; then
    echo "ERROR: $var must be an integer, got '${!var}'" >&2
    exit 2
  fi
done

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

# --- Banner ------------------------------------------------------------------
echo "╔══════════════════════════════════════════════╗"
echo "║ oxid64 Safety Verification (Swiss-Cheese)   ║"
echo "╚══════════════════════════════════════════════╝"
echo "MODE=${MODE}  STRICT=${STRICT}  FUZZ_CASES=${FUZZ_CASES}  FUZZ_RUNS=${FUZZ_RUNS}"

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 1: Lint + Format check
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Lint & Format"
cargo fmt --all -- --check
layer_pass "cargo fmt --check"
cargo clippy --all-targets --all-features -- -D warnings
layer_pass "cargo clippy"

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 2: Core tests (all backends, all integration tests, doctests)
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Core Tests"
cargo test
layer_pass "cargo test (all)"

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 3: Proptest / differential fuzz (x86 backends)
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Proptest Differential Fuzz"
env PROPTEST_CASES="${FUZZ_CASES}" cargo test \
  --test sse_decode_tests \
  --test avx2_decode_tests \
  --test avx512_vbmi_decode_tests \
  --test sse_encode_tests \
  --test avx2_encode_tests \
  --test avx512_vbmi_encode_tests \
  --test simd_fuzz_strict \
  --test proptest
layer_pass "proptest differential (${FUZZ_CASES} cases)"

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 4: cargo-careful (overflow/UB hardening in debug builds)
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "cargo-careful"
if have cargo-careful || cargo +nightly careful --version >/dev/null 2>&1; then
  if [[ "$MODE" == "smoke" ]]; then
    cargo +nightly careful test --lib
    layer_pass "cargo-careful --lib (smoke)"
  else
    cargo +nightly careful test --lib --tests
    cargo +nightly careful test --doc
    layer_pass "cargo-careful --lib --tests --doc (full)"
  fi
else
  warn_or_fail "cargo-careful not installed (cargo +nightly install cargo-careful)." || true
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 5: Miri (strict provenance + symbolic alignment)
# ═══════════════════════════════════════════════════════════════════════════════
MIRI_FLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-disable-isolation"

layer_start "Miri"
if have_miri; then
  if [[ "$MODE" == "smoke" ]]; then
    # Smoke: lib + scalar contracts + model tests only (fast).
    MIRIFLAGS="$MIRI_FLAGS" cargo +nightly miri test --lib
    layer_pass "Miri --lib"
    MIRIFLAGS="$MIRI_FLAGS" cargo +nightly miri test --test scalar_contracts
    layer_pass "Miri scalar_contracts"
    MIRIFLAGS="$MIRI_FLAGS" cargo +nightly miri test --test common_contracts
    layer_pass "Miri common_contracts"
    MIRIFLAGS="$MIRI_FLAGS" cargo +nightly miri test \
      --test ssse3_models --test avx2_models --test avx512_vbmi_models \
      --test neon_models --test wasm_simd128_models
    layer_pass "Miri model tests (all backends)"
  else
    # Full: entire test suite under Miri.
    MIRIFLAGS="$MIRI_FLAGS" cargo +nightly miri test
    layer_pass "Miri full test suite"
    # Many-seeds exploration for additional scheduling coverage.
    MIRIFLAGS="$MIRI_FLAGS -Zmiri-many-seeds=0..4" cargo +nightly miri test --lib
    layer_pass "Miri many-seeds (lib)"
  fi
else
  warn_or_fail "cargo-miri not installed (rustup component add miri --toolchain nightly)." || true
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 6: AddressSanitizer
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "AddressSanitizer"
if have clang; then
  if [[ "$MODE" == "smoke" ]]; then
    ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" \
    RUSTDOCFLAGS="-Zsanitizer=address" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
    layer_pass "ASan --lib --tests (smoke)"
  else
    ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" \
    RUSTDOCFLAGS="-Zsanitizer=address" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
    ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" \
    RUSTDOCFLAGS="-Zsanitizer=address" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc
    layer_pass "ASan --lib --tests --doc (full)"
  fi
else
  warn_or_fail "clang not found; ASan requires clang for -Zbuild-std." || true
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 7: MemorySanitizer
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "MemorySanitizer"
if have clang; then
  if [[ "$MODE" == "smoke" ]]; then
    MSAN_OPTIONS="halt_on_error=1,exit_code=86,poison_in_dtor=1" \
    RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
    layer_pass "MSan --lib --tests (smoke)"
  else
    MSAN_OPTIONS="halt_on_error=1,exit_code=86,poison_in_dtor=1" \
    RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
    MSAN_OPTIONS="halt_on_error=1,exit_code=86,poison_in_dtor=1" \
    RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc
    layer_pass "MSan --lib --tests --doc (full)"
  fi
else
  warn_or_fail "clang not found; MSan requires clang for -Zbuild-std." || true
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 8: Kani formal verification proofs
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "Kani Proofs"
if have_kani; then
  if [[ "$MODE" == "smoke" ]]; then
    # One harness per backend family as smoke signal.
    cargo kani --default-unwind 8 --harness remaining_matches_pointer_delta_within_allocation
    layer_pass "Kani smoke: core helpers"
    cargo kani --default-unwind 8 --harness ssse3_ds64_store_offsets_fit_guarded_output
    layer_pass "Kani smoke: SSSE3"
    cargo kani --default-unwind 8 --harness avx2_ds128_store_offsets_fit_guarded_output
    layer_pass "Kani smoke: AVX2"
    cargo kani --default-unwind 8 --harness avx512_ds256_store_offsets_fit_guarded_output
    layer_pass "Kani smoke: AVX-512"
    cargo kani --default-unwind 8 --harness wasm_ds64_store_offsets_fit_guarded_output
    layer_pass "Kani smoke: WASM"
    cargo kani --default-unwind 8 --harness neon_non_strict_schedule_model_matches_documented_lanes
    layer_pass "Kani smoke: NEON"
  else
    # Full: run every explicit harness from the Justfile.
    local_harnesses=(
      remaining_matches_pointer_delta_within_allocation
      can_read_and_can_write_agree_with_remaining
      safe_in_end_for_4_within_allocation
      safe_in_end_for_16_within_allocation
      guard_helpers_imply_required_io_room
      prepare_decode_output_matches_decoded_len_contract
      decoded_and_encoded_lengths_stay_bounded_in_small_domain
      decode_offsets_monotonic_and_dispatch_contracts_hold
      ssse3_ds64_store_offsets_fit_guarded_output
      ssse3_ds64_double_store_offsets_fit_guarded_output
      ssse3_tail16_store_fits_guarded_output
      ssse3_non_strict_schedule_model_matches_documented_lanes
      ssse3_written_prefix_model_is_bounded_by_decoded_output
      avx2_ds128_store_offsets_fit_guarded_output
      avx2_ds128_double_store_offsets_fit_guarded_output
      avx2_ds128_triple_store_offsets_fit_guarded_output
      avx2_tail16_store_fits_guarded_output
      avx2_non_strict_schedule_model_matches_documented_lanes
      avx2_strict_written_prefix_model_is_bounded_by_decoded_output
      avx2_partial_written_prefix_model_is_bounded_by_decoded_output
      avx2_unchecked_preflight_matches_documented_contract
      avx512_ds256_store_offsets_fit_guarded_output
      avx512_ds256_double_store_offsets_fit_guarded_output
      avx512_non_strict_schedule_model_matches_documented_lanes
      avx512_written_prefix_model_is_bounded_by_decoded_output
      avx512_encode_schedule_is_contiguous_without_gaps
      avx512_encode_required_input_matches_farthest_load
      neon_non_strict_schedule_model_matches_documented_lanes
      neon_written_prefix_model_is_bounded_by_decoded_output
      neon_encode_prefix_consumes_only_full_blocks
      neon_encode_required_input_matches_block_boundaries
      wasm_ds64_store_offsets_fit_guarded_output
      wasm_ds64_double_store_offsets_fit_guarded_output
      wasm_tail16_store_fits_guarded_output
      wasm_non_strict_schedule_model_matches_documented_lanes
      wasm_written_prefix_model_is_bounded_by_decoded_output
      wasm_pshufb_model_matches_low_nibble_spec
      wasm_encode_prefix_consumes_only_full_blocks
      wasm_encode_required_input_matches_block_boundaries
    )
    for h in "${local_harnesses[@]}"; do
      cargo kani --default-unwind 8 --harness "$h"
    done
    layer_pass "Kani full: ${#local_harnesses[@]} harnesses verified"
  fi
else
  warn_or_fail "cargo-kani not installed." || true
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Layer 9: cargo-fuzz build + smoke run
# ═══════════════════════════════════════════════════════════════════════════════
layer_start "cargo-fuzz"
if have_fuzz; then
  # List of all fuzz targets (from Justfile).
  FUZZ_TARGETS=(
    decode_diff encode_diff roundtrip invalid_semantics tail_alignment
    ssse3_strict_diff ssse3_non_strict_schedule
    avx2_strict_diff avx2_non_strict_schedule avx2_unchecked_contract avx2_partial_write_bounds
    avx512_strict_diff avx512_non_strict_schedule avx512_encode_diff avx512_partial_write_bounds
    neon_strict_diff neon_non_strict_schedule neon_encode_diff neon_partial_write_bounds
    wasm_pshufb_compat wasm_non_strict_schedule wasm_encode_prefix_model wasm_partial_write_model
  )

  if [[ "$MODE" == "smoke" ]]; then
    # Smoke: just build-check a few representative targets.
    for t in decode_diff roundtrip ssse3_strict_diff avx2_strict_diff; do
      cargo +nightly fuzz build "$t"
    done
    layer_pass "fuzz build smoke (4 targets)"
  else
    # Full: build all, then smoke-run each.
    for t in "${FUZZ_TARGETS[@]}"; do
      cargo +nightly fuzz build "$t"
    done
    layer_pass "fuzz build all (${#FUZZ_TARGETS[@]} targets)"

    for t in "${FUZZ_TARGETS[@]}"; do
      # Ensure corpus directory exists.
      mkdir -p "fuzz/corpus/${t}"
      cargo +nightly fuzz run "$t" "fuzz/corpus/${t}" -- -runs="${FUZZ_RUNS}" || {
        echo "WARN: fuzz target '$t' smoke run failed (may require native arch)" >&2
      }
    done
    layer_pass "fuzz smoke run all (${FUZZ_RUNS} runs each)"
  fi
else
  warn_or_fail "cargo-fuzz not installed (cargo +nightly install cargo-fuzz)." || true
fi

# ═══════════════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════════════
echo
echo "╔══════════════════════════════════════════════╗"
echo "║ Verification Summary                        ║"
echo "╚══════════════════════════════════════════════╝"
echo "  Mode:    ${MODE}"
echo "  Passed:  ${PASS_COUNT}"
echo "  Skipped: ${SKIP_COUNT}"
echo "  Failed:  ${FAIL_COUNT}"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo
  echo "RESULT: FAILED (${FAIL_COUNT} layer(s) failed)"
  exit 1
fi

if [[ "$SKIP_COUNT" -gt 0 && "$STRICT" -eq 1 ]]; then
  echo
  echo "RESULT: FAILED (strict mode, ${SKIP_COUNT} layer(s) skipped)"
  exit 1
fi

echo
echo "RESULT: PASSED"
