# AGENTS.md

This repo builds a Rust Base64 library inspired by Turbo-Base64 C. Metrics
comparisons use the C implementation; algorithmic ideas are derived from it.
Priorities: correctness first, then performance.

If you are an agent, follow these rules and commands before coding.

## Quick Start
- Use `just` for common tasks.
- Build the Turbo-Base64 C library before tests/benchmarks.
- Prefer strict correctness paths unless a task explicitly targets non-strict.

## Build, Lint, Test

### Justfile (preferred)
- Build C dependency: `just build-c`
- Run tests (nextest): `just test`
- Benchmarks (criterion): `just bench`
- Format: `just fmt`
- Lint: `just lint`
- Clean all: `just clean`

### Cargo equivalents
- Build: `cargo build`
- Build (release): `cargo build --release`
- Test (default): `cargo test`
- Test with nextest: `cargo nextest run`
- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Bench: `cargo bench`

### Single test (Rust)
- Unit or integration test: `cargo test <test_name>`
- Integration test file: `cargo test --test sse_decode_tests`
- Specific integration test: `cargo test --test sse_decode_tests test_sse_decode_invalid_chars -- --exact`
- Proptest case (filter): `cargo test proptest:: -- --nocapture`

### C dependency
- C static lib for comparisons: `just build-c` (wraps `make libtb64.a` in `Turbo-Base64/`)
- If you only need Rust builds, you can skip `build-c` but benches and some
  comparisons require it.

## Benchmarks and Performance Tooling

### Criterion benches
- Run all benches: `cargo bench`
- Single bench: `cargo bench --bench base64_bench -- "Base64 Decoding/Rust Port (SSSE3 Strict)/1048576" --exact --noplot`

### SSE strict harness
- Harness (asm + bench + perf): `scripts/sse_strict_harness.sh --variant strict`
- Sweep variants and rank: `scripts/sse_strict_sweep.sh --rounds 2 --top-k 4`

### Perf decode report
- Full perf report: `scripts/perf_decode_report.sh`

### Bench shielding (repeatability)
- Use `scripts/bench_shield.sh` for CPU isolation and stable measurements.
- Example:
  `./scripts/bench_shield.sh --cpu 6,7 --run-cpu 7 --performance --no-turbo --direct-bench base64_bench -- "Base64 Decoding/Rust Port (Safe Scalar)/1048576" --exact --noplot`

## Repo Layout
- `src/lib.rs`: crate root, exports `engine` and `Decoder`.
- `src/engine/`: scalar + SIMD engines (SSSE3, AVX2).
- `benches/base64_bench.rs`: Criterion benchmarks, includes Turbo-Base64 C calls.
- `tests/`: integration tests for SIMD correctness.
- `scripts/`: performance harnesses and bench shielding utilities.
- `Turbo-Base64/`: embedded C library used for comparisons and metrics.

## Code Style and Conventions

### Formatting
- Rustfmt is the standard. Use `cargo fmt --all` before sending diffs.
- Keep line length reasonable and follow existing style (inline doc comments,
  section dividers).

### Imports
- Group imports by module and keep `use` statements local to module scope.
- Prefer explicit imports over glob imports.
- Standard pattern: `use super::...;` then `use core::arch::...` for SIMD.

### Naming
- Modules: `snake_case` (e.g., `ssse3`, `avx2`).
- Types: `CamelCase` (e.g., `Ssse3Decoder`, `DecodeOpts`).
- Functions: `snake_case` (e.g., `decode_base64_fast`).
- Constants: `SCREAMING_SNAKE_CASE` for tables and flags.
- Bench labels are human-readable; keep them stable for comparisons.

### Types and APIs
- Public APIs return `Option<usize>` for decode failures; avoid panics.
- Prefer `usize` for lengths and indices.
- Use `const fn` for compile-time tables and helpers.
- Use `#[inline]` or `#[inline(always)]` only when it matches existing hot-path
  patterns.

### Error Handling
- Decode returns `None` on invalid Base64 or invalid padding/length.
- Avoid partial writes on error; follow existing accumulator patterns.
- Maintain strict vs non-strict semantics in SSSE3 decoder.

### Safety and SIMD
- Use `target_feature` gates for SIMD paths.
- Guard runtime dispatch with `is_x86_feature_detected!`.
- Keep unsafe blocks minimal, with clear safety comments.
- Avoid introducing extra XMM register pressure without benchmarking.

### Performance Notes
- SIMD paths have tight hot loops; changes must be benchmarked.
- Use existing harness scripts to validate ASM and perf regressions.
- Keep scalar fallback correct and aligned with strict decoding rules.

## Turbo-Base64 C Integration
- The C library is used for reference metrics and comparison.
- `build.rs` links `Turbo-Base64/libtb64.a` and builds additional
  NB64CHECK/B64CHECK variants with symbol suffixes for bench use.
- If adding new C symbols, update `TB64_PUBLIC_SYMBOLS` in `build.rs`.

## Cursor / Copilot Rules
- No `.cursor/rules/`, `.cursorrules`, or `.github/copilot-instructions.md`
  were found in this repo.

## Contribution Guidelines for Agents
- Correctness is the top priority; do not trade correctness for speed.
- Any performance change must include before/after numbers in your notes.
- Keep benchmarks and labels stable unless you are intentionally updating them.
