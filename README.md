# oxid64

High-performance Base64 library in Rust.

## Features

- Fast scalar implementation with ILP-friendly lookup tables.
- Optimized decoding and encoding paths.
- Performance comparisons with other libraries (like Turbo-Base64).

## Project Structure

- `src/`: Core library and module implementations.
- `benches/`: Performance benchmarks using Criterion.
- `tests/`: Integration tests (Rust and C tests).
- `doc/`: Documentation and reference files.
- `scripts/`: Helper scripts for profiling and benchmarking.

## Usage

Check the `scalar` module for the primary functions:

```rust
use oxid64::scalar::decode_base64_fast;

let mut out = [0u8; 128];
let input = b"SGVsbG8gV29ybGQ=";
if let Some(len) = decode_base64_fast(input, &mut out) {
    println!("Decoded {} bytes", len);
}
```

## Developing

Use the `just` tool for common tasks:

- `just build-c`: Build the C dependency.
- `just test`: Run all tests.
- `just bench`: Run benchmarks.
- `just fmt`: Format code.
- `just lint`: Lint code.
