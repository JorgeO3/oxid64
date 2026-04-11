# AVX-512 VBMI Backend Audit Notes

The AVX-512 VBMI backend intentionally exposes two decode contracts:

- `strict`: correctness-grade path for untrusted input
- `non-strict`: `CHECK0`-style trusted-input path

The encoder is correctness-grade only after its planner/guard invariants hold.

## Warning about `non-strict`

`non-strict` does **not** validate every 64-byte lane of each DS256 block.
It exists for parity with Turbo-Base64 C's `CHECK0` semantics and should only be
used when the Base64 input is already trusted or validated elsewhere.

## Warning about AVX-512 feature gating

The runtime gate must satisfy `avx512f + avx512bw + avx512vbmi`, not just
`avx512vbmi` alone. The backend now requires the full feature set explicitly.

## What is currently verified

- encode planner geometry and required-input thresholds
- strict differential tests against scalar on valid and invalid inputs
- non-strict schedule characterization on AVX-512 hardware paths
- bounded partial-write behavior on invalid decode
- exact-window and canary tests for strict decode and encode
- Miri over AVX512 planner/schedule models
- Kani proofs for AVX512-specific guard/store/planner contracts
- AVX512-specific fuzz targets for strict, non-strict, encode, and partial-write contracts

## What a green result means

- `strict` AVX512 is strongly checked for hosts that actually execute the backend
- `non-strict` behavior is documented and regression-tested as a distinct `CHECK0` API

It does **not** mean that AVX-512 intrinsics are formally proven end to end.
