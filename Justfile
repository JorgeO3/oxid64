set shell := ["bash", "-c"]

# Build the original Turbo-Base64 C library as a static lib for testing/benchmarking
build-c:
	@echo "Building Turbo-Base64 C library..."
	cd Turbo-Base64 && make libtb64.a CC=clang

# Clean the C library artifacts
clean-c:
	cd Turbo-Base64 && make clean

# Run tests using cargo nextest
test:
	cargo nextest run

# Phase 2: run cargo-careful for lib/tests/doctests.
test-careful:
	cargo +nightly careful test --lib --tests
	cargo +nightly careful test --doc

# Native AArch64/ARMv8 only: run NEON integration tests under cargo-careful.
test-careful-neon:
	cargo +nightly careful test --test neon_decode_tests --test neon_encode_tests

# Native AArch64/ARMv8 only: quick cargo-careful smoke for NEON.
test-careful-neon-smoke:
	cargo +nightly careful test --test neon_decode_tests test_neon_decode_valid_padding_at_simd_block_boundaries -- --exact
	cargo +nightly careful test --test neon_encode_tests test_neon_encode_boundary_lengths_match_scalar -- --exact

# WASM SIMD128 runtime tests via wasmtime (compile with +simd128).
test-wasm-runtime:
	CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime run --dir=." RUSTFLAGS="-C target-feature=+simd128" cargo test --target wasm32-wasip1 --test wasm_simd128_decode_tests --test wasm_simd128_encode_tests

# Quick WASM SIMD128 runtime smoke via wasmtime.
test-wasm-runtime-smoke:
	CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime run --dir=." RUSTFLAGS="-C target-feature=+simd128" cargo test --target wasm32-wasip1 --test wasm_simd128_decode_tests test_wasm_decode_valid_alphabet_regression_for_simd -- --exact
	CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime run --dir=." RUSTFLAGS="-C target-feature=+simd128" cargo test --target wasm32-wasip1 --test wasm_simd128_encode_tests test_wasm_encode_boundary_lengths_match_scalar -- --exact

# WASM SIMD128 cargo-careful pass over host-side pure models.
test-careful-wasm:
	cargo +nightly careful test --test wasm_simd128_models

# Quick WASM SIMD128 cargo-careful smoke over host-side pure models.
test-careful-wasm-smoke:
	cargo +nightly careful test --test wasm_simd128_models wasm_pshufb_model_uses_low_nibble_for_ascii_controls -- --exact

# Phase 3: run AddressSanitizer on supported host target.
test-asan:
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc

# Native AArch64/ARMv8 only: run AddressSanitizer over NEON integration tests.
test-asan-neon:
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target aarch64-unknown-linux-gnu --test neon_decode_tests --test neon_encode_tests

# Native AArch64/ARMv8 only: quick AddressSanitizer smoke for NEON.
test-asan-neon-smoke:
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target aarch64-unknown-linux-gnu --test neon_decode_tests test_neon_decode_valid_padding_at_simd_block_boundaries -- --exact
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target aarch64-unknown-linux-gnu --test neon_encode_tests test_neon_encode_boundary_lengths_match_scalar -- --exact

# WASM SIMD128 AddressSanitizer strategy: host-side pure model coverage.
test-asan-wasm:
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --test wasm_simd128_models

# Quick WASM SIMD128 AddressSanitizer smoke over host-side models.
test-asan-wasm-smoke:
	ASAN_OPTIONS=detect_leaks=0 RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --test wasm_simd128_models wasm_pshufb_model_uses_low_nibble_for_ascii_controls -- --exact

# Phase 4: run MemorySanitizer on supported host target.
test-msan:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --doc

# Native AArch64/ARMv8 only: run MemorySanitizer over NEON integration tests.
test-msan-neon:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target aarch64-unknown-linux-gnu --test neon_decode_tests --test neon_encode_tests

# Native AArch64/ARMv8 only: quick MemorySanitizer smoke for NEON.
test-msan-neon-smoke:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target aarch64-unknown-linux-gnu --test neon_decode_tests test_neon_decode_valid_padding_at_simd_block_boundaries -- --exact
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target aarch64-unknown-linux-gnu --test neon_encode_tests test_neon_encode_boundary_lengths_match_scalar -- --exact

