# SSSE3 Backend Audit Notes

The SSSE3 backend intentionally exposes two decode contracts:

- `strict` (`CHECK1`-style): correctness-grade path for untrusted input
- `non-strict` (`CHECK0`-style): trusted-input / benchmark-oriented path

## Warning about `non-strict`

`non-strict` does **not** fully validate every 16-byte lane of each DS64 block.
It exists for parity with Turbo-Base64 C and should only be used when the
encoded input is already trusted or has been validated elsewhere.

## What is currently verified

- strict differential tests against scalar
- non-strict schedule characterization on real SSSE3 hardware paths
- bounded partial-write behavior on invalid decode
- canary / exact-window tests for encode and decode
- Miri over SSSE3 schedule models and contracts
- Kani proofs for SSSE3-specific guard/store contracts

## What a green result means

- `strict` SSSE3 is strongly checked for x86_64 host execution
- `non-strict` behavior is documented and regression-tested as a distinct API

It does **not** mean that SSSE3 intrinsics are fully formally proven end to end.
