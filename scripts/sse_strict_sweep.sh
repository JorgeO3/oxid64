#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  scripts/sse_strict_sweep.sh [options]

Options:
  --variant KEY       Variant key to include (repeatable).
  --rounds N          Criterion rounds per variant (default: 3)
  --top-k N           Number of top variants in final ranking (default: 3)
  --core N            CPU core for taskset (default: 0)
  --size N            Criterion size suffix (default: 1048576)
  --out-dir PATH      Output directory (default: perf_reports/sse_sweep)
  --report PATH       Final markdown report path (default: <out-dir>/sse_sweep_<timestamp>.md)
  --harness PATH      Harness script path (default: scripts/sse_strict_harness.sh)
  --skip-asm          Skip ASM in all rounds (faster, no gates)
  -h, --help          Show this help

Default variants (if none passed):
  strict strict_range resynth_a3 resynth_a3_single resynth_add4 ptest_mask ptest_nomask sse42_pcmpestrm arith hybrid

Notes:
  - Runs variants in series and rounds in series (stable signal over speed).
  - ASM is collected only on round 1 by default; rounds >1 use --skip-asm.
  - Uses median throughput across rounds to rank variants.
USAGE
}

require_cmd() {
    local c="$1"
    if ! command -v "$c" >/dev/null 2>&1; then
        echo "Missing required command: $c" >&2
        exit 1
    fi
}

ROUNDS=3
TOP_K=3
CORE=0
SIZE=1048576
OUT_DIR="perf_reports/sse_sweep"
REPORT=""
HARNESS="scripts/sse_strict_harness.sh"
SKIP_ASM_ALL=0
VARIANTS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --variant)
            VARIANTS+=("$2")
            shift 2
            ;;
        --rounds)
            ROUNDS="$2"
            shift 2
            ;;
        --top-k)
            TOP_K="$2"
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
        --out-dir)
            OUT_DIR="$2"
            shift 2
            ;;
        --report)
            REPORT="$2"
            shift 2
            ;;
        --harness)
            HARNESS="$2"
            shift 2
            ;;
        --skip-asm)
            SKIP_ASM_ALL=1
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
    VARIANTS=(
        "strict"
        "strict_range"
        "resynth_a3"
        "resynth_a3_single"
        "resynth_add4"
        "ptest_mask"
        "ptest_nomask"
        "sse42_pcmpestrm"
        "arith"
        "hybrid"
    )
fi

if [[ ! -x "$HARNESS" ]]; then
    echo "Harness not executable or missing: $HARNESS" >&2
    exit 1
fi

if ! [[ "$ROUNDS" =~ ^[0-9]+$ ]] || [[ "$ROUNDS" -lt 1 ]]; then
    echo "--rounds must be >= 1" >&2
    exit 1
fi
if ! [[ "$TOP_K" =~ ^[0-9]+$ ]] || [[ "$TOP_K" -lt 1 ]]; then
    echo "--top-k must be >= 1" >&2
    exit 1
fi

require_cmd awk
require_cmd sed
require_cmd rg
require_cmd sort

mkdir -p "$OUT_DIR"
ts="$(date +%Y%m%d_%H%M%S)"
if [[ -z "$REPORT" ]]; then
    REPORT="$OUT_DIR/sse_sweep_${ts}.md"
fi
mkdir -p "$(dirname "$REPORT")"
touch "$REPORT"

declare -A THRPTS
declare -A GATE
declare -A PSHUFB
declare -A PMADDUBSW
declare -A PMADDWD
declare -A PMOVMSKB
declare -A MOVDQU
declare -A MOVDQA
declare -A LCPI
declare -A RSP
declare -A XMM_STACK

extract_thrpt_mid() {
    local bench_log="$1"
    sed -n 's/.*thrpt:  \[[^ ]* GiB\/s \([^ ]*\) GiB\/s [^]]*\].*/\1/p' "$bench_log" | tail -n1
}

