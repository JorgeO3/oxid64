# Safety Phase 0 Baseline

Generated: 2026-03-17T10:18:20-05:00

## Stack Order (mandatory)

1. cargo-careful
2. ASan
3. MSan
4. Miri
5. Kani
6. cargo-fuzz

## Toolchain

- cargo: cargo 1.94.0 (85eff7c80 2026-01-15)
- rustup: rustup 1.29.0 (28d1352db 2026-03-05)
- rustc (active): rustc 1.94.0 (4a4ef493e 2026-03-02)
- cargo nightly: cargo 1.96.0-nightly (f298b8c82 2026-02-24)

### Installed toolchains

- stable-x86_64-unknown-linux-gnu (active, default)
- nightly-x86_64-unknown-linux-gnu
- nightly-2025-11-21-x86_64-unknown-linux-gnu
- 1.82.0-x86_64-unknown-linux-gnu
- 1.94.0-x86_64-unknown-linux-gnu

### Installed targets

- stable targets:
  - wasm32-unknown-unknown
  - wasm32-wasip1
  - x86_64-unknown-linux-gnu
- nightly targets:
  - x86_64-unknown-linux-gnu

### Nightly components

- cargo-x86_64-unknown-linux-gnu
- clippy-x86_64-unknown-linux-gnu
- miri-x86_64-unknown-linux-gnu
- rust-docs-x86_64-unknown-linux-gnu
- rust-src
- rust-std-x86_64-unknown-linux-gnu
- rustc-x86_64-unknown-linux-gnu
- rustfmt-x86_64-unknown-linux-gnu

## Required binaries

- cargo-careful: installed (/home/jorge/.cargo/bin/cargo-careful)
- cargo-fuzz: installed (/home/jorge/.cargo/bin/cargo-fuzz)
- cargo-miri: installed (/home/jorge/.cargo/bin/cargo-miri)
- cargo-kani: installed (/home/jorge/.cargo/bin/cargo-kani)
- clang: installed (/usr/bin/clang)
- llvm-symbolizer: installed (/usr/bin/llvm-symbolizer)
- just: installed (/home/jorge/.cargo/bin/just)
- make: installed (/usr/bin/make)

## Optional binaries (final phase)

- cargo-geiger: installed (/home/jorge/.cargo/bin/cargo-geiger)
- valgrind: installed (/usr/bin/valgrind)
- wasmtime: installed (/home/jorge/.wasmtime/bin/wasmtime)
- qemu-aarch64: missing

## Quick command checks

- cargo +nightly miri --help: ok
- cargo kani --version: cargo-kani 0.67.0
- cargo-fuzz -V: cargo-fuzz 0.13.1
- cargo-careful: binary detected (run via cargo +nightly careful ...)

## Notes

- This report only captures environment readiness (Phase 0), not correctness claims.
- Use just verify-safety for the current matrix smoke run.
- Use MSAN_ENABLED=1 just verify-safety to enable MSan in the existing script.
