#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  scripts/perf_decode_report.sh [options]

Options:
  --core N            CPU core for taskset (default: 2)
  --size N            Decoded size in bytes (default: 1048576)
  --iters N           Iterations per run (default: 40000)
  --runs N            Throughput runs per mode (default: 5)
  --stat-runs N       perf stat repeat count (default: 3)
  --freq N            perf record frequency (default: 999)
  --target-dir PATH   Cargo target dir (default: /tmp/oxid64_portable)
  --out-dir PATH      Output directory (default: perf_reports)
  --report PATH       Output markdown report path (default: <out-dir>/perf_decode_<timestamp>.md)
  --modes "..."       Space-separated perf_compare modes to run
  --list-modes        Print supported decode modes and exit
  --skip-asm          Skip cargo asm section
  -h, --help          Show this help

Notes:
  - Builds perf_compare in release mode without forcing target-cpu=native.
  - By default runs all supported decode modes exposed by perf_compare.
  - Use --modes to narrow the report to a smaller set.
EOF
}

CORE=2
SIZE=1048576
ITERS=40000
RUNS=5
STAT_RUNS=3
FREQ=999
TARGET_DIR="/tmp/oxid64_portable"
OUT_DIR="perf_reports"
REPORT=""
MODES=""
LIST_MODES=0
SKIP_ASM=0
EVENTS="cycles,instructions,branches,branch-misses,L1-dcache-load-misses,LLC-load-misses"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --core) CORE="$2"; shift 2 ;;
        --size) SIZE="$2"; shift 2 ;;
        --iters) ITERS="$2"; shift 2 ;;
        --runs) RUNS="$2"; shift 2 ;;
        --stat-runs) STAT_RUNS="$2"; shift 2 ;;
        --freq) FREQ="$2"; shift 2 ;;
        --target-dir) TARGET_DIR="$2"; shift 2 ;;
        --out-dir) OUT_DIR="$2"; shift 2 ;;
        --report) REPORT="$2"; shift 2 ;;
        --modes) MODES="$2"; shift 2 ;;
        --list-modes) LIST_MODES=1; shift ;;
        --skip-asm) SKIP_ASM=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 1
            ;;
    esac
done

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing required command: $cmd" >&2
        exit 1
    fi
}

run_cmd() {
    local title="$1"
    shift
    echo "## ${title}"
    echo '```bash'
    printf '%q ' "$@"
    echo
    echo '```'
    "$@"
    echo
}

run_optional_cmd() {
    local title="$1"
    shift
    echo "## ${title}"
    echo '```bash'
    printf '%q ' "$@"
    echo
    echo '```'
    set +e
    "$@"
    local rc=$?
    set -e
    if [[ $rc -ne 0 ]]; then
        echo
        echo "Command exited with status ${rc}"
    fi
    echo
}

mode_symbol() {
    case "$1" in
        oxid64-ssse3-strict-api|oxid64-ssse3-strict-kernel)
            printf '%s\n' 'oxid64::engine::ssse3::decode_engine::decode_ssse3_strict'
            ;;
        oxid64-ssse3-nonstrict-api|oxid64-ssse3-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::ssse3::decode_engine::decode_ssse3'
            ;;
        tb64-ssse3-check)
            printf '%s\n' 'tb64v128dec_b64check'
            ;;
        tb64-ssse3-partial)
            printf '%s\n' 'tb64v128dec'
            ;;
        tb64-ssse3-unchecked)
            printf '%s\n' 'tb64v128dec_nb64check'
            ;;
        oxid64-avx2-strict-api|oxid64-avx2-strict-kernel)
            printf '%s\n' 'oxid64::engine::avx2::decode_engine::decode_avx2_strict'
            ;;
        oxid64-avx2-nonstrict-api|oxid64-avx2-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::avx2::decode_engine::decode_avx2'
            ;;
        oxid64-avx2-unchecked-kernel)
            printf '%s\n' 'oxid64::engine::avx2::decode_engine::decode_avx2_unchecked'
            ;;
        tb64-avx2-check)
            printf '%s\n' 'tb64v256dec_b64check'
            ;;
        tb64-avx2-partial)
            printf '%s\n' 'tb64v256dec'
            ;;
        tb64-avx2-unchecked)
            printf '%s\n' 'tb64v256dec_nb64check'
            ;;
        fastbase64-avx2-check)
            printf '%s\n' 'fast_avx2_base64_decode'
            ;;
        oxid64-avx512-strict-api|oxid64-avx512-strict-kernel)
            printf '%s\n' 'oxid64::engine::avx512vbmi::decode_engine::decode_avx512_strict'
            ;;
        oxid64-avx512-nonstrict-api|oxid64-avx512-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::avx512vbmi::decode_engine::decode_avx512'
            ;;
        tb64-avx512-check)
            printf '%s\n' 'tb64v512dec_b64check'
            ;;
        tb64-avx512-partial)
            printf '%s\n' 'tb64v512dec'
            ;;
        tb64-avx512-unchecked)
            printf '%s\n' 'tb64v512dec_nb64check'
            ;;
        oxid64-neon-strict-api|oxid64-neon-strict-kernel)
            printf '%s\n' 'oxid64::engine::neon::decode_engine::decode_neon_strict'
            ;;
        oxid64-neon-nonstrict-api|oxid64-neon-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::neon::decode_engine::decode_neon'
            ;;
        tb64-neon-check)
            printf '%s\n' 'tb64v128dec_b64check'
            ;;
        tb64-neon-partial)
            printf '%s\n' 'tb64v128dec'
            ;;
        tb64-neon-unchecked)
            printf '%s\n' 'tb64v128dec_nb64check'
            ;;
        *)
            printf '\n'
            ;;
    esac
}