calc_mean() {
    printf '%s\n' "$@" | awk '{s+=$1} END {if(NR==0)print "NA"; else printf "%.4f", s/NR}'
}

calc_median() {
    printf '%s\n' "$@" | sort -g | awk '
        {a[NR]=$1}
        END {
            if (NR==0) { print "NA"; exit }
            if (NR%2==1) { printf "%.4f", a[(NR+1)/2]; exit }
            printf "%.4f", (a[NR/2] + a[NR/2 + 1]) / 2.0
        }'
}

calc_min() {
    printf '%s\n' "$@" | sort -g | head -n1
}

calc_max() {
    printf '%s\n' "$@" | sort -g | tail -n1
}

calc_stddev() {
    printf '%s\n' "$@" | awk '
        {x[NR]=$1; s+=$1}
        END {
            if (NR<=1) { printf "0.0000"; exit }
            m=s/NR
            for (i=1;i<=NR;i++) { d=x[i]-m; ss+=d*d }
            printf "%.4f", sqrt(ss/NR)
        }'
}

{
    echo "# SSE Strict Sweep Report"
    echo
    echo "- Date: $(date -Iseconds)"
    echo "- Host: $(hostname)"
    echo "- Core: ${CORE}"
    echo "- Size: ${SIZE}"
    echo "- Rounds per variant: ${ROUNDS}"
    echo "- Top-k: ${TOP_K}"
    echo "- Harness: \`${HARNESS}\`"
    echo "- Variants: ${VARIANTS[*]}"
    echo "- Skip asm all: ${SKIP_ASM_ALL}"
    echo
    echo "## Raw Runs"
    echo
} > "$REPORT"

for v in "${VARIANTS[@]}"; do
    {
        echo "### ${v}"
        echo
        echo "| round | throughput_mid_gib_s | notes |"
        echo "|---:|---:|---|"
    } >> "$REPORT"

    for r in $(seq 1 "$ROUNDS"); do
        run_out="$OUT_DIR/${ts}_${v}_r${r}"
        mkdir -p "$run_out"

        cmd=("$HARNESS" "--variant" "$v" "--core" "$CORE" "--size" "$SIZE" "--skip-perf" "--out-dir" "$run_out")
        notes=""
        if [[ "$SKIP_ASM_ALL" -eq 1 ]]; then
            cmd+=("--skip-asm")
            notes="asm_skipped"
        elif [[ "$r" -gt 1 ]]; then
            cmd+=("--skip-asm")
            notes="asm_skipped_round>1"
        fi

        "${cmd[@]}" >"$run_out/sweep_driver.log" 2>&1

        bench_log="$(ls -t "$run_out"/*_"$v".bench.txt 2>/dev/null | head -n1 || true)"
        if [[ -z "$bench_log" ]]; then
            echo "Missing bench log for variant=${v} round=${r}" >&2
            exit 1
        fi

        thrpt="$(extract_thrpt_mid "$bench_log")"
        if [[ -z "$thrpt" ]]; then
            echo "Failed to parse throughput for variant=${v} round=${r}" >&2
            exit 1
        fi

        THRPTS["$v"]+="${THRPTS[$v]:+ }$thrpt"

        if [[ "$r" -eq 1 && "$SKIP_ASM_ALL" -eq 0 ]]; then
            rep="$(ls -t "$run_out"/sse_strict_harness_*.md | head -n1)"
            line_counts="$(grep -m1 '^- counts:' "$rep" || true)"
            line_gates="$(grep -m1 '^- gates:' "$rep" || true)"

            PSHUFB["$v"]="$(sed -n 's/.*pshufb=\([0-9]\+\).*/\1/p' <<<"$line_counts")"
            PMADDUBSW["$v"]="$(sed -n 's/.*pmaddubsw=\([0-9]\+\).*/\1/p' <<<"$line_counts")"
            PMADDWD["$v"]="$(sed -n 's/.*pmaddwd=\([0-9]\+\).*/\1/p' <<<"$line_counts")"
            PMOVMSKB["$v"]="$(sed -n 's/.*pmovmskb=\([0-9]\+\).*/\1/p' <<<"$line_counts")"
            MOVDQU["$v"]="$(sed -n 's/.*movdqu=\([0-9]\+\).*/\1/p' <<<"$line_counts")"
            MOVDQA["$v"]="$(sed -n 's/.*movdqa=\([0-9]\+\).*/\1/p' <<<"$line_counts")"
            LCPI["$v"]="$(sed -n 's/.*LCPI_refs=\([0-9]\+\).*/\1/p' <<<"$line_gates")"
            RSP["$v"]="$(sed -n 's/.*rsp_refs=\([0-9]\+\).*/\1/p' <<<"$line_gates")"
            XMM_STACK["$v"]="$(sed -n 's/.*xmm_stack_refs=\([0-9]\+\).*/\1/p' <<<"$line_gates")"
            GATE["$v"]="$(sed -n 's/.*status=\([^ ]\+\).*/\1/p' <<<"$line_gates")"
        fi

        echo "| ${r} | ${thrpt} | ${notes} |" >> "$REPORT"
    done
    echo >> "$REPORT"
