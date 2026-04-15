#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  scripts/sse_strict_harness.sh [options]

Options:
  --variant KEY       Variant key to run (repeatable). Default: strict,resynth_a3,resynth_a3_single,resynth_add4
  --core N            CPU core for taskset (default: 0)
  --size N            Criterion size suffix (default: 1048576)
  --stat-runs N       perf stat repeats (default: 3)
  --events LIST       perf events (default: cycles,instructions,branches,branch-misses,cache-misses)
  --out-dir PATH      Output directory (default: perf_reports/sse_strict)
  --report PATH       Output markdown report (default: <out-dir>/sse_strict_harness_<timestamp>.md)
  --skip-asm          Skip cargo asm dumps and instruction counting
  --skip-bench        Skip cargo bench exact runs
  --skip-perf         Skip perf stat runs
  -h, --help          Show this help

Variant keys:
  strict
  strict_range
  resynth_a3
  resynth_a3_single
  resynth_add4
  hybrid
  arith
  ptest_mask
  ptest_nomask
  sse42_pcmpestrm

Notes:
  - Runs in series (no parallel benches) to avoid noisy cross-interference.
  - Applies quick asm gates:
      * potential constant reload pressure via [rip + .LCPI] count
      * potential spills via [rsp + ...] references
USAGE
}

require_cmd() {
    local c="$1"
    if ! command -v "$c" >/dev/null 2>&1; then
        echo "Missing required command: $c" >&2
        exit 1
    fi
}

CORE=0
SIZE=1048576
STAT_RUNS=3
EVENTS="cycles,instructions,branches,branch-misses,cache-misses"
OUT_DIR="perf_reports/sse_strict"
REPORT=""
SKIP_ASM=0
SKIP_BENCH=0
SKIP_PERF=0

VARIANTS=()

declare -A BENCH_LABEL
declare -A ASM_SYMBOL

BENCH_LABEL["strict"]="Base64 Decoding/Rust Port (SSSE3, C-Style Strict)"
ASM_SYMBOL["strict"]="oxid64::simd::ssse3_cstyle::ssse3_cstyle_engine::decode_base64_ssse3_cstyle_strict"

BENCH_LABEL["strict_range"]="Base64 Decoding/Rust Port (SSSE3, C-Style Strict Range)"
ASM_SYMBOL["strict_range"]="oxid64::simd::ssse3_cstyle::ssse3_cstyle_engine::decode_base64_ssse3_cstyle_strict_range"

BENCH_LABEL["resynth_a3"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Resynth A3)"
ASM_SYMBOL["resynth_a3"]="oxid64::simd::ssse3_cstyle_experiments_hybrid::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_resynth_a3"

BENCH_LABEL["resynth_a3_single"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Resynth A3 Single)"
ASM_SYMBOL["resynth_a3_single"]="oxid64::simd::ssse3_cstyle_experiments_hybrid::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_resynth_a3_single"

BENCH_LABEL["resynth_add4"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Resynth Add4)"
ASM_SYMBOL["resynth_add4"]="oxid64::simd::ssse3_cstyle_experiments_hybrid::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_resynth_add4"

BENCH_LABEL["hybrid"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Hybrid Buckets)"
ASM_SYMBOL["hybrid"]="oxid64::simd::ssse3_cstyle_experiments_hybrid::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_hybrid"

BENCH_LABEL["arith"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Arith Check)"
ASM_SYMBOL["arith"]="oxid64::simd::ssse3_cstyle_experiments::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_arithcheck"

BENCH_LABEL["ptest_mask"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict PTEST Mask)"
ASM_SYMBOL["ptest_mask"]="oxid64::simd::ssse3_cstyle_experiments::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_mask"

BENCH_LABEL["ptest_nomask"]="Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict PTEST NoMask)"
ASM_SYMBOL["ptest_nomask"]="oxid64::simd::ssse3_cstyle_experiments::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse41_nomask"

BENCH_LABEL["sse42_pcmpestrm"]="Base64 Decoding/Rust Port (SSSE3+SSE4.2, C-Style Strict PCMPESTRM)"
ASM_SYMBOL["sse42_pcmpestrm"]="oxid64::simd::ssse3_cstyle_experiments_hybrid::ssse3_cstyle_exp_engine::decode_base64_ssse3_cstyle_strict_sse42_pcmpestrm"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --variant)
            VARIANTS+=("$2")
            shift 2
            ;;
        --core)
            CORE="$2"
            shift 2
            ;;
        --size)
            SIZE="$2"
            shift 2
            ;;
        --stat-runs)
            STAT_RUNS="$2"
            shift 2
            ;;
        --events)
            EVENTS="$2"
            shift 2
            ;;
        --out-dir)
            OUT_DIR="$2"
            shift 2
            ;;
        --report)
            REPORT="$2"
            shift 2
            ;;
        --skip-asm)
            SKIP_ASM=1
            shift
            ;;
        --skip-bench)
            SKIP_BENCH=1
            shift
            ;;
        --skip-perf)
            SKIP_PERF=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 1
            ;;
    esac
