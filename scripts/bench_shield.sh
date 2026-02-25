#!/usr/bin/env bash
set -euo pipefail

# bench_shield.sh (Fedora / systemd + cgroup v2)
#
# Goals:
# - Keep the benchmark CPU as "quiet" as practical on a desktop GNOME system.
# - Avoid accidentally pinning to E-cores on hybrid Intel (warn based on cpu_capacity/cpu_atom).
# - Provide a robust "direct bench exe" path for Criterion.
#
# Design (what this DOES and DOES NOT do):
# - Uses systemd AllowedCPUs to move *userland/systemd-managed* workloads away from reserved CPUs.
# - Runs the benchmark inside a dedicated systemd scope in bench.slice with AllowedCPUs=RUN_CPU.
# - Optionally pins noisy IRQs and their irq/* threads away from the bench CPUs.
# - It cannot fully "expel" all kernel work (ksoftirqd/kworker/rcu) without kernel boot params,
#   but IRQ affinity + picking a good core gets you very close.
#
# Typical usage:
#   # Stable "lab mode" (no turbo) with two reserved P-cores, run on CPU7 (buffer on CPU6)
#   ./scripts/bench_shield.sh --cpu 6,7 --run-cpu 7 --performance --stop-irqbalance --leave-irqbalance-stopped \
#     --no-aslr --no-turbo \
#     --irq-pin --irq-move 129 --irq-housekeep "0-5,8-19" \
#     --direct-bench base64_bench -- \
#     "Base64 Decoding/Rust Port (Safe Scalar)/1048576" \
#     --exact --noplot --warm-up-time 10 --measurement-time 30 --sample-size 100 \
#     --baseline rustdec_1m_no_turbo
#
# Notes:
# - Prefer --baseline for repeated runs; use --save-baseline once.
# - --stop-irqbalance stops irqbalance for this run; with --leave-irqbalance-stopped it won't be restarted.

ISO_CPUS="1"
RUN_CPU_OVERRIDE=""

DO_PERF="0"
DO_NO_TURBO="0"
DO_IRQBAL="0"
DO_LEAVE_IRQBAL_STOPPED="0"
DO_SESSION_SHIELD="1"
DO_NO_ASLR="0"
DO_WARN_ECORE="1"
DO_PRINT_PSR="1"

DO_IRQ_PIN="0"
IRQ_MOVE_CSV=""
IRQ_HOUSEKEEP_LIST=""

DIRECT_BENCH="0"
DIRECT_BENCH_NAME=""
CMD=()

die() { echo "error: $*" >&2; exit 1; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || die "missing command: $1"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cpu) ISO_CPUS="${2:-}"; shift 2;;
    --run-cpu) RUN_CPU_OVERRIDE="${2:-}"; shift 2;;

    --performance) DO_PERF="1"; shift;;
    --no-turbo) DO_NO_TURBO="1"; shift;;
    --stop-irqbalance) DO_IRQBAL="1"; shift;;
    --leave-irqbalance-stopped) DO_LEAVE_IRQBAL_STOPPED="1"; shift;;
    --no-session-shield) DO_SESSION_SHIELD="0"; shift;;
    --no-aslr) DO_NO_ASLR="1"; shift;;
    --no-warn-ecore) DO_WARN_ECORE="0"; shift;;
    --no-print-psr) DO_PRINT_PSR="0"; shift;;

    # IRQ pinning: move selected IRQs (and their irq/<n>-* threads) to housekeeping CPUs
    --irq-pin) DO_IRQ_PIN="1"; shift;;
    --irq-move) IRQ_MOVE_CSV="${2:-}"; shift 2;;
    --irq-housekeep) IRQ_HOUSEKEEP_LIST="${2:-}"; shift 2;;

    --direct-bench)
      DIRECT_BENCH="1"
      DIRECT_BENCH_NAME="${2:-}"
      [[ -n "$DIRECT_BENCH_NAME" ]] || die "--direct-bench requires a bench target name, e.g. base64_bench"
      shift 2
      ;;
    --) shift; CMD=("$@"); break;;
    -h|--help)
      sed -n '1,420p' "$0"
      exit 0
      ;;
    *) die "unknown arg: $1 (use --help)";;
  esac
done

