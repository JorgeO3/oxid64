# WASM SIMD128 Backend Audit Notes

The WASM SIMD128 backend intentionally exposes two decode contracts:

- `strict`: correctness-grade path for untrusted input
- `non-strict`: `CHECK0`-style trusted-input path

## Critical decoder note

The decoder depends on SSSE3-style `pshufb` semantics for its raw ASCII lookup
tables. The backend now uses an explicit compatibility helper instead of plain
`i8x16_swizzle` for those lookups, because WASM swizzle zeroes indices `>= 16`
instead of using the low nibble.

## Warning about `non-strict`

`non-strict` validates only the first 16-byte lane of each DS64 block. It is a
trusted-input `CHECK0` contract and should not be used as a full validator for
untrusted Base64.

## Decode bounds contract

The decode kernel uses overlapping 16-byte stores to materialize 12-byte logical
outputs. The shared helpers therefore guard the real touched spans, not only the
logical decoded length:

- tail16: 16 bytes readable, 16 bytes writable
- DS64: 96 bytes readable, 52 bytes writable
- double DS64: 160 bytes readable, 100 bytes writable

## What is currently verified

- pure model tests for `pshufb` compatibility semantics
- pure schedule and prefix-bound models for `CHECK0` and overlapping stores
- strict/high-bit/padding/misaligned integration tests on wasm targets
- encode boundary and canary tests around SIMD-entry/drain/tail thresholds
- Kani proofs for DS64/tail store guards, `pshufb` low-nibble semantics, and encode planner thresholds
- Miri over WASM model tests on the host
- WASM-specific fuzz/model targets for `pshufb`, schedule, encode-prefix, and partial-write contracts

## Runtime execution setup

Real runtime execution of the backend should use `wasm32-wasip1` plus `wasmtime`
with `+simd128`, for example:

```bash
just verify-wasm-simd128-smoke runs=8
just verify-wasm-simd128 runs=64
```

## Stack status

- `cargo-careful`: runtime smoke works with normal `cargo test`; the `cargo-careful` layer is currently host-side pure-model coverage because the careful wasm sysroot is still missing WASI C runtime pieces (`crt1-command.o`, `-lc`)
- `ASan`: host-side pure-model coverage via `test-asan-wasm` / `test-asan-wasm-smoke`
- `MSan`: host-side pure-model coverage via `test-msan-wasm` / `test-msan-wasm-smoke`
- `Miri`: wired for host-side pure models
- `Kani`: wired for host-side pure models
- `cargo-fuzz`: wired for host-side model/property fuzz targets

So the stack is operational for WASM correctness/models, and runtime execution is
validated with `wasmtime`, but sanitizer and careful coverage are currently
model-level rather than full wasm-runtime instrumentation.
