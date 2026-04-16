#!/usr/bin/env bash
# lane_defs.sh — Single source of truth for verification lane inventories.
#
# Sourced by verify_safety.sh and usable by CI workflows.
# Every list here must match the actual repo contents; adding a Kani harness
# or fuzz target without updating this file is a bug.

# ═══════════════════════════════════════════════════════════════════════════════
# Kani harness families
# ═══════════════════════════════════════════════════════════════════════════════

KANI_CORE=(
  remaining_matches_pointer_delta_within_allocation
  can_read_and_can_write_agree_with_remaining
  safe_in_end_for_4_within_allocation
  safe_in_end_for_16_within_allocation
  guard_helpers_imply_required_io_room
  prepare_decode_output_matches_decoded_len_contract
  decoded_and_encoded_lengths_stay_bounded_in_small_domain
  decode_offsets_monotonic_and_dispatch_contracts_hold
)

KANI_SSSE3=(
  ssse3_ds64_store_offsets_fit_guarded_output
  ssse3_ds64_double_store_offsets_fit_guarded_output
  ssse3_tail16_store_fits_guarded_output
  ssse3_non_strict_schedule_model_matches_documented_lanes
  ssse3_written_prefix_model_is_bounded_by_decoded_output
)

KANI_AVX2=(
  avx2_ds128_store_offsets_fit_guarded_output
  avx2_ds128_double_store_offsets_fit_guarded_output
  avx2_ds128_triple_store_offsets_fit_guarded_output
  avx2_tail16_store_fits_guarded_output
  avx2_non_strict_schedule_model_matches_documented_lanes
  avx2_strict_written_prefix_model_is_bounded_by_decoded_output
  avx2_partial_written_prefix_model_is_bounded_by_decoded_output
  avx2_unchecked_preflight_matches_documented_contract
)

KANI_AVX512=(
  avx512_ds256_store_offsets_fit_guarded_output
  avx512_ds256_double_store_offsets_fit_guarded_output
  avx512_non_strict_schedule_model_matches_documented_lanes
  avx512_written_prefix_model_is_bounded_by_decoded_output
  avx512_encode_schedule_is_contiguous_without_gaps
  avx512_encode_required_input_matches_farthest_load
)

KANI_NEON=(
  neon_non_strict_schedule_model_matches_documented_lanes
  neon_written_prefix_model_is_bounded_by_decoded_output
  neon_encode_prefix_consumes_only_full_blocks
  neon_encode_required_input_matches_block_boundaries
)

KANI_WASM=(
  wasm_ds64_store_offsets_fit_guarded_output
  wasm_ds64_double_store_offsets_fit_guarded_output
  wasm_tail16_store_fits_guarded_output
  wasm_non_strict_schedule_model_matches_documented_lanes
  wasm_written_prefix_model_is_bounded_by_decoded_output
  wasm_pshufb_model_matches_low_nibble_spec
  wasm_encode_prefix_consumes_only_full_blocks
  wasm_encode_required_input_matches_block_boundaries
)

KANI_ALL=(
  "${KANI_CORE[@]}"
  "${KANI_SSSE3[@]}"
  "${KANI_AVX2[@]}"
  "${KANI_AVX512[@]}"
  "${KANI_NEON[@]}"
  "${KANI_WASM[@]}"
)

# Smoke representatives: one per family.
KANI_SMOKE_CORE=( remaining_matches_pointer_delta_within_allocation )
KANI_SMOKE_SSSE3=( ssse3_ds64_store_offsets_fit_guarded_output )
KANI_SMOKE_AVX2=( avx2_ds128_store_offsets_fit_guarded_output )
KANI_SMOKE_AVX512=( avx512_ds256_store_offsets_fit_guarded_output )
KANI_SMOKE_NEON=( neon_non_strict_schedule_model_matches_documented_lanes )
KANI_SMOKE_WASM=( wasm_ds64_store_offsets_fit_guarded_output )

KANI_SMOKE=(
  "${KANI_SMOKE_CORE[@]}"
  "${KANI_SMOKE_SSSE3[@]}"
  "${KANI_SMOKE_AVX2[@]}"
  "${KANI_SMOKE_AVX512[@]}"
  "${KANI_SMOKE_NEON[@]}"
  "${KANI_SMOKE_WASM[@]}"
)

# ═══════════════════════════════════════════════════════════════════════════════
# Fuzz target families
# ═══════════════════════════════════════════════════════════════════════════════

FUZZ_COMMON=(
  decode_diff encode_diff roundtrip invalid_semantics tail_alignment
)

FUZZ_SSSE3=(
  ssse3_strict_diff ssse3_non_strict_schedule
)

FUZZ_AVX2=(
  avx2_strict_diff avx2_non_strict_schedule avx2_unchecked_contract avx2_partial_write_bounds
)

FUZZ_AVX512=(
  avx512_strict_diff avx512_non_strict_schedule avx512_encode_diff avx512_partial_write_bounds
)

FUZZ_NEON=(
  neon_strict_diff neon_non_strict_schedule neon_encode_diff neon_partial_write_bounds
)

FUZZ_WASM=(
  wasm_pshufb_compat wasm_non_strict_schedule wasm_encode_prefix_model wasm_partial_write_model
)

FUZZ_X86=( "${FUZZ_COMMON[@]}" "${FUZZ_SSSE3[@]}" "${FUZZ_AVX2[@]}" "${FUZZ_AVX512[@]}" )