mode_asm_symbol() {
    case "$1" in
        oxid64-ssse3-strict-api|oxid64-ssse3-strict-kernel)
            printf '%s\n' 'oxid64::engine::ssse3::decode_engine::decode_ssse3_strict'
            ;;
        oxid64-ssse3-nonstrict-api|oxid64-ssse3-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::ssse3::decode_engine::decode_ssse3'
            ;;
        oxid64-avx2-strict-api|oxid64-avx2-strict-kernel)
            printf '%s\n' 'oxid64::engine::avx2::decode_engine::decode_avx2_strict'
            ;;
        oxid64-avx2-nonstrict-api|oxid64-avx2-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::avx2::decode_engine::decode_avx2'
            ;;
        oxid64-avx2-unchecked-kernel)
            printf '%s\n' 'oxid64::engine::avx2::decode_engine::decode_avx2_unchecked'
            ;;
        oxid64-avx512-strict-api|oxid64-avx512-strict-kernel)
            printf '%s\n' 'oxid64::engine::avx512vbmi::decode_engine::decode_avx512_strict'
            ;;
        oxid64-avx512-nonstrict-api|oxid64-avx512-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::avx512vbmi::decode_engine::decode_avx512'
            ;;
        oxid64-neon-strict-api|oxid64-neon-strict-kernel)
            printf '%s\n' 'oxid64::engine::neon::decode_engine::decode_neon_strict'
            ;;
        oxid64-neon-nonstrict-api|oxid64-neon-nonstrict-kernel)
            printf '%s\n' 'oxid64::engine::neon::decode_engine::decode_neon'
            ;;
        *)
            printf '\n'
            ;;
    esac
}

sanitize_mode() {
    printf '%s\n' "$1" | tr '/:' '__' | tr '-' '_'
}

contains_mode() {
    local needle="$1"
    shift
    local item
    for item in "$@"; do
        if [[ "$item" == "$needle" ]]; then
            return 0
        fi
    done
    return 1
}

run_throughput() {
    local mode="$1"
    echo "## Throughput ${mode}"
    for ((i = 1; i <= RUNS; i++)); do
        echo "### Run ${i}/${RUNS}"
        echo '```bash'
        printf '%q ' taskset -c "$CORE" "$PERF_BIN" "$mode" "$SIZE" "$ITERS"
        echo
        echo '```'
        taskset -c "$CORE" "$PERF_BIN" "$mode" "$SIZE" "$ITERS"
        echo
    done
}

require_cmd cargo
require_cmd perf
require_cmd taskset

mkdir -p "$OUT_DIR"
ts="$(date +%Y%m%d_%H%M%S)"
if [[ -z "$REPORT" ]]; then
    REPORT="$OUT_DIR/perf_decode_${ts}.md"
fi
mkdir -p "$(dirname "$REPORT")"
touch "$REPORT"

PERF_BIN="$TARGET_DIR/release/perf_compare"

exec > >(tee -a "$REPORT") 2>&1

echo "# Decode Perf Report"
echo
echo "- Date: $(date -Iseconds)"
echo "- Host: $(hostname)"
echo "- Core: ${CORE}"
echo "- Size: ${SIZE}"
echo "- Iterations: ${ITERS}"
echo "- Throughput runs: ${RUNS}"
echo "- perf stat repeats: ${STAT_RUNS}"
echo "- perf record freq: ${FREQ}"
echo "- Cargo target dir: ${TARGET_DIR}"
echo "- Report path: ${REPORT}"
echo

