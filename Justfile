set shell := ["bash", "-c"]

# Build the original Turbo-Base64 C library as a static lib for testing/benchmarking
build-c:
	@echo "Building Turbo-Base64 C library..."
	cd Turbo-Base64 && make libtb64.a CC=clang

# Clean the C library artifacts
clean-c:
	cd Turbo-Base64 && make clean

# Run tests using cargo nextest, linking the C library
test: build-c
	cargo nextest run

# Run benchmarks using criterion, linking the C library
bench: build-c
	cargo bench

# Format Rust code
fmt:
	cargo fmt --all

# Run clippy linter
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Clean both Rust and C artifacts
clean: clean-c
	cargo clean
