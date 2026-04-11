# oxid64

High-performance Base64 codec in Rust with strict correctness by default and SIMD acceleration where available.

[![Crates.io](https://img.shields.io/crates/v/oxid64.svg)](https://crates.io/crates/oxid64)
[![docs.rs](https://docs.rs/oxid64/badge.svg)](https://docs.rs/oxid64)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Goals

- Correctness first (strict RFC 4648 decoding by default).
- Safe API surface (`Option` on decode errors, no partial-success ambiguity).
- Competitive throughput against C baselines (Turbo-Base64 and related implementations).
- Reproducible performance methodology (`bench_shield` + Criterion).

## Feature Matrix

- x86/x86_64:
  - SSSE3 decode/encode
  - AVX2 decode/encode
  - AVX-512 VBMI decode/encode
- aarch64:
  - NEON decode/encode
- wasm32:
  - SIMD128 decode/encode
- all targets:
  - scalar fallback
  - runtime dispatch via `Decoder::detect()`

## Installation

```toml
[dependencies]
oxid64 = "0.1"
```

MSRV: Rust `1.89+` (edition 2024).

## Quick Start

```rust
use oxid64::{Base64Decoder, Decoder};

let engine = Decoder::detect();

let encoded = engine.encode(b"Hello, world!");
assert_eq!(encoded, b"SGVsbG8sIHdvcmxkIQ==");

let decoded = engine.decode(&encoded).expect("valid base64");
assert_eq!(decoded, b"Hello, world!");
```

## Zero-Allocation API

```rust
use oxid64::{Base64Decoder, Decoder, decoded_len, encoded_len};

let engine = Decoder::detect();
let input = b"oxid64";

let mut enc = vec![0u8; encoded_len(input.len())];
let n = engine.encode_to_slice(input, &mut enc);
assert_eq!(&enc[..n], b"b3hpZDY0");

let mut dec = vec![0u8; decoded_len(&enc).unwrap()];
let m = engine.decode_to_slice(&enc, &mut dec).unwrap();
assert_eq!(&dec[..m], input);
```

## Strictness Model

- Default (`strict: true`): validates all relevant SIMD vectors and matches strict scalar behavior.
- Optional (`strict: false` in SIMD engines): faster path for trusted input scenarios.

For untrusted data, keep strict mode.

## Benchmark Methodology

This project follows a benchmark style similar to Turbo-Base64 reports, but with strict Rust-side reproducibility.

What benchmark numbers in this README mean:

- Single-thread throughput
- Includes strict Base64 validity checking on decode for strict rows
- Practical dataset sizes (not cache-only microbenchmarks)
- Cross-library comparison in one harness (`Criterion` + FFI baselines)
- Versions pinned to what is in this repo at measurement time

Important unit note:

- `oxid64` local tables here use `GiB/s` (binary bytes/s) and mean `µs`
- Turbo-Base64 public reports are usually in decimal `MB/s`
- Do not compare `MB/s` vs `GiB/s` without conversion

## Benchmark Environment

### Local Snapshot (this repo)

- Harness: `benches/base64_bench.rs` (Criterion)
- Shielding: `scripts/bench_shield.sh` (CPU isolation, governor, IRQ controls)
- Representative workload in table below: `1 MiB` (`1048576` bytes)
- Decode group: `Base64 Decoding Strict Compare`
- Encode group: `Base64 Encoding Compare`

### Reference Context (Turbo-Base64 style)

Turbo-Base64 publishes broader matrices by:
- CPU model / clock / memory / compiler
- size classes (small/medium/large)
- E/D throughput columns (encode/decode)
- scalar + SIMD family variants

This README mirrors that philosophy for ergonomics while keeping local numbers reproducible from this tree.

## Decode Benchmarks

### Scalar Matrix (strict/non-strict)

Objetivo: comparar rutas escalares con la misma semántica de validación.

- `oxid64` strict scalar
- `oxid64` non-strict scalar (`TODO`, pendiente de implementación)
- Turbo-Base64 check (validación)
- Turbo-Base64 no-check (sin validación estricta completa)

#### Sample Size: 1 KiB (`1024` bytes)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Scalar Strict | `TBD` | `TBD` | pending |
| `oxid64` Scalar Non-Strict | `TBD` | `TBD` | TODO |
| `TurboBase64` C (check) | `TBD` | `TBD` | pending |
| `TurboBase64` C (no-check) | `TBD` | `TBD` | pending |

#### Sample Size: 10 KiB (`10240` bytes)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Scalar Strict | `TBD` | `TBD` | pending |
| `oxid64` Scalar Non-Strict | `TBD` | `TBD` | TODO |
| `TurboBase64` C (check) | `TBD` | `TBD` | pending |
| `TurboBase64` C (no-check) | `TBD` | `TBD` | pending |

#### Sample Size: 1 MiB (`1048576` bytes)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Scalar Strict | `TBD` | `TBD` | pending |
| `oxid64` Scalar Non-Strict | `TBD` | `TBD` | TODO |
| `TurboBase64` C (check) | `TBD` | `TBD` | pending |
| `TurboBase64` C (no-check) | `TBD` | `TBD` | pending |

### 1 MiB (`1048576` bytes, strict compare)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Rust (`AVX2 Strict`) | `42.845 µs` | `22.793 GiB/s` | measured |
| Turbo-Base64 C (`AVX2, check`) | `56.994 µs` | `17.134 GiB/s` | measured |
| `oxid64` Rust (`SSSE3 Strict`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX-512 VBMI Strict`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`AVX2, default`) | `TBD` | `TBD` | pending |
| `fastb64z` Zig (`Decode Fast`) | `TBD` | `TBD` | pending |
| `FastBase64` Pascal (`Decode`) | `TBD` | `TBD` | pending |
| `lemire/fastbase64` C (`AVX2 validate`) | `TBD` | `TBD` | pending |

Current local delta:
- `oxid64` AVX2 strict vs Turbo-Base64 AVX2 check: about **+33.0%** throughput in this snapshot.

### 10 KiB (`10240` bytes, practical short-block class)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Rust (`SSSE3 Strict`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX2 Strict`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX-512 VBMI Strict`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`AVX2, check`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`AVX2, default`) | `TBD` | `TBD` | pending |
| `fastb64z` Zig (`Decode Fast`) | `TBD` | `TBD` | pending |
| `FastBase64` Pascal (`Decode`) | `TBD` | `TBD` | pending |
| `lemire/fastbase64` C (`AVX2 validate`) | `TBD` | `TBD` | pending |

## Encode Benchmarks

### 1 MiB (`1048576` bytes)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Rust (`Safe Scalar`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`SSSE3`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX2`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX-512 VBMI`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`Fast Scalar`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`Extreme Fast Scalar`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`SSE`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`AVX2`) | `TBD` | `TBD` | pending |
| `fastb64z` Zig (`Encode Std`) | `TBD` | `TBD` | pending |
| `FastBase64` Pascal (`Encode`) | `TBD` | `TBD` | pending |
| `lemire/fastbase64` C (`AVX2`) | `TBD` | `TBD` | pending |

### 10 KiB (`10240` bytes)

| Implementation | Mean Time | Throughput | Status |
|---|---:|---:|---|
| `oxid64` Rust (`Safe Scalar`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`SSSE3`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX2`) | `TBD` | `TBD` | pending |
| `oxid64` Rust (`AVX-512 VBMI`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`Fast Scalar`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`Extreme Fast Scalar`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`SSE`) | `TBD` | `TBD` | pending |
| Turbo-Base64 C (`AVX2`) | `TBD` | `TBD` | pending |
| `fastb64z` Zig (`Encode Std`) | `TBD` | `TBD` | pending |
| `FastBase64` Pascal (`Encode`) | `TBD` | `TBD` | pending |
| `lemire/fastbase64` C (`AVX2`) | `TBD` | `TBD` | pending |

## Reproduce Benchmarks

Build C baseline:

```bash
just build-c
```

Run benchmark suite:

```bash
just bench
```

Shielded (recommended):

```bash
just bench-shield "Base64 Decoding Strict Compare/Rust Port (AVX2 Strict)/1048576"
just bench-shield "Base64 Decoding Strict Compare/TurboBase64 C (AVX2, check)/1048576"
```

### Scalar sections (structured runs)

Usa una sección por tamaño y corre las 4 filas de la matriz escalar.

Ejemplo plantilla (`<SIZE>` = `1024` / `10240` / `1048576`):

```bash
just bench-shield "Base64 Decoding/Rust Port (Safe Scalar)/<SIZE>"
just bench-shield "Base64 Decoding/Rust Port (Safe Scalar Non-Strict)/<SIZE>" # TODO: when implemented
just bench-shield "Base64 Decoding/TurboBase64 C (auto, check)/<SIZE>"
just bench-shield "Base64 Decoding/TurboBase64 C (auto, no check)/<SIZE>"
```

Recomendación de metodología:
- mismo perfil de CPU para las 4 filas
- mismo `sample-size`, warmup y measurement time
- guardar baseline por tamaño para comparar cambios

## Safety & Verification

SIMD hot paths use `unsafe` intrinsics, so the project tracks safety with layered verification (Swiss-cheese model):

- Kani (proof harnesses): active for small helper contracts
- Miri (UB checks): in progress
- MSan (uninitialized-memory checks): in progress
- Differential fuzzing (`proptest` + `cargo-fuzz` smoke targets): active

Details and commands:
- `doc/safety_verification.md`
- `just verify-safety`
- `just verify-safety-strict`

## Development

- `just test`: run tests
- `just bench`: run Criterion benches
- `just fmt`: format
- `just lint`: clippy with warnings as errors
- `just clean`: clean Rust + C artifacts

## License

MIT
