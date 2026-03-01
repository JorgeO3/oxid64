# Safety Verification Matrix

This file tracks the verification state behind safety claims for `oxid64`.

## Swiss-Cheese Verification Layers

The target guarantee model is:

- Kani Verified: mathematical proofs for panic/overflow freedom across bounded models.
- Miri Verified: dynamic UB checks over Rust memory model and pointer rules.
- MSan Audited: no reads from uninitialized memory in native code paths.
- Fuzz Tested: sustained randomized differential testing (scalar vs SSSE3 vs AVX2).

## Current Status

- Kani: `in progress` (wired into `scripts/verify_safety.sh`, requires `cargo-kani` installed).
- Miri: `in progress` (wired into `scripts/verify_safety.sh`, requires nightly + `cargo-miri`).
- MSan: `in progress` (wired as opt-in `MSAN_ENABLED=1`, requires sanitizer-capable nightly toolchain).
- Fuzz: `active` (`proptest` and SIMD differential suites enabled in `tests/`).

## Reproducible Commands (Current)

- Full test suite:
  - `cargo test`
- Extended SIMD differential fuzz pass:
  - `PROPTEST_CASES=5000 cargo test --test sse_decode_tests --test avx2_decode_tests --test sse_encode_tests --test simd_fuzz_strict`
- Safety matrix (best effort):
  - `just verify-safety`
- Safety matrix (strict gate):
  - `just verify-safety-strict`

## Release Claim Policy

Do not publish absolute claims such as:

- "Kani Verified"
- "MIRI Verified"
- "MSan Audited"
- "2.5B fuzz iterations"

unless the corresponding pipeline is implemented, reproducible, and green in CI.
