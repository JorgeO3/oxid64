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

# Safety verification matrix (best-effort local pass).
verify-safety fuzz-cases='5000': build-c
	./scripts/verify_safety.sh --fuzz-cases {{fuzz-cases}}

# Safety verification matrix (strict gate: fails if a layer is missing).
verify-safety-strict fuzz-cases='5000': build-c
	./scripts/verify_safety.sh --strict --fuzz-cases {{fuzz-cases}}

# Run a shielded benchmark for stable/repeatable numbers (desktop Linux).
bench-shield name save-baseline='' baseline='': build-c
	bash ./scripts/bench_shield.sh \
		--cpu 6,7 \
		--run-cpu 7 \
		--performance \
		--stop-irqbalance \
		--leave-irqbalance-stopped \
		--no-aslr \
		--no-turbo \
		--irq-pin \
		--irq-move 129,125 \
		--irq-housekeep "0-5,8-19" \
		--direct-bench base64_bench -- "{{ name }}" \
		--exact --noplot --warm-up-time 10 --measurement-time 30 --sample-size 100 \
		{{ if save-baseline != '' { "--save-baseline $save-baseline" } else { "" } }} \
		{{ if baseline != '' { "--baseline $baseline" } else { "" } }}

# Run bench_shield with custom script/criterion args.
bench-shield-custom *args: build-c
	./scripts/bench_shield.sh {{args}}

# Format Rust code
fmt:
	cargo fmt --all

# Run clippy linter
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Clean both Rust and C artifacts
clean: clean-c
	cargo clean