# WASM SIMD128 MemorySanitizer strategy: host-side pure model coverage.
test-msan-wasm:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --test wasm_simd128_models

# Quick WASM SIMD128 MemorySanitizer smoke over host-side models.
test-msan-wasm-smoke:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --test wasm_simd128_models wasm_pshufb_model_uses_low_nibble_for_ascii_controls -- --exact

# Phase 4: same as test-msan, but with origin tracking for triage.
test-msan-origins:
	MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" RUSTDOCFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests

# Phase 4: controlled negative run to prove MSan instrumentation is live.
test-msan-smoke:
	bash -ceu 'set +e; MSAN_OPTIONS=halt_on_error=1,exit_code=86,poison_in_dtor=1 RUSTFLAGS="-Zsanitizer=memory" RUSTDOCFLAGS="-Zsanitizer=memory" cargo +nightly run -Zbuild-std --target x86_64-unknown-linux-gnu --bin msan_smoke >/tmp/oxid64-msan-smoke.log 2>&1; status=$?; cat /tmp/oxid64-msan-smoke.log; if [ "$status" -ne 86 ]; then echo "expected MSan smoke to fail with exit code 86, got $status" >&2; exit 1; fi'

# Phase 5: run Miri on contract-focused library tests.
test-miri:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --lib

# SSSE3-specific Miri contract coverage.
test-miri-ssse3:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test ssse3_models

# AVX2-specific Miri contract coverage.
test-miri-avx2:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test avx2_models

# AVX512-specific Miri contract coverage.
test-miri-avx512:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test avx512_vbmi_models

# NEON-specific Miri model coverage.
test-miri-neon:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test neon_models

# WASM-specific Miri model coverage.
test-miri-wasm:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test wasm_simd128_models

# Quick Miri smoke for NEON models.
test-miri-neon-smoke:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test neon_models neon_encode_model_required_input_matches_block_boundaries -- --exact

# Quick Miri smoke for WASM models.
test-miri-wasm-smoke:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri test --test wasm_simd128_models wasm_pshufb_model_uses_low_nibble_for_ascii_controls -- --exact

# Phase 5: widen Miri exploration with multiple seeds.
test-miri-many-seeds:
	MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance -Zmiri-many-seeds=0..4" cargo +nightly miri test --lib

# Phase 5: controlled negative run to prove Miri instrumentation is active.
test-miri-smoke:
	bash -ceu 'set +e; MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" cargo +nightly miri run --bin miri_smoke >/tmp/oxid64-miri-smoke.log 2>&1; status=$?; cat /tmp/oxid64-miri-smoke.log; if [ "$status" -eq 0 ]; then echo "expected Miri smoke to fail with UB report" >&2; exit 1; fi; if ! grep -q "Undefined Behavior" /tmp/oxid64-miri-smoke.log; then echo "expected Miri smoke log to mention Undefined Behavior" >&2; exit 1; fi'

# Phase 6: run Kani proofs for small helper contracts.
test-kani:
	cargo kani --default-unwind 8 --harness remaining_matches_pointer_delta_within_allocation
	cargo kani --default-unwind 8 --harness can_read_and_can_write_agree_with_remaining
	cargo kani --default-unwind 8 --harness safe_in_end_for_4_within_allocation
	cargo kani --default-unwind 8 --harness safe_in_end_for_16_within_allocation
	cargo kani --default-unwind 8 --harness guard_helpers_imply_required_io_room
	cargo kani --default-unwind 8 --harness prepare_decode_output_matches_decoded_len_contract
	cargo kani --default-unwind 8 --harness decoded_and_encoded_lengths_stay_bounded_in_small_domain
	cargo kani --default-unwind 8 --harness decode_offsets_monotonic_and_dispatch_contracts_hold
	cargo kani --default-unwind 8 --harness ssse3_ds64_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness ssse3_ds64_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness ssse3_tail16_store_fits_guarded_output
	cargo kani --default-unwind 8 --harness ssse3_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness ssse3_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx2_ds128_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx2_ds128_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx2_ds128_triple_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx2_tail16_store_fits_guarded_output
	cargo kani --default-unwind 8 --harness avx2_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness avx2_strict_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx2_partial_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx2_unchecked_preflight_matches_documented_contract
	cargo kani --default-unwind 8 --harness avx512_ds256_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx512_ds256_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx512_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness avx512_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx512_encode_schedule_is_contiguous_without_gaps
	cargo kani --default-unwind 8 --harness avx512_encode_required_input_matches_farthest_load
	cargo kani --default-unwind 8 --harness wasm_ds64_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness wasm_ds64_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness wasm_tail16_store_fits_guarded_output
	cargo kani --default-unwind 8 --harness wasm_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness wasm_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness wasm_pshufb_model_matches_low_nibble_spec
	cargo kani --default-unwind 8 --harness wasm_encode_prefix_consumes_only_full_blocks
	cargo kani --default-unwind 8 --harness wasm_encode_required_input_matches_block_boundaries

