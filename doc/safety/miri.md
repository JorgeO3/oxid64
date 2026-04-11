# Miri Guide

This document defines the practical Miri scope for `oxid64`.

## What Miri covers here

- shared contract helpers in `src/engine/common.rs`
- scalar tail handling and exact-window slice contracts
- dispatch contracts in `src/engine/mod.rs`
- pointer arithmetic, alignment-sensitive windows, and provenance-sensitive helpers
- pure backend models from `src/engine/models/` exercised by `tests/*_models.rs`
- SSSE3 schedule/alignment models under test-only verification code
- AVX2 schedule/write-bound models under test-only verification code
- AVX512 VBMI planner/schedule models under test-only verification code
- NEON planner/schedule models under test-only verification code
- WASM SIMD128 planner/schedule/`pshufb` models under test-only verification code

## What Miri does not cover here

- full execution of all SIMD kernels on every architecture
- `NEON` runtime behavior on this x86_64 host
- full WASM runtime behavior by itself (runtime wasm execution is covered separately with `wasmtime`)
- end-to-end semantic correctness by itself

## Base run

```bash
just test-miri
```

This uses a strict baseline profile with:

- full backtraces
- symbolic alignment checks
- strict provenance

Backend-focused subsets:

```bash
just test-miri-ssse3
just test-miri-avx2
just test-miri-avx512
just test-miri-neon
just test-miri-wasm
```

## Many-seeds run

```bash
just test-miri-many-seeds
```

Use this to widen interpreter exploration after the baseline is green.

## Instrumentation smoke

```bash
just test-miri-smoke
```

This intentionally executes `src/bin/miri_smoke.rs`, which performs a
controlled UB operation via `MaybeUninit::assume_init()`. The command is
expected to fail and the log should mention `Undefined Behavior`.

## Big-endian run

Big-endian Miri is intentionally deferred until the host has the target
installed, for example:

```bash
rustup target add --toolchain nightly s390x-unknown-linux-gnu
MIRIFLAGS="-Zmiri-backtrace=full" cargo +nightly miri test --lib --target s390x-unknown-linux-gnu
```

This is not part of the default Phase 5 gate today.

## Interpretation of a green run

If the Miri suite is green, it means the covered helper/contract code did not
trigger Rust-memory-model violations under interpretation. It does **not** mean
that every SIMD backend is fully proven sound.
