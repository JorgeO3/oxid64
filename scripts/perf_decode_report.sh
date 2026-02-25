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
  --skip-asm          Skip cargo asm section
  -h, --help          Show this help

Notes:
  - Builds in release mode without forcing target-cpu=native.
  - Captures rust decoder (rustdec) and Turbo C fast decoder (cfastdec).
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
    local c="$1"
    if ! command -v "$c" >/dev/null 2>&1; then
        echo "Missing required command: $c" >&2
        exit 1
    fi
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

rust_data="$OUT_DIR/rustdec_${ts}.data"
cfast_data="$OUT_DIR/cfastdec_${ts}.data"
asm_file="$OUT_DIR/asm_rustdec_${ts}.s"
perf_bin="$TARGET_DIR/release/perf_compare"

exec > >(tee -a "$REPORT") 2>&1

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

run_throughput() {
    local mode="$1"
    echo "## Throughput ${mode}"
    for ((i = 1; i <= RUNS; i++)); do
        echo "### Run ${i}/${RUNS}"
        echo '```bash'
        printf '%q ' taskset -c "$CORE" "$perf_bin" "$mode" "$SIZE" "$ITERS"
        echo
        echo '```'
        taskset -c "$CORE" "$perf_bin" "$mode" "$SIZE" "$ITERS"
        echo
    done
}

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
run_cmd "Build perf_compare (portable release)" env CARGO_TARGET_DIR="$TARGET_DIR" cargo build --release --bin perf_compare

if [[ ! -x "$perf_bin" ]]; then
    echo "Expected binary not found: $perf_bin" >&2
    exit 1
fi

run_throughput rustdec
run_throughput cfastdec

run_cmd "perf stat rustdec" perf stat -r "$STAT_RUNS" -e "$EVENTS" taskset -c "$CORE" "$perf_bin" rustdec "$SIZE" "$ITERS"
run_cmd "perf stat cfastdec" perf stat -r "$STAT_RUNS" -e "$EVENTS" taskset -c "$CORE" "$perf_bin" cfastdec "$SIZE" "$ITERS"

run_cmd "perf record rustdec" perf record -o "$rust_data" -F "$FREQ" -g --call-graph fp -- taskset -c "$CORE" "$perf_bin" rustdec "$SIZE" "$ITERS"
run_cmd "perf report rustdec (symbol,dso)" perf report -i "$rust_data" --stdio --sort symbol,dso
run_cmd "perf annotate rustdec hot symbol" perf annotate -i "$rust_data" --stdio --symbol "oxid64::scalar::decode_base64_fast"

run_cmd "perf record cfastdec" perf record -o "$cfast_data" -F "$FREQ" -g --call-graph fp -- taskset -c "$CORE" "$perf_bin" cfastdec "$SIZE" "$ITERS"
run_cmd "perf report cfastdec (symbol,dso)" perf report -i "$cfast_data" --stdio --sort symbol,dso
run_cmd "perf annotate cfastdec hot symbol" perf annotate -i "$cfast_data" --stdio --symbol "tb64xdec"

if [[ "$SKIP_ASM" -eq 0 ]]; then
    run_cmd "cargo asm dump (rust decode)" env CARGO_TARGET_DIR="$TARGET_DIR" cargo asm --lib --intel oxid64::scalar::decode_base64_fast
    run_cmd "cargo asm saved copy" env CARGO_TARGET_DIR="$TARGET_DIR" bash -lc "cargo asm --lib --intel oxid64::scalar::decode_base64_fast > '$asm_file'"
    run_cmd "cargo asm first 220 lines" sed -n '1,220p' "$asm_file"
else
    echo "## cargo asm"
    echo "Skipped (--skip-asm)"
    echo
fi

echo "## Artifacts"
echo "- rustdec perf.data: \`$rust_data\`"
echo "- cfastdec perf.data: \`$cfast_data\`"
if [[ "$SKIP_ASM" -eq 0 ]]; then
    echo "- rust asm dump: \`$asm_file\`"
fi
echo
echo "Report completed: $REPORT"