FUZZ_ALL=(
  "${FUZZ_COMMON[@]}"
  "${FUZZ_SSSE3[@]}"
  "${FUZZ_AVX2[@]}"
  "${FUZZ_AVX512[@]}"
  "${FUZZ_NEON[@]}"
  "${FUZZ_WASM[@]}"
)

# Smoke representatives for fuzz build check.
FUZZ_SMOKE=( decode_diff roundtrip ssse3_strict_diff avx2_strict_diff )

# ═══════════════════════════════════════════════════════════════════════════════
# Miri shard families
# ═══════════════════════════════════════════════════════════════════════════════

MIRI_SHARD_LIB=( --lib )

MIRI_SHARD_CONTRACTS=(
  --test scalar_contracts
  --test common_contracts
  --test msan_scalar_contracts
)

MIRI_SHARD_X86_MODELS=(
  --test ssse3_models
  --test avx2_models
  --test avx512_vbmi_models
)

MIRI_SHARD_OTHER_MODELS=(
  --test neon_models
  --test wasm_simd128_models
)

MIRI_SHARD_PROPTEST=(
  --test proptest
  --test simd_fuzz_strict
)

MIRI_SHARD_X86_INTEGRATION=(
  --test sse_decode_tests
  --test sse_encode_tests
  --test avx2_decode_tests
  --test avx2_encode_tests
  --test avx512_vbmi_decode_tests
  --test avx512_vbmi_encode_tests
)

# ═══════════════════════════════════════════════════════════════════════════════
# Proptest test bins (used for extended property testing with higher PROPTEST_CASES)
# ═══════════════════════════════════════════════════════════════════════════════

PROPTEST_X86_BINS=(
  --test sse_decode_tests
  --test avx2_decode_tests
  --test avx512_vbmi_decode_tests
  --test sse_encode_tests
  --test avx2_encode_tests
  --test avx512_vbmi_encode_tests
  --test simd_fuzz_strict
  --test proptest
)

PROPTEST_NEON_BINS=(
  --test neon_decode_tests
  --test neon_encode_tests
)

# ═══════════════════════════════════════════════════════════════════════════════
# Path-based routing patterns
# ═══════════════════════════════════════════════════════════════════════════════

# Given a list of changed files, these patterns determine which backend lanes
# to activate. If any file matches ROUTE_SHARED, all lanes run.

ROUTE_SHARED_PATTERNS=(
  "src/lib.rs"
  "src/engine/mod.rs"
  "src/engine/common.rs"
  "src/engine/scalar.rs"
  "src/verify/"
  "Cargo.toml"
  "scripts/verify_safety.sh"
  "scripts/lane_defs.sh"
)

ROUTE_SSSE3_PATTERNS=(
  "src/engine/ssse3"
  "tests/sse_"
  "tests/ssse3_"
  "fuzz/fuzz_targets/ssse3_"
)

ROUTE_AVX2_PATTERNS=(
  "src/engine/avx2"
  "tests/avx2_"
  "fuzz/fuzz_targets/avx2_"
)

ROUTE_AVX512_PATTERNS=(
  "src/engine/avx512"
  "tests/avx512_"
  "fuzz/fuzz_targets/avx512_"
)

ROUTE_NEON_PATTERNS=(
  "src/engine/neon"
  "tests/neon_"
  "fuzz/fuzz_targets/neon_"
)

ROUTE_WASM_PATTERNS=(
  "src/engine/wasm"
  "tests/wasm_"
  "fuzz/fuzz_targets/wasm_"
)

# Returns which backend families are affected by a list of changed files.
# Usage: affected_backends < <(git diff --name-only HEAD~1)
# Output: space-separated list from: all ssse3 avx2 avx512 neon wasm
affected_backends() {
  local -A hit=()
  local all=0
  while IFS= read -r file; do
    # Skip docs/benchmarks unless they touch safety scripts.
    if [[ "$file" == doc/* || "$file" == benches/* || "$file" == README.md ]]; then
      # But safety docs/scripts trigger all.
      if [[ "$file" == doc/safety/* || "$file" == scripts/* ]]; then
        all=1
      fi
      continue
    fi

    for pat in "${ROUTE_SHARED_PATTERNS[@]}"; do
      if [[ "$file" == $pat* ]]; then
        all=1
        break
      fi
    done
    [[ "$all" -eq 1 ]] && break

    for pat in "${ROUTE_SSSE3_PATTERNS[@]}"; do
      [[ "$file" == $pat* ]] && hit[ssse3]=1
    done
    for pat in "${ROUTE_AVX2_PATTERNS[@]}"; do
      [[ "$file" == $pat* ]] && hit[avx2]=1
    done
    for pat in "${ROUTE_AVX512_PATTERNS[@]}"; do
      [[ "$file" == $pat* ]] && hit[avx512]=1
    done
    for pat in "${ROUTE_NEON_PATTERNS[@]}"; do
      [[ "$file" == $pat* ]] && hit[neon]=1
    done
    for pat in "${ROUTE_WASM_PATTERNS[@]}"; do
      [[ "$file" == $pat* ]] && hit[wasm]=1
    done
  done

  if [[ "$all" -eq 1 ]]; then
    echo "all"
  elif [[ ${#hit[@]} -eq 0 ]]; then
    echo "all"  # Conservative default: unknown files trigger full run.
  else
    echo "${!hit[*]}"
  fi
}
