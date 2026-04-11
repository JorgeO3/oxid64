# Kani Guide

This document defines the practical Kani scope for `oxid64`.

## What Kani covers here

- shared bounds helpers in `src/engine/common.rs`
- small length and offset contracts
- guard helpers for decode tails and DS64 shells
- dispatch composition on a tiny modeled path
- pure backend models from `src/engine/models/`
- SSSE3-specific guard/store contracts and CHECK0 schedule models
- AVX2-specific DS128/tail guard-store contracts and CHECK0 schedule models
- AVX512-specific DS256/tail guard-store contracts and encode planner models
- NEON-specific DN256 schedule models and encode planner models
- WASM-specific DS64/tail guard-store contracts, `pshufb` semantics, and encode planner models

## What Kani does not cover here

- full SIMD kernels with real intrinsics
- large hot loops such as `process_ds64_*`, `process_ds128_*`, `process_ds256_*`
- all architectures and all runtime dispatch paths

## Normal run

```bash
just test-kani
```

This runs the current critical proof set with a small unwind bound.

## Current proof set

- `remaining_matches_pointer_delta_within_allocation`
- `can_read_and_can_write_agree_with_remaining`
- `safe_in_end_for_4_within_allocation`
- `safe_in_end_for_16_within_allocation`
- `guard_helpers_imply_required_io_room`
- `prepare_decode_output_matches_decoded_len_contract`
- `decoded_and_encoded_lengths_stay_bounded_in_small_domain`
- `decode_offsets_monotonic_and_dispatch_contracts_hold`
- `ssse3_ds64_store_offsets_fit_guarded_output`
- `ssse3_ds64_double_store_offsets_fit_guarded_output`
- `ssse3_tail16_store_fits_guarded_output`
- `ssse3_non_strict_schedule_model_matches_documented_lanes`
- `ssse3_written_prefix_model_is_bounded_by_decoded_output`
- `avx2_ds128_store_offsets_fit_guarded_output`
- `avx2_ds128_double_store_offsets_fit_guarded_output`
- `avx2_ds128_triple_store_offsets_fit_guarded_output`
- `avx2_tail16_store_fits_guarded_output`
- `avx2_non_strict_schedule_model_matches_documented_lanes`
- `avx2_strict_written_prefix_model_is_bounded_by_decoded_output`
- `avx2_partial_written_prefix_model_is_bounded_by_decoded_output`
- `avx2_unchecked_preflight_matches_documented_contract`
- `avx512_ds256_store_offsets_fit_guarded_output`
- `avx512_ds256_double_store_offsets_fit_guarded_output`
- `avx512_non_strict_schedule_model_matches_documented_lanes`
- `avx512_written_prefix_model_is_bounded_by_decoded_output`
- `avx512_encode_schedule_is_contiguous_without_gaps`
- `avx512_encode_required_input_matches_farthest_load`
- `neon_non_strict_schedule_model_matches_documented_lanes`
- `neon_written_prefix_model_is_bounded_by_decoded_output`
- `neon_encode_prefix_consumes_only_full_blocks`
- `neon_encode_required_input_matches_block_boundaries`
- `wasm_ds64_store_offsets_fit_guarded_output`
- `wasm_ds64_double_store_offsets_fit_guarded_output`
- `wasm_tail16_store_fits_guarded_output`
- `wasm_non_strict_schedule_model_matches_documented_lanes`
- `wasm_written_prefix_model_is_bounded_by_decoded_output`
- `wasm_pshufb_model_matches_low_nibble_spec`
- `wasm_encode_prefix_consumes_only_full_blocks`
- `wasm_encode_required_input_matches_block_boundaries`

## Interpretation of a green run

If the Kani suite is green, it means these small helper contracts were proven in
their modeled domains. It does **not** mean the entire SIMD codec is formally
verified.
