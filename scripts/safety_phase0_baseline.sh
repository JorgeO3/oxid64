#!/usr/bin/env bash
set -euo pipefail

REPORT_PATH="${1:-doc/safety/baseline.md}"
REPORT_DIR="$(dirname "$REPORT_PATH")"

mkdir -p "$REPORT_DIR"

check_bin() {
  local bin="$1"
  if command -v "$bin" >/dev/null 2>&1; then
    printf -- "- %s: installed (%s)\n" "$bin" "$(command -v "$bin")"
  else
    printf -- "- %s: missing\n" "$bin"
  fi
}

{
  printf -- "# Safety Phase 0 Baseline\n\n"
  printf -- "Generated: %s\n\n" "$(date -Iseconds)"

  printf -- "## Stack Order (mandatory)\n\n"
  printf -- "1. cargo-careful\n"
  printf -- "2. ASan\n"
  printf -- "3. MSan\n"
  printf -- "4. Miri\n"
  printf -- "5. Kani\n"
  printf -- "6. cargo-fuzz\n\n"

  printf -- "## Toolchain\n\n"
  printf -- "- cargo: %s\n" "$(cargo --version)"
  printf -- "- rustup: %s\n" "$(rustup --version 2>/dev/null | head -n 1)"
  printf -- "- rustc (active): %s\n" "$(rustc --version)"
  printf -- "- cargo nightly: %s\n\n" "$(cargo +nightly --version)"

  printf -- "### Installed toolchains\n\n"
  rustup toolchain list | sed 's/^/- /'
  printf -- "\n"

  printf -- "### Installed targets\n\n"
  printf -- "- stable targets:\n"
  rustup target list --installed | sed 's/^/  - /'
  printf -- "- nightly targets:\n"
  rustup target list --toolchain nightly-x86_64-unknown-linux-gnu --installed | sed 's/^/  - /'
  printf -- "\n"

  printf -- "### Nightly components\n\n"
  rustup component list --toolchain nightly-x86_64-unknown-linux-gnu --installed | sed 's/^/- /'
  printf -- "\n"

  printf -- "## Required binaries\n\n"
  check_bin cargo-careful
  check_bin cargo-fuzz
  check_bin cargo-miri
  check_bin cargo-kani
  check_bin clang
  check_bin llvm-symbolizer
  check_bin just
  check_bin make
  printf -- "\n"

  printf -- "## Optional binaries (final phase)\n\n"
  check_bin cargo-geiger
  check_bin valgrind
  check_bin wasmtime
  check_bin qemu-aarch64
  printf -- "\n"

  printf -- "## Quick command checks\n\n"
  cargo +nightly miri --help >/dev/null
  printf -- "- cargo +nightly miri --help: ok\n"
  printf -- "- cargo kani --version: %s\n" "$(cargo kani --version | head -n 1)"
  printf -- "- cargo-fuzz -V: %s\n" "$(cargo-fuzz -V | head -n 1)"
  printf -- "- cargo-careful: binary detected (run via cargo +nightly careful ...)\n\n"

  printf -- "## Notes\n\n"
  printf -- "- This report only captures environment readiness (Phase 0), not correctness claims.\n"
  printf -- "- Use just verify-safety for the current matrix smoke run.\n"
  printf -- "- Use MSAN_ENABLED=1 just verify-safety to enable MSan in the existing script.\n"
} > "$REPORT_PATH"

printf -- "Wrote Phase 0 baseline report to %s\n" "$REPORT_PATH"
