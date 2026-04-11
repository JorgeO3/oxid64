use oxid64::engine::models::wasm_simd128::{
    DOUBLE_THRESHOLD, DS64_INPUT_BYTES, DS64_OUTPUT_BYTES, ENCODE_DRAIN_REQUIRED_INPUT,
    ENCODE_MAIN_REQUIRED_INPUT, ENCODE_SIMD_ENTRY_THRESHOLD, ENCODE_TAIL_REQUIRED_INPUT,
    STORE_OVERHANG_BYTES, STORE_WIDTH_BYTES, TAIL_THRESHOLD, can_run_encode_drain,
    can_run_encode_main, can_run_encode_tail, encode_prefix_input_len, encode_prefix_output_len,
    non_strict_checks_offset, pshufb_lookup_byte, pshufb_select_index,
    simd_touched_prefix_before_error, simd_written_prefix_before_error, wasm_swizzle_select_index,
};

#[test]
fn wasm_pshufb_model_uses_low_nibble_for_ascii_controls() {
    let table = *b"0123456789abcdef";

    assert_eq!(pshufb_select_index(b'A'), Some(1));
    assert_eq!(pshufb_lookup_byte(table, b'A'), b'1');
    assert_eq!(pshufb_select_index(b'B'), Some(2));
    assert_eq!(pshufb_lookup_byte(table, b'B'), b'2');
    assert_eq!(pshufb_select_index(b'/'), Some(15));
    assert_eq!(pshufb_lookup_byte(table, b'/'), b'f');
    assert_eq!(pshufb_select_index(0xFF), None);
    assert_eq!(pshufb_lookup_byte(table, 0xFF), 0);
}

#[test]
fn wasm_pshufb_model_differs_from_raw_wasm_swizzle_for_base64_ascii() {
    assert_eq!(pshufb_select_index(b'A'), Some(1));
    assert_eq!(wasm_swizzle_select_index(b'A'), None);
    assert_eq!(pshufb_select_index(b'B'), Some(2));
    assert_eq!(wasm_swizzle_select_index(b'B'), None);
    assert_eq!(pshufb_select_index(b'/'), Some(15));
    assert_eq!(wasm_swizzle_select_index(b'/'), None);
}

#[test]
fn wasm_decode_model_schedule_and_prefix_bounds() {
    assert!(non_strict_checks_offset(20, 0));

    assert!(non_strict_checks_offset(128, 0));
    assert!(!non_strict_checks_offset(128, 16));
    assert!(!non_strict_checks_offset(128, 32));
    assert!(!non_strict_checks_offset(128, 48));
    assert!(non_strict_checks_offset(128, 64));
    assert!(non_strict_checks_offset(128, 112));

    assert_eq!(simd_written_prefix_before_error(19), 0);
    assert_eq!(simd_written_prefix_before_error(20), 0);
    assert_eq!(simd_written_prefix_before_error(100), 72);
    assert_eq!(simd_written_prefix_before_error(168), 120);
    assert_eq!(simd_touched_prefix_before_error(19), 0);
    assert_eq!(simd_touched_prefix_before_error(20), 0);
    assert_eq!(simd_touched_prefix_before_error(100), 76);
    assert_eq!(simd_touched_prefix_before_error(168), 124);
}

#[test]
fn wasm_decode_model_span_thresholds_match_real_loads_and_stores() {
    assert_eq!(TAIL_THRESHOLD, 16);
    assert_eq!(DOUBLE_THRESHOLD, 32 + 2 * DS64_INPUT_BYTES);
    assert_eq!(STORE_WIDTH_BYTES, 16);
    assert_eq!(DS64_OUTPUT_BYTES + STORE_OVERHANG_BYTES, 52);
}

#[test]
fn wasm_encode_model_required_input_matches_block_boundaries() {
    assert_eq!(ENCODE_SIMD_ENTRY_THRESHOLD, 52);
    assert!(!can_run_encode_main(ENCODE_MAIN_REQUIRED_INPUT - 1));
    assert!(can_run_encode_main(ENCODE_MAIN_REQUIRED_INPUT));
    assert!(!can_run_encode_drain(ENCODE_DRAIN_REQUIRED_INPUT - 1));
    assert!(can_run_encode_drain(ENCODE_DRAIN_REQUIRED_INPUT));
    assert!(!can_run_encode_tail(ENCODE_TAIL_REQUIRED_INPUT - 1));
    assert!(can_run_encode_tail(ENCODE_TAIL_REQUIRED_INPUT));

    assert_eq!(encode_prefix_input_len(51), 0);
    assert_eq!(encode_prefix_input_len(52), 48);
    assert_eq!(encode_prefix_input_len(59), 48);
    assert_eq!(encode_prefix_input_len(60), 48);
    assert_eq!(encode_prefix_input_len(95), 84);
    assert_eq!(encode_prefix_input_len(96), 84);
    assert_eq!(encode_prefix_input_len(107), 96);
    assert_eq!(encode_prefix_input_len(108), 96);

    assert_eq!(encode_prefix_output_len(51), 0);
    assert_eq!(encode_prefix_output_len(52), 64);
    assert_eq!(encode_prefix_output_len(60), 64);
    assert_eq!(encode_prefix_output_len(108), 128);
    assert_eq!(encode_prefix_output_len(120), 144);
}