# SSSE3-only Kani subset.
test-kani-ssse3:
	cargo kani --default-unwind 8 --harness ssse3_ds64_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness ssse3_ds64_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness ssse3_tail16_store_fits_guarded_output
	cargo kani --default-unwind 8 --harness ssse3_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness ssse3_written_prefix_model_is_bounded_by_decoded_output

# AVX2-only Kani subset.
test-kani-avx2:
	cargo kani --default-unwind 8 --harness avx2_ds128_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx2_ds128_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx2_ds128_triple_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx2_tail16_store_fits_guarded_output
	cargo kani --default-unwind 8 --harness avx2_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness avx2_strict_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx2_partial_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx2_unchecked_preflight_matches_documented_contract

# AVX512-only Kani subset.
test-kani-avx512:
	cargo kani --default-unwind 8 --harness avx512_ds256_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx512_ds256_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness avx512_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness avx512_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness avx512_encode_schedule_is_contiguous_without_gaps
	cargo kani --default-unwind 8 --harness avx512_encode_required_input_matches_farthest_load
	cargo kani --default-unwind 8 --harness neon_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness neon_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness neon_encode_prefix_consumes_only_full_blocks
	cargo kani --default-unwind 8 --harness neon_encode_required_input_matches_block_boundaries

# NEON-only Kani model subset.
test-kani-neon:
	cargo kani --default-unwind 8 --harness neon_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness neon_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness neon_encode_prefix_consumes_only_full_blocks
	cargo kani --default-unwind 8 --harness neon_encode_required_input_matches_block_boundaries

# WASM-only Kani model subset.
test-kani-wasm:
	cargo kani --default-unwind 8 --harness wasm_ds64_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness wasm_ds64_double_store_offsets_fit_guarded_output
	cargo kani --default-unwind 8 --harness wasm_tail16_store_fits_guarded_output
	cargo kani --default-unwind 8 --harness wasm_non_strict_schedule_model_matches_documented_lanes
	cargo kani --default-unwind 8 --harness wasm_written_prefix_model_is_bounded_by_decoded_output
	cargo kani --default-unwind 8 --harness wasm_pshufb_model_matches_low_nibble_spec
	cargo kani --default-unwind 8 --harness wasm_encode_prefix_consumes_only_full_blocks
	cargo kani --default-unwind 8 --harness wasm_encode_required_input_matches_block_boundaries

# Quick Kani smoke for NEON models.
test-kani-neon-smoke:
	cargo kani --default-unwind 8 --harness neon_encode_required_input_matches_block_boundaries

# Quick Kani smoke for WASM models.
test-kani-wasm-smoke:
	cargo kani --default-unwind 8 --harness wasm_pshufb_model_matches_low_nibble_spec