need_cmd systemctl
need_cmd systemd-run
need_cmd sudo
need_cmd nproc
need_cmd awk
need_cmd sed
need_cmd ps
need_cmd readlink
need_cmd taskset

NCPU="$(nproc)"

# Parse isolated CPU list: "1" or "1,2"
IFS=',' read -r -a ISO_ARR <<< "$ISO_CPUS"
[[ "${#ISO_ARR[@]}" -ge 1 ]] || die "--cpu must be like 1 or 1,2"

# Default RUN_CPU is first of --cpu list unless overridden
RUN_CPU="${ISO_ARR[0]}"
if [[ -n "$RUN_CPU_OVERRIDE" ]]; then
  RUN_CPU="$RUN_CPU_OVERRIDE"
fi

# Validate isolated CPUs and RUN_CPU
for c in "${ISO_ARR[@]}"; do
  [[ "$c" =~ ^[0-9]+$ ]] || die "--cpu contains non-integer: $c"
  (( c >= 0 && c < NCPU )) || die "iso cpu $c out of range 0..$((NCPU-1))"
done
[[ "$RUN_CPU" =~ ^[0-9]+$ ]] || die "--run-cpu must be an integer"
(( RUN_CPU >= 0 && RUN_CPU < NCPU )) || die "run cpu $RUN_CPU out of range 0..$((NCPU-1))"

# Build "all CPUs except isolated set"
declare -A ISO_SET=()
for c in "${ISO_ARR[@]}"; do ISO_SET["$c"]=1; done

CPU_EXCEPT=""
for ((i=0; i<NCPU; i++)); do
  [[ -n "${ISO_SET[$i]+x}" ]] && continue
  CPU_EXCEPT+="${CPU_EXCEPT:+,}$i"
done

# Detect current session scope (e.g., session-2.scope)
SESSION_SCOPE=""
if [[ "$DO_SESSION_SHIELD" == "1" ]] && command -v loginctl >/dev/null 2>&1; then
  if [[ -n "${XDG_SESSION_ID:-}" ]]; then
    SESSION_SCOPE="$(loginctl show-session "$XDG_SESSION_ID" -p Scope --value 2>/dev/null || true)"
  fi
  if [[ -z "$SESSION_SCOPE" ]]; then
    sid="$(loginctl session-status 2>/dev/null | head -n 1 | awk '{print $1}' || true)"
    [[ -n "$sid" ]] && SESSION_SCOPE="$(loginctl show-session "$sid" -p Scope --value 2>/dev/null || true)"
  fi
fi

get_allowed() {
  local unit="$1"
  systemctl show -p AllowedCPUs "$unit" 2>/dev/null | sed 's/^AllowedCPUs=//'
}

# Units we may modify
UID_NUM="$(id -u)"
USER_AT_UNIT="user@${UID_NUM}.service"
USER_SLICE_UNIT="user.slice"
USER_UID_SLICE_UNIT="user-${UID_NUM}.slice"

ORIG_SYSTEM_SLICE="$(get_allowed system.slice || true)"
ORIG_INIT_SCOPE="$(get_allowed init.scope || true)"
ORIG_SESSION_SCOPE=""
ORIG_USER_SLICE="$(get_allowed "$USER_SLICE_UNIT" || true)"
ORIG_USER_AT="$(get_allowed "$USER_AT_UNIT" || true)"
ORIG_USER_UID_SLICE="$(get_allowed "$USER_UID_SLICE_UNIT" || true)"
if [[ -n "$SESSION_SCOPE" ]]; then
  ORIG_SESSION_SCOPE="$(get_allowed "$SESSION_SCOPE" || true)"
fi

IRQBAL_WAS_ACTIVE="0"
GOV_WAS=""
NO_TURBO_WAS=""
ASLR_WAS=""

# IRQ pinning state (restore on exit)
declare -A IRQ_OLD_KIND=()
declare -A IRQ_OLD_VAL=()
declare -A IRQ_THREAD_OLD_AFF=()
declare -A IRQ_THREAD_PID=()