run_cmd "Git revision" git rev-parse --short HEAD
run_cmd "Build perf_compare (portable release)" env CARGO_TARGET_DIR="$TARGET_DIR" cargo build --release --features "c-benchmarks,perf-tools" --bin perf_compare

if [[ ! -x "$PERF_BIN" ]]; then
    echo "Expected binary not found: $PERF_BIN" >&2
    exit 1
fi

mapfile -t SUPPORTED_MODES < <("$PERF_BIN" --supported)

if [[ "$LIST_MODES" -eq 1 ]]; then
    printf '%s\n' "${SUPPORTED_MODES[@]}"
    exit 0
fi

if [[ -n "$MODES" ]]; then
    read -r -a REQUESTED_MODES <<<"$MODES"
else
    # Default: use all supported decode modes reported by perf_compare on this host.
    # Filter to decode-only (exclude encode modes) so the report stays focused.
    REQUESTED_MODES=()
    for mode in "${SUPPORTED_MODES[@]}"; do
        if [[ "$mode" != *"-encode"* ]]; then
            REQUESTED_MODES+=("$mode")
        fi
    done
fi

ACTIVE_MODES=()
for mode in "${REQUESTED_MODES[@]}"; do
    if contains_mode "$mode" "${SUPPORTED_MODES[@]}"; then
        ACTIVE_MODES+=("$mode")
    else
        if [[ -n "$MODES" ]]; then
            echo "Requested mode is not supported on this host: $mode" >&2
            exit 1
        fi
    fi
done

if [[ ${#ACTIVE_MODES[@]} -eq 0 ]]; then
    echo "No supported decode modes selected" >&2
    exit 1
fi

echo "## Active modes"
for mode in "${ACTIVE_MODES[@]}"; do
    echo "- ${mode}"
done
echo

DATA_FILES=()
ASM_SYMBOLS=()
for mode in "${ACTIVE_MODES[@]}"; do
    run_throughput "$mode"

    safe_mode="$(sanitize_mode "$mode")"
    data_file="$OUT_DIR/${safe_mode}_${ts}.data"
    DATA_FILES+=("$data_file")

    run_cmd "perf stat ${mode}" perf stat -r "$STAT_RUNS" -e "$EVENTS" taskset -c "$CORE" "$PERF_BIN" "$mode" "$SIZE" "$ITERS"
    run_cmd "perf record ${mode}" perf record -o "$data_file" -F "$FREQ" -g --call-graph fp -- taskset -c "$CORE" "$PERF_BIN" "$mode" "$SIZE" "$ITERS"
    run_cmd "perf report ${mode} (symbol,dso)" perf report -i "$data_file" --stdio --sort symbol,dso

    symbol="$(mode_symbol "$mode")"
    if [[ -n "$symbol" ]]; then
        run_optional_cmd "perf annotate ${mode}" perf annotate -i "$data_file" --stdio --symbol "$symbol"
    fi

    asm_symbol="$(mode_asm_symbol "$mode")"
    if [[ -n "$asm_symbol" ]] && ! contains_mode "$asm_symbol" "${ASM_SYMBOLS[@]}"; then
        ASM_SYMBOLS+=("$asm_symbol")
    fi
done

if [[ "$SKIP_ASM" -eq 0 ]]; then
    echo "## cargo asm"
    echo
    for asm_symbol in "${ASM_SYMBOLS[@]}"; do
        safe_symbol="$(sanitize_mode "$asm_symbol")"
        asm_file="$OUT_DIR/${safe_symbol}_${ts}.s"
        run_optional_cmd "cargo asm dump ${asm_symbol}" env CARGO_TARGET_DIR="$TARGET_DIR" cargo asm --lib --intel "$asm_symbol"
        run_optional_cmd "cargo asm saved copy ${asm_symbol}" env CARGO_TARGET_DIR="$TARGET_DIR" bash -lc "cargo asm --lib --intel '$asm_symbol' > '$asm_file'"
    done
else
    echo "## cargo asm"
    echo "Skipped (--skip-asm)"
    echo
fi

echo "## Artifacts"
for data_file in "${DATA_FILES[@]}"; do
    printf -- '- perf.data: `%s`\n' "$data_file"
done
if [[ "$SKIP_ASM" -eq 0 ]]; then
    for asm_symbol in "${ASM_SYMBOLS[@]}"; do
        safe_symbol="$(sanitize_mode "$asm_symbol")"
        printf -- '- cargo asm: `%s`\n' "$OUT_DIR/${safe_symbol}_${ts}.s"
    done
fi
echo
echo "Report completed: $REPORT"