# Phase 7: build all fuzz targets.
fuzz-build:
	cargo +nightly fuzz build decode_diff
	cargo +nightly fuzz build encode_diff
	cargo +nightly fuzz build roundtrip
	cargo +nightly fuzz build invalid_semantics
	cargo +nightly fuzz build tail_alignment
	cargo +nightly fuzz build ssse3_strict_diff
	cargo +nightly fuzz build ssse3_non_strict_schedule
	cargo +nightly fuzz build avx2_strict_diff
	cargo +nightly fuzz build avx2_non_strict_schedule
	cargo +nightly fuzz build avx2_unchecked_contract
	cargo +nightly fuzz build avx2_partial_write_bounds
	cargo +nightly fuzz build avx512_strict_diff
	cargo +nightly fuzz build avx512_non_strict_schedule
	cargo +nightly fuzz build avx512_encode_diff
	cargo +nightly fuzz build avx512_partial_write_bounds
	cargo +nightly fuzz build neon_strict_diff
	cargo +nightly fuzz build neon_non_strict_schedule
	cargo +nightly fuzz build neon_encode_diff
	cargo +nightly fuzz build neon_partial_write_bounds
	cargo +nightly fuzz build wasm_pshufb_compat
	cargo +nightly fuzz build wasm_non_strict_schedule
	cargo +nightly fuzz build wasm_encode_prefix_model
	cargo +nightly fuzz build wasm_partial_write_model

# Phase 7: build AVX2-specific fuzz targets.
fuzz-build-avx2:
	cargo +nightly fuzz build avx2_strict_diff
	cargo +nightly fuzz build avx2_non_strict_schedule
	cargo +nightly fuzz build avx2_unchecked_contract
	cargo +nightly fuzz build avx2_partial_write_bounds

# Phase 7: build AVX512-specific fuzz targets.
fuzz-build-avx512:
	cargo +nightly fuzz build avx512_strict_diff
	cargo +nightly fuzz build avx512_non_strict_schedule
	cargo +nightly fuzz build avx512_encode_diff
	cargo +nightly fuzz build avx512_partial_write_bounds

# Phase 7: build NEON-specific fuzz targets.
fuzz-build-neon:
	cargo +nightly fuzz build neon_strict_diff
	cargo +nightly fuzz build neon_non_strict_schedule
	cargo +nightly fuzz build neon_encode_diff
	cargo +nightly fuzz build neon_partial_write_bounds

# Phase 7: build WASM-specific fuzz/model targets.
fuzz-build-wasm:
	cargo +nightly fuzz build wasm_pshufb_compat
	cargo +nightly fuzz build wasm_non_strict_schedule
	cargo +nightly fuzz build wasm_encode_prefix_model
	cargo +nightly fuzz build wasm_partial_write_model

# Phase 7: short smoke run for a single fuzz target.
fuzz-smoke target='decode_diff' runs='64':
	bash -ceu 'target="{{target}}"; target="${target##*=}"; runs="{{runs}}"; runs="${runs##*=}"; cargo +nightly fuzz run "${target}" "fuzz/corpus/${target}" -- -runs="${runs}"'

# Phase 7: short smoke run across the current fuzz target set.
fuzz-smoke-all runs='32':
	bash -ceu 'runs="{{runs}}"; runs="${runs##*=}"; cargo +nightly fuzz run decode_diff fuzz/corpus/decode_diff -- -runs="${runs}"; cargo +nightly fuzz run encode_diff fuzz/corpus/encode_diff -- -runs="${runs}"; cargo +nightly fuzz run roundtrip fuzz/corpus/roundtrip -- -runs="${runs}"; cargo +nightly fuzz run invalid_semantics fuzz/corpus/invalid_semantics -- -runs="${runs}"; cargo +nightly fuzz run tail_alignment fuzz/corpus/tail_alignment -- -runs="${runs}"; cargo +nightly fuzz run ssse3_strict_diff fuzz/corpus/ssse3_strict_diff -- -runs="${runs}"; cargo +nightly fuzz run ssse3_non_strict_schedule fuzz/corpus/ssse3_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run avx2_strict_diff fuzz/corpus/avx2_strict_diff -- -runs="${runs}"; cargo +nightly fuzz run avx2_non_strict_schedule fuzz/corpus/avx2_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run avx2_unchecked_contract fuzz/corpus/avx2_unchecked_contract -- -runs="${runs}"; cargo +nightly fuzz run avx2_partial_write_bounds fuzz/corpus/avx2_partial_write_bounds -- -runs="${runs}"; cargo +nightly fuzz run avx512_strict_diff fuzz/corpus/avx512_strict_diff -- -runs="${runs}"; cargo +nightly fuzz run avx512_non_strict_schedule fuzz/corpus/avx512_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run avx512_encode_diff fuzz/corpus/avx512_encode_diff -- -runs="${runs}"; cargo +nightly fuzz run avx512_partial_write_bounds fuzz/corpus/avx512_partial_write_bounds -- -runs="${runs}"; cargo +nightly fuzz run wasm_pshufb_compat fuzz/corpus/wasm_pshufb_compat -- -runs="${runs}"; cargo +nightly fuzz run wasm_non_strict_schedule fuzz/corpus/wasm_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run wasm_encode_prefix_model fuzz/corpus/wasm_encode_prefix_model -- -runs="${runs}"; cargo +nightly fuzz run wasm_partial_write_model fuzz/corpus/wasm_partial_write_model -- -runs="${runs}"'