warn_if_ecore() {
  [[ "$DO_WARN_ECORE" == "1" ]] || return 0

  cap_path="/sys/devices/system/cpu/cpu${RUN_CPU}/cpu_capacity"
  if [[ -r "$cap_path" ]]; then
    cap="$(cat "$cap_path" 2>/dev/null || echo 0)"
    # Typical on Arrow Lake: P ~1012-1024, E ~756
    if (( cap > 0 && cap < 900 )); then
      echo "WARN: cpu${RUN_CPU} looks like an E-core (cpu_capacity=$cap). Prefer P-cores (often 0-7)." >&2
    fi
    return 0
  fi

  if [[ -r /sys/devices/cpu_atom/cpus ]]; then
    atom="$(cat /sys/devices/cpu_atom/cpus)"
    if awk -v cpu="$RUN_CPU" '
      function inrange(tok, cpu) {
        if (tok ~ /-/) { split(tok,a,"-"); return (cpu>=a[1] && cpu<=a[2]); }
        return (cpu==tok);
      }
      BEGIN{
        n=split("'"$atom"'", parts, /,/);
        for(i=1;i<=n;i++){ gsub(/^[ \t]+|[ \t]+$/,"",parts[i]); if(inrange(parts[i],cpu)){ exit 0 } }
        exit 1
      }'; then
      echo "WARN: cpu${RUN_CPU} is listed under cpu_atom (E-core). Prefer P-cores." >&2
    fi
  fi
}

restore_irq_pinning() {
  # Restore IRQ thread affinities first (if PID still exists)
  for irq in "${!IRQ_THREAD_PID[@]}"; do
    pid="${IRQ_THREAD_PID[$irq]}"
    old="${IRQ_THREAD_OLD_AFF[$irq]:-}"
    [[ -n "$pid" && -n "$old" ]] || continue
    if [[ -e "/proc/$pid/status" ]]; then
      sudo taskset -pc "$old" "$pid" >/dev/null 2>&1 || true
    fi
  done

  # Restore IRQ smp_affinity(_list)
  for irq in "${!IRQ_OLD_KIND[@]}"; do
    kind="${IRQ_OLD_KIND[$irq]}"
    val="${IRQ_OLD_VAL[$irq]:-}"
    [[ -n "$val" ]] || continue
    if [[ "$kind" == "list" ]]; then
      echo "$val" | sudo tee "/proc/irq/${irq}/smp_affinity_list" >/dev/null 2>&1 || true
    else
      echo "$val" | sudo tee "/proc/irq/${irq}/smp_affinity" >/dev/null 2>&1 || true
    fi
  done
}

cleanup() {
  set +e

  restore_irq_pinning

  sudo systemctl set-property --runtime -- system.slice AllowedCPUs="$ORIG_SYSTEM_SLICE" >/dev/null 2>&1
  sudo systemctl set-property --runtime -- init.scope   AllowedCPUs="$ORIG_INIT_SCOPE"   >/dev/null 2>&1

  sudo systemctl set-property --runtime -- "$USER_SLICE_UNIT" AllowedCPUs="$ORIG_USER_SLICE" >/dev/null 2>&1
  sudo systemctl set-property --runtime -- "$USER_AT_UNIT"    AllowedCPUs="$ORIG_USER_AT"    >/dev/null 2>&1
  sudo systemctl set-property --runtime -- "$USER_UID_SLICE_UNIT" AllowedCPUs="$ORIG_USER_UID_SLICE" >/dev/null 2>&1

  if [[ -n "$SESSION_SCOPE" ]]; then
    sudo systemctl set-property --runtime -- "$SESSION_SCOPE" AllowedCPUs="$ORIG_SESSION_SCOPE" >/dev/null 2>&1
  fi

  if [[ "$DO_IRQBAL" == "1" && "$IRQBAL_WAS_ACTIVE" == "1" && "$DO_LEAVE_IRQBAL_STOPPED" != "1" ]]; then
    sudo systemctl start irqbalance >/dev/null 2>&1
  fi

  if [[ "$DO_PERF" == "1" && -n "$GOV_WAS" ]] && command -v cpupower >/dev/null 2>&1; then
    sudo cpupower frequency-set -g "$GOV_WAS" >/dev/null 2>&1
  fi

  if [[ "$DO_NO_TURBO" == "1" && -n "$NO_TURBO_WAS" ]] && [[ -w /sys/devices/system/cpu/intel_pstate/no_turbo ]]; then
    echo "$NO_TURBO_WAS" | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo >/dev/null 2>&1
  fi

  if [[ "$DO_NO_ASLR" == "1" && -n "$ASLR_WAS" ]] && [[ -e /proc/sys/kernel/randomize_va_space ]]; then
    echo "$ASLR_WAS" | sudo tee /proc/sys/kernel/randomize_va_space >/dev/null 2>&1
  fi
}
trap cleanup EXIT INT TERM

