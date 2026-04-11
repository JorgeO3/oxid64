# AVX2 Backend Audit Notes

The AVX2 backend intentionally exposes three decode contracts:

- `strict`: correctness-grade path for untrusted input
- `non-strict`: `CHECK0`-style trusted-input path
- `unchecked`: trusted benchmark/reference path with minimal validation

## Warning about `non-strict`

`non-strict` does **not** validate every 32-byte lane of each DS128 block.
It exists for parity with Turbo-Base64 C's `CHECK0` semantics and should only be
used when the Base64 input is already trusted or validated elsewhere.

## Warning about `unchecked`

`decode_to_slice_unchecked` is not a full semantic validator. Today it uses the
unchecked AVX2 kernel for the SIMD body, but it still falls back to the scalar
checked decoder for the remaining tail. Treat it as a memory-audited trusted
input API, not as a backend-independent unchecked contract.

## What is currently verified

- strict differential tests against scalar for valid and invalid inputs
- non-strict schedule characterization on real AVX2 hardware paths
- bounded partial-write behavior on invalid decode for strict and non-strict
- canary / exact-window tests for strict decode and unchecked safety cases
- Miri over AVX2 schedule and write-bound models
- Kani proofs for AVX2-specific guard/store contracts
- AVX2-specific fuzz targets for strict, non-strict, unchecked, and partial-write contracts

## What a green result means

- `strict` AVX2 is strongly checked for x86_64 host execution
- `non-strict` behavior is documented and regression-tested as a distinct `CHECK0` API
- `unchecked` is covered for output bounds and trusted-input behavior, not for full validation

It does **not** mean that AVX2 intrinsics are formally proven end to end.