# Phase 7: short smoke run across AVX2-specific fuzz targets.
fuzz-smoke-avx2 runs='32':
	bash -ceu 'runs="{{runs}}"; runs="${runs##*=}"; cargo +nightly fuzz run avx2_strict_diff fuzz/corpus/avx2_strict_diff -- -runs="${runs}"; cargo +nightly fuzz run avx2_non_strict_schedule fuzz/corpus/avx2_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run avx2_unchecked_contract fuzz/corpus/avx2_unchecked_contract -- -runs="${runs}"; cargo +nightly fuzz run avx2_partial_write_bounds fuzz/corpus/avx2_partial_write_bounds -- -runs="${runs}"'

# Phase 7: short smoke run across AVX512-specific fuzz targets.
fuzz-smoke-avx512 runs='32':
	bash -ceu 'runs="{{runs}}"; runs="${runs##*=}"; cargo +nightly fuzz run avx512_strict_diff fuzz/corpus/avx512_strict_diff -- -runs="${runs}"; cargo +nightly fuzz run avx512_non_strict_schedule fuzz/corpus/avx512_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run avx512_encode_diff fuzz/corpus/avx512_encode_diff -- -runs="${runs}"; cargo +nightly fuzz run avx512_partial_write_bounds fuzz/corpus/avx512_partial_write_bounds -- -runs="${runs}"'

# Phase 7: short smoke run across NEON-specific fuzz targets.
# Run this on native AArch64/ARMv8 hardware so the backend is actually exercised.
fuzz-smoke-neon runs='32':
	bash -ceu 'runs="{{runs}}"; runs="${runs##*=}"; cargo +nightly fuzz run neon_strict_diff fuzz/corpus/neon_strict_diff -- -runs="${runs}"; cargo +nightly fuzz run neon_non_strict_schedule fuzz/corpus/neon_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run neon_encode_diff fuzz/corpus/neon_encode_diff -- -runs="${runs}"; cargo +nightly fuzz run neon_partial_write_bounds fuzz/corpus/neon_partial_write_bounds -- -runs="${runs}"'

# Phase 7: short smoke run across WASM-specific fuzz/model targets.
fuzz-smoke-wasm runs='32':
	bash -ceu 'runs="{{runs}}"; runs="${runs##*=}"; cargo +nightly fuzz run wasm_pshufb_compat fuzz/corpus/wasm_pshufb_compat -- -runs="${runs}"; cargo +nightly fuzz run wasm_non_strict_schedule fuzz/corpus/wasm_non_strict_schedule -- -runs="${runs}"; cargo +nightly fuzz run wasm_encode_prefix_model fuzz/corpus/wasm_encode_prefix_model -- -runs="${runs}"; cargo +nightly fuzz run wasm_partial_write_model fuzz/corpus/wasm_partial_write_model -- -runs="${runs}"'

# Native AArch64/ARMv8 server checklist for the NEON backend.
verify-neon-aarch64 runs='32':
	cargo test --test neon_decode_tests --test neon_encode_tests
	just test-careful-neon
	just test-asan-neon
	just test-msan-neon
	just test-miri-neon
	just test-kani-neon
	just fuzz-build-neon
	just fuzz-smoke-neon runs={{runs}}

