# MSan Guide

This document defines the practical `MemorySanitizer` scope for `oxid64`.

## What MSan covers here

- scalar decode/encode paths
- tail handlers and padding boundaries
- checked wrappers and preflight contracts
- x86 host tests that execute on `x86_64-unknown-linux-gnu`
- temporaries and exact-window output checks exercised by tests
- the focused scalar/tail suite in `tests/msan_scalar_contracts.rs`

## What MSan does not prove

- semantic correctness of the codec by itself
- general pointer/provenance UB (that is Miri territory)
- `NEON` and `WASM` execution on this host
- every possible SIMD path on every architecture

## Normal run

```bash
just test-msan
```

This must stay green on supported Linux x86_64 hosts.

## Instrumentation smoke

```bash
just test-msan-smoke
```

This intentionally executes `src/bin/msan_smoke.rs`, which performs a
controlled uninitialized read through `memcmp(3)` so we can confirm that MSan
is actually active. The command is expected to fail with the configured MSan
exit code.

## Triage run with origins

```bash
just test-msan-origins
```

Use this only when debugging a real MSan report. It is slower, but it provides
better origin information for where the poisoned bytes came from.

## Current host assumptions

- toolchain: nightly Rust
- target: `x86_64-unknown-linux-gnu`
- `rust-src` installed for `-Zbuild-std`
- Clang/MSan-capable toolchain available on the host

## Interpretation of a green run

If `just test-msan` is green, it means:

- the instrumented test binaries ran successfully
- no uninitialized reads were observed in the covered scope

It does **not** mean that all unsafe code is sound or that all semantic bugs
have been eliminated.
