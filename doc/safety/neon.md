# NEON Backend Audit Notes

The NEON backend intentionally exposes two decode contracts:

- `strict`: correctness-grade path for untrusted input
- `non-strict`: `CHECK0`-style trusted-input path

The encoder is correctness-grade only after its input-based block planner
invariants hold.

## Warning about `non-strict`

`non-strict` does **not** validate every 64-byte lane of each DN256 group.
It exists for parity with Turbo-Base64 C's `CHECK0` semantics and should only
be used when the Base64 input is already trusted or validated elsewhere.

## Warning about the final semantic quantum

The raw NEON decode block does not understand Base64 padding by itself. The
backend therefore reserves the final 64-byte block for scalar cleanup so the
semantic last quartet (including any `=` padding) is never handled by the raw
SIMD block decoder.

## What is currently verified

- encode planner boundaries and exact 48/96-byte required-input thresholds
- strict invalid-input differential tests, including high-bit bytes
- valid padded decode at 64-byte block boundaries
- non-strict schedule characterization on NEON hardware paths
- bounded partial-write behavior on invalid decode
- exact-window and canary tests for strict decode and encode
- Miri over NEON planner/schedule models
- Kani proofs for NEON schedule and encode-planner contracts
- NEON-specific fuzz targets for strict, non-strict, encode, and partial-write contracts

## Native AArch64 required for runtime NEON execution

This workstation is `x86_64`, so the real NEON runtime suite is intentionally
left wired but not claimed as executed here.

Run the full backend pass on an `AArch64` / ARMv8 server with NEON using:

```bash
just verify-neon-aarch64 runs=64
```

For a quick confidence pass before the full campaign:

```bash
just verify-neon-aarch64-smoke runs=8
```

If you want to run the layers individually on the server:

```bash
cargo test --test neon_decode_tests --test neon_encode_tests
just test-careful-neon
just test-careful-neon-smoke
just test-asan-neon
just test-asan-neon-smoke
just test-msan-neon
just test-msan-neon-smoke
just test-miri-neon
just test-miri-neon-smoke
just test-kani-neon
just test-kani-neon-smoke
just fuzz-build-neon
just fuzz-smoke-neon runs=64
```

## What a green result means

- `strict` NEON is strongly checked for hosts that actually execute the backend
- `non-strict` behavior is documented and regression-tested as a distinct `CHECK0` API

It does **not** mean that NEON intrinsics are formally proven end to end.