# Native AArch64/ARMv8 quick smoke across the NEON safety stack.
verify-neon-aarch64-smoke runs='8':
	cargo test --test neon_decode_tests test_neon_decode_valid_padding_at_simd_block_boundaries -- --exact
	cargo test --test neon_encode_tests test_neon_encode_boundary_lengths_match_scalar -- --exact
	just test-careful-neon-smoke
	just test-asan-neon-smoke
	just test-msan-neon-smoke
	just test-miri-neon-smoke
	just test-kani-neon-smoke
	just fuzz-build-neon
	just fuzz-smoke-neon runs={{runs}}

# WASM SIMD128 full verification pass via wasmtime plus host-side models.
verify-wasm-simd128 runs='64':
	just test-wasm-runtime
	just test-careful-wasm
	just test-asan-wasm
	just test-msan-wasm
	just test-miri-wasm
	just test-kani-wasm
	just fuzz-build-wasm
	just fuzz-smoke-wasm runs={{runs}}

# WASM SIMD128 quick smoke via wasmtime plus host-side models.
verify-wasm-simd128-smoke runs='8':
	just test-wasm-runtime-smoke
	just test-careful-wasm-smoke
	just test-asan-wasm-smoke
	just test-msan-wasm-smoke
	just test-miri-wasm-smoke
	just test-kani-wasm-smoke
	just fuzz-build-wasm
	just fuzz-smoke-wasm runs={{runs}}

# Run benchmarks using criterion (C libraries are built automatically by build.rs)
bench:
	cargo bench --features c-benchmarks

# Safety verification matrix (best-effort local pass, full mode).
# Lanes run in parallel with isolated target dirs.
verify-safety fuzz_cases='5000' max_lanes='4' jobs='':
	./scripts/verify_safety.sh --fuzz-cases {{fuzz_cases}} --max-lanes {{max_lanes}} {{ if jobs != '' { "--jobs " + jobs } else { "" } }}

# Safety verification matrix (strict gate: fails if a layer is missing).
verify-safety-strict fuzz_cases='5000' max_lanes='4' jobs='':
	./scripts/verify_safety.sh --strict --fuzz-cases {{fuzz_cases}} --max-lanes {{max_lanes}} {{ if jobs != '' { "--jobs " + jobs } else { "" } }}

# Quick safety smoke (~2 min): lint, core tests, Miri lib, one Kani per backend.
verify-safety-smoke max_lanes='4':
	./scripts/verify_safety.sh --mode smoke --max-lanes {{max_lanes}}

# Quick safety smoke (strict: fail on missing tools).
verify-safety-smoke-strict max_lanes='4':
	./scripts/verify_safety.sh --mode smoke --strict --max-lanes {{max_lanes}}

# Show which lanes would run without executing anything.
verify-safety-dry-run fuzz_cases='5000':
	./scripts/verify_safety.sh --dry-run --fuzz-cases {{fuzz_cases}}

# Run safety with routing: only verify backends affected by changed files.
# Example: git diff --name-only HEAD~1 > /tmp/changed.txt && just verify-safety-routed /tmp/changed.txt
verify-safety-routed changed_file fuzz_cases='5000' max_lanes='4':
	./scripts/verify_safety.sh --changed {{changed_file}} --fuzz-cases {{fuzz_cases}} --max-lanes {{max_lanes}}

# Phase 0: environment/tooling baseline report for safety stack.
safety-phase0-report report='doc/safety/baseline.md':
	./scripts/safety_phase0_baseline.sh "{{report}}"

# Phase 0: baseline report + current safety matrix run.
safety-phase0 fuzz_cases='5000' report='doc/safety/baseline.md':
	./scripts/safety_phase0_baseline.sh "{{report}}"
	./scripts/verify_safety.sh --fuzz-cases {{fuzz_cases}}

# Run a shielded benchmark for stable/repeatable numbers (desktop Linux).
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
		{{ if save-baseline != '' { "--save-baseline $save-baseline" } else { "" } }} \
		{{ if baseline != '' { "--baseline $baseline" } else { "" } }}

# Run bench_shield with custom script/criterion args.
bench-shield-custom *args:
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
