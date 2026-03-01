#!/usr/bin/env bash
set -euo pipefail

STRICT=0
FUZZ_CASES="${FUZZ_CASES:-5000}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      STRICT=1
      shift
      ;;
    --fuzz-cases)
      FUZZ_CASES="$2"
      shift 2
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

have() {
  command -v "$1" >/dev/null 2>&1
}

warn_or_fail_missing() {
  local msg="$1"
  if [[ "$STRICT" -eq 1 ]]; then
    echo "ERROR: $msg" >&2
    exit 1
  fi
  echo "WARN:  $msg"
}

run_layer() {
  local name="$1"
  shift
  echo
  echo "==> [$name]"
  "$@"
}

echo "Safety verification (Swiss-Cheese model)"
echo "STRICT=${STRICT} FUZZ_CASES=${FUZZ_CASES}"

run_layer "Core tests" cargo test

run_layer "Fuzz (proptest differential)" \
  env PROPTEST_CASES="${FUZZ_CASES}" cargo test \
    --test sse_decode_tests \
    --test avx2_decode_tests \
    --test sse_encode_tests \
    --test simd_fuzz_strict

echo
echo "==> [Miri]"
if have cargo && cargo +nightly miri --version >/dev/null 2>&1; then
  cargo +nightly miri test --lib
else
  warn_or_fail_missing "cargo-miri is not installed (rustup component add miri --toolchain nightly)."
fi

echo
echo "==> [Kani]"
if have cargo-kani; then
  cargo kani --version >/dev/null
  # Run a smoke pass on lib targets; detailed harnesses can be added later.
  cargo kani --tests
else
  warn_or_fail_missing "cargo-kani is not installed."
fi

echo
echo "==> [MSan]"
if [[ "${MSAN_ENABLED:-0}" == "1" ]]; then
  if have clang; then
    # Use the sanitizer for both rustc and rustdoc to avoid ABI mismatch.
    # Run `--tests` to skip doctests, which are noisy with sanitizer toolchains.
    RUSTFLAGS="-Zsanitizer=memory" \
    RUSTDOCFLAGS="-Zsanitizer=memory" \
      cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --tests
  else
    warn_or_fail_missing "clang not found for MSan run."
  fi
else
  warn_or_fail_missing "MSan run is disabled (set MSAN_ENABLED=1 to enable)."
fi

echo
echo "Safety verification completed."
