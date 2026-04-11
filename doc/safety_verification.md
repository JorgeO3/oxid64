# Safety Verification Matrix

This file tracks the verification state behind safety claims for `oxid64`.

Pure backend verification models now live under `src/engine/models/` so they can
be shared by integration tests, Miri, Kani, and model fuzzing without keeping
`cfg(test)` / `cfg(miri)` / `cfg(kani)` blocks inside the runtime kernels.

## Swiss-Cheese Verification Layers

The target guarantee model is:

- Kani Verified: mathematical proofs for panic/overflow freedom across bounded models.
- Miri Verified: dynamic UB checks over Rust memory model and pointer rules.
- MSan Audited: no reads from uninitialized memory in native code paths.
- Fuzz Tested: sustained randomized differential testing (scalar vs SSSE3 vs AVX2).

## Mandatory Stack Order

The implementation order for this repository is:

1. `cargo-careful`
2. ASan
3. MSan
4. Miri
5. Kani
6. `cargo-fuzz`

Additional tooling (`cargo-geiger`, `-Zrandomize-layout`, LSan, TSan/Loom)
is deferred to the final phase.

## Current Status

- cargo-careful: `active` (Phase 2 baseline wired via `just test-careful`).
- ASan: `active` on x86_64 Linux (`just test-asan`).
- MSan: `active` on x86_64 Linux (`just test-msan`), with dedicated smoke validation via `just test-msan-smoke`.
- Miri: `active` for contract-focused library checks (`just test-miri`) and widened exploration (`just test-miri-many-seeds`).
- Kani: `active` for small helper contracts (`just test-kani`).
- Fuzz: `active` (`proptest` in `tests/` plus `cargo-fuzz` smoke targets via `just fuzz-smoke-all`).

## Reproducible Commands (Current)

- Full test suite:
  - `cargo test`
- Phase 2 (`cargo-careful`):
  - `just test-careful`
- Phase 3 (ASan):
  - `just test-asan`
- Phase 4 (MSan):
  - `just test-msan`
  - `just test-msan-smoke`
  - `just test-msan-origins`
- Phase 5 (Miri):
  - `just test-miri`
  - `just test-miri-ssse3`
  - `just test-miri-avx2`
  - `just test-miri-avx512`
  - `just test-miri-many-seeds`
  - `just test-miri-smoke`
- Phase 6 (Kani):
  - `just test-kani`
  - `just test-kani-ssse3`
  - `just test-kani-avx2`
  - `just test-kani-avx512`
- Phase 7 (`cargo-fuzz`):
  - `just fuzz-build`
  - `just fuzz-build-avx2`
  - `just fuzz-build-avx512`
  - `just fuzz-smoke target=decode_diff runs=64`
  - `just fuzz-smoke-avx2 runs=32`
  - `just fuzz-smoke-avx512 runs=32`
  - `just fuzz-smoke-all runs=32`
- Phase 0 environment baseline:
  - `just safety-phase0-report`
- Phase 0 baseline + current matrix:
  - `just safety-phase0`
- Extended SIMD differential fuzz pass:
  - `PROPTEST_CASES=5000 cargo test --test sse_decode_tests --test avx2_decode_tests --test sse_encode_tests --test simd_fuzz_strict`
- Safety matrix (best effort):
  - `just verify-safety`
  - `just verify-safety fuzz_cases=200`
- Safety matrix (strict gate):
  - `just verify-safety-strict`

### Miri invocation

- Command form: `cargo +nightly miri --help`
- Base profile: `MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --lib`
- Scope and limitations: `doc/safety/miri.md`
- `just test-miri-smoke` is intentionally red and only proves that Miri is active.

### MSan notes

- Scope and limitations: `doc/safety/msan.md`
- `just test-msan` is the normal green run.
- `just test-msan-smoke` is intentionally red and only proves instrumentation is active.

### Kani notes

- Scope and limitations: `doc/safety/kani.md`
- `just test-kani` proves helper/contract properties in a bounded model.

### Fuzz notes

- Scope and targets: `doc/safety/fuzz.md`
- `just fuzz-smoke-all` is intentionally short and only validates target wiring/corpus health.

### SSSE3 notes

- Backend-specific contract notes: `doc/safety/ssse3.md`
- `strict` and `non-strict` are intentionally different APIs; only `strict` is a full validator for untrusted input.

### AVX2 notes

- Backend-specific contract notes: `doc/safety/avx2.md`
- `strict`, `non-strict`, and `unchecked` are intentionally different contracts.
- Only `strict` is a full validator for untrusted input.

### AVX512 notes

- Backend-specific contract notes: `doc/safety/avx512vbmi.md`
- `strict` and `non-strict` are intentionally different contracts.
- Only `strict` is a full validator for untrusted input.

### NEON notes

- Backend-specific contract notes: `doc/safety/neon.md`
- `strict` and `non-strict` are intentionally different contracts.
- Only `strict` is a full validator for untrusted input.

### WASM SIMD128 notes

- Backend-specific contract notes: `doc/safety/wasm_simd128.md`
- `strict` and `non-strict` are intentionally different contracts.
- Only `strict` is a full validator for untrusted input.
- Runtime smoke is exercised with `wasmtime`; `cargo-careful`/ASan/MSan are currently model-level for this backend.

## Release Claim Policy

Do not publish absolute claims such as:

- "Kani Verified"
- "MIRI Verified"
- "MSan Audited"
- "2.5B fuzz iterations"

unless the corresponding pipeline is implemented, reproducible, and green in CI.