done

rank_tsv="$OUT_DIR/${ts}_rank.tsv"
> "$rank_tsv"

for v in "${VARIANTS[@]}"; do
    IFS=' ' read -r -a vals <<< "${THRPTS[$v]}"
    mean="$(calc_mean "${vals[@]}")"
    med="$(calc_median "${vals[@]}")"
    min_v="$(calc_min "${vals[@]}")"
    max_v="$(calc_max "${vals[@]}")"
    std_v="$(calc_stddev "${vals[@]}")"
    gate="${GATE[$v]:-NA}"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$med" "$v" "$mean" "$std_v" "$min_v" "$max_v" "$gate" >> "$rank_tsv"
done

{
    echo "## Ranking (by median throughput)"
    echo
    echo "| rank | variant | median_gib_s | mean_gib_s | stddev | min | max | gate |"
    echo "|---:|---|---:|---:|---:|---:|---:|---|"
} >> "$REPORT"

rank=1
while IFS=$'\t' read -r med v mean std_v min_v max_v gate; do
    echo "| ${rank} | ${v} | ${med} | ${mean} | ${std_v} | ${min_v} | ${max_v} | ${gate} |" >> "$REPORT"
    rank=$((rank + 1))
done < <(sort -gr "$rank_tsv")

{
    echo
    echo "## Top ${TOP_K}"
    echo
    echo "| variant | median_gib_s | gate |"
    echo "|---|---:|---|"
} >> "$REPORT"

head -n "$TOP_K" < <(sort -gr "$rank_tsv") | while IFS=$'\t' read -r med v _ _ _ _ gate; do
    echo "| ${v} | ${med} | ${gate} |" >> "$REPORT"
done

if [[ "$SKIP_ASM_ALL" -eq 0 ]]; then
    {
        echo
        echo "## ASM Snapshot (round 1)"
        echo
        echo "| variant | pshufb | pmaddubsw | pmaddwd | pmovmskb | movdqu | movdqa | LCPI refs | rsp refs | xmm stack refs |"
        echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    } >> "$REPORT"
    while IFS=$'\t' read -r _ v _ _ _ _ _; do
        echo "| ${v} | ${PSHUFB[$v]:-NA} | ${PMADDUBSW[$v]:-NA} | ${PMADDWD[$v]:-NA} | ${PMOVMSKB[$v]:-NA} | ${MOVDQU[$v]:-NA} | ${MOVDQA[$v]:-NA} | ${LCPI[$v]:-NA} | ${RSP[$v]:-NA} | ${XMM_STACK[$v]:-NA} |" >> "$REPORT"
    done < <(sort -gr "$rank_tsv")
fi

echo "Sweep completed: $REPORT"
