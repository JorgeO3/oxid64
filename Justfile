set shell := ["bash", "-c"]

default:
	@just --list

build:
	cargo build

test:
	cargo nextest run --lib --tests

doctest:
	cargo test --doc

careful:
	cargo +nightly careful test --lib --tests
	cargo +nightly careful test --doc

miri:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --lib

miri-many-seeds:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-many-seeds=0..4" cargo +nightly miri test --lib

asan:
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc

msan:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc

kani family='all' unwind='8':
	bash -ceu 'family="{{family}}"; family="${family##*=}"; unwind="{{unwind}}"; unwind="${unwind##*=}"; source ./scripts/lane_defs.sh; run_kani_family "$family" "$unwind"'

fuzz-build family='all':
	bash -ceu 'family="{{family}}"; family="${family##*=}"; source ./scripts/lane_defs.sh; build_fuzz_family "$family"'

fuzz-smoke family='x86' runs='32':
	bash -ceu 'family="{{family}}"; family="${family##*=}"; runs="{{runs}}"; runs="${runs##*=}"; source ./scripts/lane_defs.sh; smoke_fuzz_family "$family" "$runs"'

bench:
	cargo bench --features c-benchmarks

verify-safety fuzz_cases='5000' max_lanes='4' jobs='':
	bash -ceu 'fuzz_cases="{{fuzz_cases}}"; fuzz_cases="${fuzz_cases##*=}"; max_lanes="{{max_lanes}}"; max_lanes="${max_lanes##*=}"; jobs="{{jobs}}"; jobs="${jobs##*=}"; if [[ -n "$jobs" ]]; then ./scripts/verify_safety.sh --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes" --jobs "$jobs"; else ./scripts/verify_safety.sh --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes"; fi'

verify-safety-strict fuzz_cases='5000' max_lanes='4' jobs='':
	bash -ceu 'fuzz_cases="{{fuzz_cases}}"; fuzz_cases="${fuzz_cases##*=}"; max_lanes="{{max_lanes}}"; max_lanes="${max_lanes##*=}"; jobs="{{jobs}}"; jobs="${jobs##*=}"; if [[ -n "$jobs" ]]; then ./scripts/verify_safety.sh --strict --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes" --jobs "$jobs"; else ./scripts/verify_safety.sh --strict --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes"; fi'

verify-safety-smoke max_lanes='4' jobs='':
	bash -ceu 'max_lanes="{{max_lanes}}"; max_lanes="${max_lanes##*=}"; jobs="{{jobs}}"; jobs="${jobs##*=}"; if [[ -n "$jobs" ]]; then ./scripts/verify_safety.sh --mode smoke --max-lanes "$max_lanes" --jobs "$jobs"; else ./scripts/verify_safety.sh --mode smoke --max-lanes "$max_lanes"; fi'

verify-safety-smoke-strict max_lanes='4' jobs='':
	bash -ceu 'max_lanes="{{max_lanes}}"; max_lanes="${max_lanes##*=}"; jobs="{{jobs}}"; jobs="${jobs##*=}"; if [[ -n "$jobs" ]]; then ./scripts/verify_safety.sh --mode smoke --strict --max-lanes "$max_lanes" --jobs "$jobs"; else ./scripts/verify_safety.sh --mode smoke --strict --max-lanes "$max_lanes"; fi'

verify-safety-dry-run fuzz_cases='5000' max_lanes='4' jobs='':
	bash -ceu 'fuzz_cases="{{fuzz_cases}}"; fuzz_cases="${fuzz_cases##*=}"; max_lanes="{{max_lanes}}"; max_lanes="${max_lanes##*=}"; jobs="{{jobs}}"; jobs="${jobs##*=}"; if [[ -n "$jobs" ]]; then ./scripts/verify_safety.sh --dry-run --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes" --jobs "$jobs"; else ./scripts/verify_safety.sh --dry-run --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes"; fi'

verify-safety-routed changed_file fuzz_cases='5000' max_lanes='4' jobs='':
	bash -ceu 'changed_file="{{changed_file}}"; changed_file="${changed_file##*=}"; fuzz_cases="{{fuzz_cases}}"; fuzz_cases="${fuzz_cases##*=}"; max_lanes="{{max_lanes}}"; max_lanes="${max_lanes##*=}"; jobs="{{jobs}}"; jobs="${jobs##*=}"; if [[ -n "$jobs" ]]; then ./scripts/verify_safety.sh --changed "$changed_file" --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes" --jobs "$jobs"; else ./scripts/verify_safety.sh --changed "$changed_file" --fuzz-cases "$fuzz_cases" --max-lanes "$max_lanes"; fi'

bench-shield name save-baseline='' baseline='':
	bash ./scripts/bench_shield.sh \
		--cpu 0,1 \
		--run-cpu 1 \
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
		{{ if save-baseline != '' { "--save-baseline " + save-baseline } else { "" } }} \
		{{ if baseline != '' { "--baseline " + baseline } else { "" } }}

bench-shield-custom *args:
	./scripts/bench_shield.sh {{args}}

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings

clean:
	cargo clean