sudo -v
warn_if_ecore

# Optional: stop irqbalance
if [[ "$DO_IRQBAL" == "1" ]]; then
  if systemctl is-active --quiet irqbalance; then
    IRQBAL_WAS_ACTIVE="1"
    sudo systemctl stop irqbalance
  fi
fi

# Optional: disable ASLR (LLVM-style)
if [[ "$DO_NO_ASLR" == "1" ]]; then
  [[ -e /proc/sys/kernel/randomize_va_space ]] || die "/proc/sys/kernel/randomize_va_space missing"
  ASLR_WAS="$(cat /proc/sys/kernel/randomize_va_space 2>/dev/null || true)"
  echo 0 | sudo tee /proc/sys/kernel/randomize_va_space >/dev/null
fi

# Optional: set governor performance
if [[ "$DO_PERF" == "1" ]]; then
  need_cmd cpupower || die "cpupower not found; install kernel-tools or run without --performance"
  if [[ -r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
    GOV_WAS="$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || true)"
  fi
  sudo cpupower frequency-set -g performance >/dev/null
fi

# Optional: disable turbo (for maximum repeatability; may reduce peak single-core)
if [[ "$DO_NO_TURBO" == "1" ]]; then
  [[ -r /sys/devices/system/cpu/intel_pstate/no_turbo ]] || die "intel_pstate no_turbo not found"
  NO_TURBO_WAS="$(cat /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null || true)"
  echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo >/dev/null
fi

# Shield system daemons + init off the isolated CPU(s)
sudo systemctl set-property --runtime -- system.slice AllowedCPUs="$CPU_EXCEPT"
sudo systemctl set-property --runtime -- init.scope   AllowedCPUs="$CPU_EXCEPT"

# Shield user services too (IMPORTANT on GNOME desktops)
sudo systemctl set-property --runtime -- "$USER_SLICE_UNIT" AllowedCPUs="$CPU_EXCEPT" || true
sudo systemctl set-property --runtime -- "$USER_AT_UNIT"    AllowedCPUs="$CPU_EXCEPT" || true
sudo systemctl set-property --runtime -- "$USER_UID_SLICE_UNIT" AllowedCPUs="$CPU_EXCEPT" || true

# Optionally move your login session scope off the isolated CPU(s) too
if [[ -n "$SESSION_SCOPE" ]]; then
  sudo systemctl set-property --runtime -- "$SESSION_SCOPE" AllowedCPUs="$CPU_EXCEPT"
fi

echo "==> Shielding CPU(s) $ISO_CPUS (benchmark will run on CPU $RUN_CPU)"
echo "    system.slice/init.scope allowed: $CPU_EXCEPT"
echo "    user.slice/user@UID allowed:    $CPU_EXCEPT"
if [[ -n "$SESSION_SCOPE" ]]; then
  echo "    session scope ($SESSION_SCOPE) allowed: $CPU_EXCEPT"
else
  echo "    session scope: (not modified)"
fi

# IRQ pinning (optional)
apply_irq_one() {
  local irq="$1"
  local house_list="$2"

  local f_list="/proc/irq/${irq}/smp_affinity_list"
  local f_hex="/proc/irq/${irq}/smp_affinity"

  if [[ -e "$f_list" ]]; then
    IRQ_OLD_KIND["$irq"]="list"
    IRQ_OLD_VAL["$irq"]="$(cat "$f_list" 2>/dev/null || true)"
    echo "$house_list" | sudo tee "$f_list" >/dev/null 2>&1 || true
  elif [[ -e "$f_hex" ]]; then
    # If only hex exists, we still store it, but we don't compute a generic mask here.
    # You can still move via smp_affinity_list on modern Fedora kernels for most IRQs.
    IRQ_OLD_KIND["$irq"]="hex"
    IRQ_OLD_VAL["$irq"]="$(cat "$f_hex" 2>/dev/null || true)"
    # Best-effort: do nothing unless caller set a valid hex in IRQ_HOUSEKEEP_LIST (not supported here).
    true
  fi

  # Also try moving the irq/<n>-* kthread (this often works even if effective_affinity differs).
  local pid
  pid="$(ps -eLo pid,comm | awk -v n="$irq" '$2 ~ ("^irq/" n "-") {print $1; exit}')"
  if [[ -n "$pid" ]]; then
    local old_aff
    old_aff="$(taskset -pc "$pid" 2>/dev/null | awk -F': ' '{print $2}')"
    if [[ -n "$old_aff" ]]; then
      IRQ_THREAD_PID["$irq"]="$pid"
      IRQ_THREAD_OLD_AFF["$irq"]="$old_aff"
      sudo taskset -pc "$house_list" "$pid" >/dev/null 2>&1 || true
    fi
  fi
}

if [[ "$DO_IRQ_PIN" == "1" ]]; then
  [[ -n "$IRQ_MOVE_CSV" ]] || die "--irq-pin requires --irq-move <csv>, e.g. --irq-move 129"
  house_list="${IRQ_HOUSEKEEP_LIST:-$CPU_EXCEPT}"

  IFS=',' read -r -a irqs <<< "$IRQ_MOVE_CSV"
  for irq in "${irqs[@]}"; do
    [[ "$irq" =~ ^[0-9]+$ ]] || continue
    apply_irq_one "$irq" "$house_list"
  done

  echo "==> IRQ pinning requested:"
  echo "    moved IRQs: $IRQ_MOVE_CSV"
  echo "    housekeeping CPUs: $house_list"
  # Best-effort debug: show effective affinity if present
  for irq in "${irqs[@]}"; do
    [[ "$irq" =~ ^[0-9]+$ ]] || continue
    if [[ -e "/proc/irq/${irq}/effective_affinity_list" ]]; then
      eff="$(cat "/proc/irq/${irq}/effective_affinity_list" 2>/dev/null || true)"
      [[ -n "$eff" ]] && echo "    IRQ $irq effective_affinity_list: $eff"
    fi
  done
fi

# Quick check: show last-CPU tasks + their allowed list (more meaningful than psr alone)
if [[ "$DO_PRINT_PSR" == "1" ]]; then
  echo "==> Quick check: last-CPU==${RUN_CPU} (top 20) + allowed CPUs:"
  ps -eLo pid,psr,comm --sort=psr | awk -v c="$RUN_CPU" '$2==c {print $1, $2, $3}' | head -n 20 | while read -r pid psr comm; do
    allowed="$(awk -F'\t' '/Cpus_allowed_list/ {print $2}' "/proc/$pid/status" 2>/dev/null || echo "?")"
    printf "  %-7s cpu%-2s %-28s allowed=%s\n" "$pid" "$psr" "$comm" "$allowed"
  done
fi

# Run target in a dedicated systemd slice with AllowedCPUs=RUN_CPU.
run_in_bench_slice() {
  sudo systemd-run --quiet --scope --slice=bench.slice \
    -p "AllowedCPUs=${RUN_CPU}" \
    "$@"
}

if [[ "$DIRECT_BENCH" == "1" ]]; then
  need_cmd cargo

  for a in "${CMD[@]}"; do
    if [[ "$a" == "--sampling-mode" ]]; then
      die "--sampling-mode no es una flag de Criterion CLI. Configura SamplingMode en el código del bench (BenchmarkGroup::sampling_mode)."
    fi
  done

  echo "==> Building bench target (no-run): $DIRECT_BENCH_NAME"
  cargo bench --bench "$DIRECT_BENCH_NAME" --no-run >/dev/null

  EXE="$(ls -t "target/release/deps/${DIRECT_BENCH_NAME}-"* 2>/dev/null | head -n 1 || true)"
  [[ -n "$EXE" && -x "$EXE" ]] || die "could not find bench executable for '${DIRECT_BENCH_NAME}' under target/release/deps"

  echo "==> Running bench executable in bench.slice pinned to CPU $RUN_CPU:"
  printf '    '; printf '%q ' "$EXE" --bench "${CMD[@]}"; echo

  run_in_bench_slice "$EXE" --bench "${CMD[@]}"
  exit 0
fi

[[ "${#CMD[@]}" -ge 1 ]] || die "no command provided after --"

echo "==> Running command in bench.slice pinned to CPU $RUN_CPU:"
printf '    '; printf '%q ' "${CMD[@]}"; echo
run_in_bench_slice "${CMD[@]}"
