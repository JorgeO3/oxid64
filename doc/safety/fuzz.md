# cargo-fuzz Guide

This document defines the current `cargo-fuzz` setup for `oxid64`.

## Targets

- `decode_diff`: strict decode differential against scalar
- `encode_diff`: encode differential against scalar
- `roundtrip`: scalar encode plus multi-engine decode roundtrip checks
- `invalid_semantics`: malformed-input differential checks for strict decoders
- `tail_alignment`: tails, subslices, and misalignment-sensitive windows
- `ssse3_strict_diff`: SSSE3 strict invalid-input differential checks
- `ssse3_non_strict_schedule`: SSSE3 non-strict CHECK0 schedule characterization
- `avx2_strict_diff`: AVX2 strict invalid-input differential checks
- `avx2_non_strict_schedule`: AVX2 non-strict CHECK0 schedule characterization
- `avx2_unchecked_contract`: AVX2 unchecked trusted-input/bounds contract checks
- `avx2_partial_write_bounds`: AVX2 bounded partial-write checks on invalid decode
- `avx512_strict_diff`: AVX512 strict invalid-input differential checks
- `avx512_non_strict_schedule`: AVX512 non-strict CHECK0 schedule characterization
- `avx512_encode_diff`: AVX512 encode differential checks against scalar
- `avx512_partial_write_bounds`: AVX512 bounded partial-write checks on invalid decode
- `neon_strict_diff`: NEON strict invalid-input differential checks
- `neon_non_strict_schedule`: NEON non-strict CHECK0 schedule characterization
- `neon_encode_diff`: NEON encode differential checks against scalar
- `neon_partial_write_bounds`: NEON bounded partial-write checks on invalid decode
- `wasm_pshufb_compat`: WASM `pshufb` compatibility model fuzzing
- `wasm_non_strict_schedule`: WASM non-strict CHECK0 schedule model fuzzing
- `wasm_encode_prefix_model`: WASM encode threshold/prefix model fuzzing
- `wasm_partial_write_model`: WASM partial-write prefix model fuzzing

## Build all targets

```bash
just fuzz-build
```

## Run a smoke pass for one target

```bash
just fuzz-smoke target=decode_diff runs=64
```

## Run smoke across all current targets

```bash
just fuzz-smoke-all runs=32
```

`just fuzz-smoke-all` is host-oriented and does not try to claim NEON runtime
coverage on `x86_64`. Run NEON fuzz smoke separately on native `AArch64` with:

```bash
just fuzz-build-neon
just fuzz-smoke-neon runs=32
```

WASM fuzz coverage currently targets pure models/properties on the host:

```bash
just fuzz-build-wasm
just fuzz-smoke-wasm runs=32
```

Runtime wasm execution itself is exercised separately with `wasmtime` via:

```bash
just test-wasm-runtime-smoke
```

## Run AVX2-specific smoke

```bash
just fuzz-build-avx2
just fuzz-smoke-avx2 runs=32
just fuzz-build-avx512
just fuzz-smoke-avx512 runs=32
```

## Seed corpus

The seed corpus lives under `fuzz/corpus/` and includes:

- empty inputs
- small lengths `0..128`
- valid Base64 padding boundaries
- malformed ASCII / non-ASCII cases
- tail and alignment-oriented cases

## Notes

- Fuzz smoke is not a replacement for long fuzzing campaigns.
- Host coverage is naturally richer on x86_64 than on `NEON` / `WASM` backends.