done

if [[ ${#VARIANTS[@]} -eq 0 ]]; then
    VARIANTS=("strict" "resynth_a3" "resynth_a3_single" "resynth_add4")
fi

for v in "${VARIANTS[@]}"; do
    if [[ -z "${BENCH_LABEL[$v]:-}" || -z "${ASM_SYMBOL[$v]:-}" ]]; then
        echo "Unknown variant key: $v" >&2
        exit 1
    fi
done

require_cmd cargo
require_cmd rg
require_cmd taskset
if [[ "$SKIP_PERF" -eq 0 ]]; then
    require_cmd perf
fi

mkdir -p "$OUT_DIR"
ts="$(date +%Y%m%d_%H%M%S)"
if [[ -z "$REPORT" ]]; then
    REPORT="$OUT_DIR/sse_strict_harness_${ts}.md"
fi
mkdir -p "$(dirname "$REPORT")"
touch "$REPORT"

BENCH_BIN=""
if [[ "$SKIP_PERF" -eq 0 ]]; then
    cargo bench --features c-benchmarks --bench base64_bench --no-run >/dev/null
    BENCH_BIN="$(ls -t target/release/deps/base64_bench-* | head -n1)"
fi

declare -A THRPT_MID
declare -A ASM_PSHUFB
declare -A ASM_PMADDUBSW
declare -A ASM_PMADDWD
declare -A ASM_PMOVMSKB
declare -A ASM_MOVDQU
declare -A ASM_MOVDQA
declare -A ASM_LCPI
declare -A ASM_RSP
declare -A ASM_XMM_STACK
declare -A ASM_GATE

echo "# SSE Strict Harness Report" > "$REPORT"
{
    echo
    echo "- Date: $(date -Iseconds)"
    echo "- Host: $(hostname)"
    echo "- Core: ${CORE}"
    echo "- Size: ${SIZE}"
    echo "- Variants: ${VARIANTS[*]}"
    echo "- Skip asm: ${SKIP_ASM}"
    echo "- Skip bench: ${SKIP_BENCH}"
    echo "- Skip perf: ${SKIP_PERF}"
    if [[ -n "$BENCH_BIN" ]]; then
        printf -- '- Bench binary: `%s`\n' "$BENCH_BIN"
    fi
    echo
} >> "$REPORT"

count_inst() {
    local asm_file="$1"
    local mnem="$2"
    local c
    c=$( (rg -o "\\b${mnem}\\b" "$asm_file" || true) | wc -l | tr -d '[:space:]' )
    echo "$c"
}

extract_thrpt_mid() {
    local bench_log="$1"
    local mid
    mid=$(sed -n 's/.*thrpt:  \[[^ ]* GiB\/s \([^ ]*\) GiB\/s [^]]*\].*/\1/p' "$bench_log" | tail -n1)
    if [[ -z "$mid" ]]; then
        mid="NA"
    fi
    echo "$mid"
}

dump_asm() {
    local symbol="$1"
    local out_file="$2"
    local listing
    local idx

    if cargo asm --release --bench base64_bench "$symbol" > "$out_file" 2>/dev/null; then
        return 0
    fi

    listing="$(cargo asm --release --bench base64_bench "$symbol" 2>&1 || true)"
    idx="$(printf '%s\n' "$listing" | grep -F "\"$symbol\" [" | head -n1 | sed -n 's/^\([0-9]\+\).*/\1/p')"

    if [[ -z "$idx" ]]; then
        echo "Unable to resolve cargo asm symbol index for: $symbol" >&2
        printf '%s\n' "$listing" >&2
        return 1
    fi

    cargo asm --release --bench base64_bench "$symbol" "$idx" > "$out_file"
}

run_one_variant() {
    local key="$1"
    local label="${BENCH_LABEL[$key]}"
    local symbol="${ASM_SYMBOL[$key]}"
    local target="${label}/${SIZE}"

    local asm_file="$OUT_DIR/${ts}_${key}.asm.s"
    local bench_log="$OUT_DIR/${ts}_${key}.bench.txt"
    local perf_log="$OUT_DIR/${ts}_${key}.perf_stat.txt"

    {
        echo "## Variant: ${key}"
        echo
        printf -- '- Bench label: `%s`\n' "$label"
        printf -- '- ASM symbol: `%s`\n' "$symbol"
        echo
    } >> "$REPORT"

    if [[ "$SKIP_ASM" -eq 0 ]]; then
        {
            echo "### cargo asm"
            echo
            echo '```bash'
            printf 'cargo asm --release --bench base64_bench %q > %q\n' "$symbol" "$asm_file"
            echo '```'
        } >> "$REPORT"

        dump_asm "$symbol" "$asm_file"

        ASM_PSHUFB[$key]="$(count_inst "$asm_file" pshufb)"
        ASM_PMADDUBSW[$key]="$(count_inst "$asm_file" pmaddubsw)"
        ASM_PMADDWD[$key]="$(count_inst "$asm_file" pmaddwd)"
        ASM_PMOVMSKB[$key]="$(count_inst "$asm_file" pmovmskb)"
        ASM_MOVDQU[$key]="$(count_inst "$asm_file" movdqu)"
        ASM_MOVDQA[$key]="$(count_inst "$asm_file" movdqa)"
        ASM_LCPI[$key]="$( (rg -o '\[rip \+ \.LCPI' "$asm_file" || true) | wc -l | tr -d '[:space:]' )"
        ASM_RSP[$key]="$( (rg -o '\[rsp[^]]*\]' "$asm_file" || true) | wc -l | tr -d '[:space:]' )"
        ASM_XMM_STACK[$key]="$( (rg -o -i 'xmmword ptr \[rsp' "$asm_file" || true) | wc -l | tr -d '[:space:]' )"

        local gate="PASS"
        if [[ "${ASM_XMM_STACK[$key]}" != "0" ]]; then
            gate="POTENTIAL_SPILL"
        fi
        if [[ "${ASM_LCPI[$key]}" -gt 28 ]]; then
            if [[ "$gate" == "PASS" ]]; then
                gate="POTENTIAL_RELOAD"
            else
                gate="POTENTIAL_SPILL+RELOAD"
            fi
        fi
        ASM_GATE[$key]="$gate"

        {
            echo
            printf -- '- asm file: `%s`\n' "$asm_file"
            echo "- counts: pshufb=${ASM_PSHUFB[$key]}, pmaddubsw=${ASM_PMADDUBSW[$key]}, pmaddwd=${ASM_PMADDWD[$key]}, pmovmskb=${ASM_PMOVMSKB[$key]}, movdqu=${ASM_MOVDQU[$key]}, movdqa=${ASM_MOVDQA[$key]}"
            echo "- gates: LCPI_refs=${ASM_LCPI[$key]}, rsp_refs=${ASM_RSP[$key]}, xmm_stack_refs=${ASM_XMM_STACK[$key]}, status=${ASM_GATE[$key]}"
            echo
        } >> "$REPORT"
    fi

    if [[ "$SKIP_BENCH" -eq 0 ]]; then
        {
            echo "### cargo bench (exact)"
            echo
            echo '```bash'
            printf 'taskset -c %q cargo bench --features c-benchmarks --bench base64_bench -- %q --exact --noplot\n' "$CORE" "$target"
            echo '```'
            echo
        } >> "$REPORT"

        taskset -c "$CORE" cargo bench --features c-benchmarks --bench base64_bench -- "$target" --exact --noplot \
            > "$bench_log" 2>&1

        THRPT_MID[$key]="$(extract_thrpt_mid "$bench_log")"

        {
            echo "- throughput_mid_gib_s: ${THRPT_MID[$key]}"
            echo
            echo '<details><summary>bench output</summary>'
            echo
            echo '```text'
            cat "$bench_log"
            echo '```'
            echo
            echo '</details>'
            echo
        } >> "$REPORT"
    fi

    if [[ "$SKIP_PERF" -eq 0 ]]; then
        {
            echo "### perf stat"
            echo
            echo '```bash'
            printf 'perf stat -r %q -e %q -- taskset -c %q %q %q --exact --noplot\n' "$STAT_RUNS" "$EVENTS" "$CORE" "$BENCH_BIN" "$target"
            echo '```'
            echo
        } >> "$REPORT"

        perf stat -r "$STAT_RUNS" -e "$EVENTS" -- taskset -c "$CORE" "$BENCH_BIN" "$target" --exact --noplot \
            > "$perf_log" 2>&1

        {
            echo '<details><summary>perf stat output</summary>'
            echo
            echo '```text'
            cat "$perf_log"
            echo '```'
            echo
            echo '</details>'
            echo
        } >> "$REPORT"
    fi
}

for v in "${VARIANTS[@]}"; do
    run_one_variant "$v"
done

{
    echo "## Summary"
    echo
    echo "| variant | thrpt_mid_gib_s | pshufb | pmaddubsw | pmaddwd | pmovmskb | movdqu | movdqa | LCPI refs | rsp refs | xmm stack refs | gate |"
    echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|"
    for v in "${VARIANTS[@]}"; do
        echo "| ${v} | ${THRPT_MID[$v]:-NA} | ${ASM_PSHUFB[$v]:-NA} | ${ASM_PMADDUBSW[$v]:-NA} | ${ASM_PMADDWD[$v]:-NA} | ${ASM_PMOVMSKB[$v]:-NA} | ${ASM_MOVDQU[$v]:-NA} | ${ASM_MOVDQA[$v]:-NA} | ${ASM_LCPI[$v]:-NA} | ${ASM_RSP[$v]:-NA} | ${ASM_XMM_STACK[$v]:-NA} | ${ASM_GATE[$v]:-NA} |"
    done
    echo
} >> "$REPORT"

echo "Report completed: $REPORT"
