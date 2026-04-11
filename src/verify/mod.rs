#![cfg(kani)]

use crate::engine::common::{
    can_advance, can_process_ds64, can_process_ds64_double, can_process_tail16, can_read,
    can_write, decode_offsets, prepare_decode_output, remaining, remaining_mut, safe_in_end_4,
    safe_in_end_for_width,
};
use crate::engine::dispatch_decode;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::engine::models::avx2::{
    non_strict_checks_offset as avx2_non_strict_checks_offset,
    simd_written_prefix_before_error_partial as avx2_written_prefix_partial,
    simd_written_prefix_before_error_strict as avx2_written_prefix_strict, DS128_STORE_OFFSETS,
    STORE_WIDTH_BYTES as AVX2_STORE_WIDTH_BYTES,
    STRICT_DOUBLE_THRESHOLD as AVX2_STRICT_DOUBLE_THRESHOLD,
    STRICT_SINGLE_THRESHOLD as AVX2_STRICT_SINGLE_THRESHOLD,
    STRICT_TRIPLE_THRESHOLD as AVX2_STRICT_TRIPLE_THRESHOLD, TAIL_THRESHOLD as AVX2_TAIL_THRESHOLD,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::engine::models::avx512vbmi::{
    can_run_double_es256 as avx512_can_run_double_es256,
    can_run_single_es256 as avx512_can_run_single_es256,
    non_strict_checks_offset as avx512_non_strict_checks_offset,
    simd_written_prefix_before_error as avx512_written_prefix,
    DECODE_STORE_WIDTH_BYTES as AVX512_DECODE_STORE_WIDTH_BYTES,
    DOUBLE_ES256_BLOCK_STARTS as AVX512_DOUBLE_ES256_BLOCK_STARTS,
    DOUBLE_ES256_PRELOAD_STARTS as AVX512_DOUBLE_ES256_PRELOAD_STARTS,
    DOUBLE_ES256_REQUIRED_INPUT as AVX512_DOUBLE_ES256_REQUIRED_INPUT,
    DS256_OUTPUT_BYTES as AVX512_DS256_OUTPUT_BYTES,
    DS256_STORE_OFFSETS as AVX512_DS256_STORE_OFFSETS,
    ES256_BLOCK_STARTS as AVX512_ES256_BLOCK_STARTS,
    SINGLE_ES256_REQUIRED_INPUT as AVX512_SINGLE_ES256_REQUIRED_INPUT,
};
use crate::engine::models::neon::{
    can_run_encode_block as neon_can_run_encode_block,
    can_run_encode_pair as neon_can_run_encode_pair,
    encode_prefix_input_len as neon_encode_prefix_input_len,
    encode_prefix_output_len as neon_encode_prefix_output_len,
    non_strict_checks_offset as neon_non_strict_checks_offset,
    simd_written_prefix_before_error as neon_written_prefix,
    DECODE_BLOCK_OUTPUT_BYTES as NEON_DECODE_BLOCK_OUTPUT_BYTES,
    DECODE_GROUP_OUTPUT_BYTES as NEON_DECODE_GROUP_OUTPUT_BYTES,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::engine::models::ssse3::{
    aligned_non_strict_checks_offset, aligned_written_prefix_before_error,
    DS64_DOUBLE_STORE_OFFSETS, DS64_STORE_OFFSETS, STORE_WIDTH_BYTES,
};
use crate::engine::models::wasm_simd128::{
    can_run_encode_drain as wasm_can_run_encode_drain,
    can_run_encode_main as wasm_can_run_encode_main,
    can_run_encode_tail as wasm_can_run_encode_tail,
    encode_prefix_input_len as wasm_encode_prefix_input_len,
    encode_prefix_output_len as wasm_encode_prefix_output_len,
    non_strict_checks_offset as wasm_non_strict_checks_offset,
    pshufb_lookup_byte as wasm_pshufb_lookup_byte, pshufb_select_index as wasm_pshufb_select_index,
    simd_written_prefix_before_error as wasm_written_prefix, wasm_swizzle_select_index,
    DS64_DOUBLE_STORE_OFFSETS as WASM_DS64_DOUBLE_STORE_OFFSETS,
    DS64_OUTPUT_BYTES as WASM_DS64_OUTPUT_BYTES, DS64_STORE_OFFSETS as WASM_DS64_STORE_OFFSETS,
    ENCODE_DRAIN_REQUIRED_INPUT as WASM_ENCODE_DRAIN_REQUIRED_INPUT,
    ENCODE_MAIN_REQUIRED_INPUT as WASM_ENCODE_MAIN_REQUIRED_INPUT,
    ENCODE_TAIL_REQUIRED_INPUT as WASM_ENCODE_TAIL_REQUIRED_INPUT,
    STORE_WIDTH_BYTES as WASM_STORE_WIDTH_BYTES,
};
use crate::engine::scalar::{decoded_len_strict, encoded_len, ScalarDecoder};
use crate::Base64Decoder;

#[kani::proof]
fn remaining_matches_pointer_delta_within_allocation() {
    let buf = [0u8; 16];
    let a: usize = kani::any();
    let b: usize = kani::any();
    kani::assume(a <= 16);
    kani::assume(b <= 16);

    let base = buf.as_ptr();
    let ptr = unsafe { base.add(a) };
    let end = unsafe { base.add(b) };

    assert_eq!(remaining(ptr, end), b.saturating_sub(a));
}

#[kani::proof]
fn can_read_and_can_write_agree_with_remaining() {
    let in_buf = [0u8; 16];
    let mut out_buf = [0u8; 16];
    let a: usize = kani::any();
    let b: usize = kani::any();
    let need: usize = kani::any();
    kani::assume(a <= 16);
    kani::assume(b <= 16);
    kani::assume(need <= 16);

    let in_base = in_buf.as_ptr();
    let in_ptr = unsafe { in_base.add(a) };
    let in_end = unsafe { in_base.add(b) };
    assert_eq!(can_read(in_ptr, in_end, need), b.saturating_sub(a) >= need);

    let out_base = out_buf.as_mut_ptr();
    let out_ptr = unsafe { out_base.add(a) };
    let out_end = unsafe { out_base.add(b) };
    assert_eq!(
        can_write(out_ptr, out_end, need),
        b.saturating_sub(a) >= need
    );
    assert_eq!(remaining_mut(out_ptr, out_end), b.saturating_sub(a));
}

#[kani::proof]
fn safe_in_end_for_4_within_allocation() {
    let buf = [0u8; 16];
    let len: usize = kani::any();
    kani::assume(len <= 16);

    let slice = &buf[..len];
    let ptr = safe_in_end_4(slice);
    let delta = ptr as usize - slice.as_ptr() as usize;

    assert_eq!(delta, len.saturating_sub(4));
}

#[kani::proof]
fn safe_in_end_for_16_within_allocation() {
    let buf = [0u8; 32];
    let len: usize = kani::any();
    kani::assume(len <= 32);

    let slice = &buf[..len];
    let ptr = safe_in_end_for_width(slice, 16);
    let delta = ptr as usize - slice.as_ptr() as usize;

    assert_eq!(delta, len.saturating_sub(16));
}

#[kani::proof]
fn guard_helpers_imply_required_io_room() {
    let in_buf = [0u8; 160];
    let mut out_buf = [0u8; 160];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 160);
    kani::assume(in_b <= 160);
    kani::assume(out_a <= 160);
    kani::assume(out_b <= 160);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_tail16(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_advance(in_ptr, safe_end, 16, out_ptr, out_end, 16));
    }

    if can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_advance(in_ptr, safe_end, 96, out_ptr, out_end, 52));
    }

    if can_process_ds64_double(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_advance(in_ptr, safe_end, 160, out_ptr, out_end, 100));
    }
}

#[kani::proof]
fn prepare_decode_output_matches_decoded_len_contract() {
    let buf: [u8; 8] = kani::any();
    let out = [0u8; 8];
    let len: usize = kani::any();
    let out_len: usize = kani::any();
    kani::assume(len <= 8);
    kani::assume(out_len <= 8);

    let input = &buf[..len];
    let output = &out[..out_len];

    let expected = decoded_len_strict(input);
    let actual = prepare_decode_output(input, output);

    match expected {
        Some(n) if out_len >= n => assert_eq!(actual, Some(n)),
        _ => assert_eq!(actual, None),
    }
}

#[kani::proof]
fn decoded_and_encoded_lengths_stay_bounded_in_small_domain() {
    let buf: [u8; 8] = kani::any();
    let len: usize = kani::any();
    let raw_len: usize = kani::any();
    kani::assume(len <= 8);
    kani::assume(raw_len <= 1024);

    let input = &buf[..len];
    if let Some(decoded) = decoded_len_strict(input) {
        assert!(decoded <= len);
        assert_eq!(len % 4, 0);
    }

    let encoded = encoded_len(raw_len);
    assert_eq!(encoded % 4, 0);
    assert!(encoded >= raw_len);
}

#[kani::proof]
fn decode_offsets_monotonic_and_dispatch_contracts_hold() {
    let in_buf = [0u8; 16];
    let mut out_buf = [0u8; 16];
    let a1: usize = kani::any();
    let a2: usize = kani::any();
    let b1: usize = kani::any();
    let b2: usize = kani::any();
    kani::assume(a1 <= a2);
    kani::assume(a2 <= 16);
    kani::assume(b1 <= b2);
    kani::assume(b2 <= 16);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let first = decode_offsets(
        unsafe { in_base.add(a1) },
        unsafe { out_base.add(b1) },
        in_base,
        out_base,
    );
    let second = decode_offsets(
        unsafe { in_base.add(a2) },
        unsafe { out_base.add(b2) },
        in_base,
        out_base,
    );

    assert!(second.0 >= first.0);
    assert!(second.1 >= first.1);

    unsafe fn prefix_then_scalar(input: &[u8], out: &mut [u8]) -> Option<(usize, usize)> {
        if input.len() < 4 || out.len() < 3 {
            return None;
        }
        out[..3].copy_from_slice(b"Man");
        Some((4, 3))
    }

    let input = *b"TWFuTWFu";
    let mut out = [0u8; 6];
    let written = dispatch_decode(&input, &mut out, prefix_then_scalar)
        .expect("dispatch tail contract must hold");
    assert_eq!(written, 6);
    assert_eq!(&out[..written], b"ManMan");

    let dec = ScalarDecoder;
    let decoded = dec
        .decode(b"TWFu")
        .expect("scalar decoder must decode valid input");
    assert_eq!(decoded, b"Man");
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn ssse3_ds64_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 160];
    let mut out_buf = [0u8; 160];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 160);
    kani::assume(in_b <= 160);
    kani::assume(out_a <= 160);
    kani::assume(out_b <= 160);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_write(
            unsafe { out_ptr.add(DS64_STORE_OFFSETS[0]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_STORE_OFFSETS[1]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_STORE_OFFSETS[2]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_STORE_OFFSETS[3]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn ssse3_ds64_double_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 192];
    let mut out_buf = [0u8; 192];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 192);
    kani::assume(in_b <= 192);
    kani::assume(out_a <= 192);
    kani::assume(out_b <= 192);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_ds64_double(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[0]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[1]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[2]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[3]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[4]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[5]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[6]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS64_DOUBLE_STORE_OFFSETS[7]) },
            out_end,
            STORE_WIDTH_BYTES
        ));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn ssse3_tail16_store_fits_guarded_output() {
    let in_buf = [0u8; 64];
    let mut out_buf = [0u8; 64];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 64);
    kani::assume(in_b <= 64);
    kani::assume(out_a <= 64);
    kani::assume(out_b <= 64);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_tail16(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_write(out_ptr, out_end, STORE_WIDTH_BYTES));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn ssse3_non_strict_schedule_model_matches_documented_lanes() {
    let pos: usize = kani::any();
    kani::assume(pos < 128);

    let checked = aligned_non_strict_checks_offset(128, pos);
    if pos < 64 {
        assert_eq!(checked, pos < 16);
    } else if pos < 112 {
        assert!(checked);
    } else {
        assert!(checked);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn ssse3_written_prefix_model_is_bounded_by_decoded_output() {
    let len: usize = kani::any();
    kani::assume(len <= 256);
    kani::assume(len % 4 == 0);

    let prefix = aligned_written_prefix_before_error(len);
    let decoded_upper = (len / 4) * 3;
    assert!(prefix <= decoded_upper);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_ds128_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 256];
    let mut out_buf = [0u8; 256];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 256);
    kani::assume(in_b <= 256);
    kani::assume(out_a <= 256);
    kani::assume(out_b <= 256);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let in_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_advance(
        in_ptr,
        in_end,
        AVX2_STRICT_SINGLE_THRESHOLD + 1,
        out_ptr,
        out_end,
        100,
    ) {
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[0]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[1]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[2]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[3]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[4]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[5]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[6]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(DS128_STORE_OFFSETS[7]) },
            out_end,
            AVX2_STORE_WIDTH_BYTES
        ));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_ds128_double_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 512];
    let mut out_buf = [0u8; 512];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 512);
    kani::assume(in_b <= 512);
    kani::assume(out_a <= 512);
    kani::assume(out_b <= 512);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let in_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_advance(
        in_ptr,
        in_end,
        AVX2_STRICT_DOUBLE_THRESHOLD + 1,
        out_ptr,
        out_end,
        196,
    ) {
        for base in [0usize, 96] {
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[0]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[1]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[2]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[3]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[4]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[5]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[6]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[7]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_ds128_triple_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 768];
    let mut out_buf = [0u8; 768];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 768);
    kani::assume(in_b <= 768);
    kani::assume(out_a <= 768);
    kani::assume(out_b <= 768);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let in_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_advance(
        in_ptr,
        in_end,
        AVX2_STRICT_TRIPLE_THRESHOLD + 1,
        out_ptr,
        out_end,
        292,
    ) {
        for base in [0usize, 96, 192] {
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[0]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[1]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[2]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[3]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[4]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[5]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[6]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + DS128_STORE_OFFSETS[7]) },
                out_end,
                AVX2_STORE_WIDTH_BYTES
            ));
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_tail16_store_fits_guarded_output() {
    let in_buf = [0u8; 64];
    let mut out_buf = [0u8; 64];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 64);
    kani::assume(in_b <= 64);
    kani::assume(out_a <= 64);
    kani::assume(out_b <= 64);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let in_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_advance(
        in_ptr,
        in_end,
        AVX2_TAIL_THRESHOLD + 1,
        out_ptr,
        out_end,
        16,
    ) {
        assert!(can_write(out_ptr, out_end, AVX2_STORE_WIDTH_BYTES));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_non_strict_schedule_model_matches_documented_lanes() {
    let len_choice: u8 = kani::any();
    let len = match len_choice % 3 {
        0 => 200usize,
        1 => 328usize,
        _ => 456usize,
    };
    let pos: usize = kani::any();
    kani::assume(pos < len);

    let checked = avx2_non_strict_checks_offset(len, pos);
    let unchecked = match len {
        200 => (32..64).contains(&pos),
        328 => (32..64).contains(&pos) || (160..192).contains(&pos),
        456 => (32..64).contains(&pos) || (160..192).contains(&pos) || (288..320).contains(&pos),
        _ => false,
    };
    assert_eq!(checked, !unchecked);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_strict_written_prefix_model_is_bounded_by_decoded_output() {
    let len_choice: u8 = kani::any();
    let len = match len_choice % 3 {
        0 => 200usize,
        1 => 328usize,
        _ => 456usize,
    };
    let prefix = avx2_written_prefix_strict(len);
    assert!(prefix <= (len / 4) * 3);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_partial_written_prefix_model_is_bounded_by_decoded_output() {
    let len_choice: u8 = kani::any();
    let len = match len_choice % 3 {
        0 => 200usize,
        1 => 328usize,
        _ => 456usize,
    };
    let prefix = avx2_written_prefix_partial(len);
    assert!(prefix <= (len / 4) * 3);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx2_unchecked_preflight_matches_documented_contract() {
    let buf: [u8; 12] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= 12);
    let input = &buf[..len];
    let got = crate::engine::avx2::decoded_len_unchecked(input);
    if len == 0 {
        assert_eq!(got, Some(0));
    } else if len % 4 != 0 {
        assert_eq!(got, None);
    } else if let Some(n) = got {
        assert!(n <= (len / 4) * 3);
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx512_ds256_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 768];
    let mut out_buf = [0u8; 768];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 768);
    kani::assume(in_b <= 768);
    kani::assume(out_a <= 768);
    kani::assume(out_b <= 768);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let in_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_advance(
        in_ptr,
        in_end,
        389,
        out_ptr,
        out_end,
        AVX512_DS256_OUTPUT_BYTES + 64,
    ) {
        assert!(can_write(
            unsafe { out_ptr.add(AVX512_DS256_STORE_OFFSETS[0]) },
            out_end,
            AVX512_DECODE_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(AVX512_DS256_STORE_OFFSETS[1]) },
            out_end,
            AVX512_DECODE_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(AVX512_DS256_STORE_OFFSETS[2]) },
            out_end,
            AVX512_DECODE_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(AVX512_DS256_STORE_OFFSETS[3]) },
            out_end,
            AVX512_DECODE_STORE_WIDTH_BYTES
        ));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx512_ds256_double_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 1024];
    let mut out_buf = [0u8; 1024];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 1024);
    kani::assume(in_b <= 1024);
    kani::assume(out_a <= 1024);
    kani::assume(out_b <= 1024);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let in_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_advance(in_ptr, in_end, 645, out_ptr, out_end, 400) {
        for base in [0usize, AVX512_DS256_OUTPUT_BYTES] {
            assert!(can_write(
                unsafe { out_ptr.add(base + AVX512_DS256_STORE_OFFSETS[0]) },
                out_end,
                AVX512_DECODE_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + AVX512_DS256_STORE_OFFSETS[1]) },
                out_end,
                AVX512_DECODE_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + AVX512_DS256_STORE_OFFSETS[2]) },
                out_end,
                AVX512_DECODE_STORE_WIDTH_BYTES
            ));
            assert!(can_write(
                unsafe { out_ptr.add(base + AVX512_DS256_STORE_OFFSETS[3]) },
                out_end,
                AVX512_DECODE_STORE_WIDTH_BYTES
            ));
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx512_non_strict_schedule_model_matches_documented_lanes() {
    let len_choice: u8 = kani::any();
    let len = if len_choice & 1 == 0 {
        392usize
    } else {
        648usize
    };
    let pos: usize = kani::any();
    kani::assume(pos < len);

    let checked = avx512_non_strict_checks_offset(len, pos);
    let unchecked = match len {
        392 => (64..128).contains(&pos),
        648 => (64..128).contains(&pos) || (320..384).contains(&pos),
        _ => false,
    };
    assert_eq!(checked, !unchecked);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx512_written_prefix_model_is_bounded_by_decoded_output() {
    let len_choice: u8 = kani::any();
    let len = if len_choice & 1 == 0 {
        392usize
    } else {
        648usize
    };
    let prefix = avx512_written_prefix(len);
    assert!(prefix <= (len / 4) * 3);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx512_encode_schedule_is_contiguous_without_gaps() {
    assert_eq!(AVX512_ES256_BLOCK_STARTS[0], 0);
    assert_eq!(AVX512_ES256_BLOCK_STARTS[1], 48);
    assert_eq!(AVX512_ES256_BLOCK_STARTS[2], 96);
    assert_eq!(AVX512_ES256_BLOCK_STARTS[3], 144);

    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[0], 0);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[1], 48);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[2], 96);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[3], 144);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[4], 192);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[5], 240);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[6], 288);
    assert_eq!(AVX512_DOUBLE_ES256_BLOCK_STARTS[7], 336);

    assert_eq!(AVX512_DOUBLE_ES256_PRELOAD_STARTS[0], 384);
    assert_eq!(AVX512_DOUBLE_ES256_PRELOAD_STARTS[1], 432);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[kani::proof]
fn avx512_encode_required_input_matches_farthest_load() {
    let rem: usize = kani::any();
    kani::assume(rem <= 1024);
    if avx512_can_run_single_es256(rem) {
        assert!(rem >= AVX512_SINGLE_ES256_REQUIRED_INPUT);
    }
    if avx512_can_run_double_es256(rem) {
        assert!(rem >= AVX512_DOUBLE_ES256_REQUIRED_INPUT);
    }
}

#[kani::proof]
fn neon_non_strict_schedule_model_matches_documented_lanes() {
    let len_choice: u8 = kani::any();
    let len = if len_choice & 1 == 0 {
        320usize
    } else {
        576usize
    };
    let pos: usize = kani::any();
    kani::assume(pos < len);

    let checked = neon_non_strict_checks_offset(len, pos);
    let unchecked = match len {
        320 => (0..192).contains(&pos),
        576 => (0..192).contains(&pos) || (256..448).contains(&pos),
        _ => false,
    };
    assert_eq!(checked, !unchecked);
}

#[kani::proof]
fn neon_written_prefix_model_is_bounded_by_decoded_output() {
    let len_choice: u8 = kani::any();
    let len = match len_choice % 3 {
        0 => 64usize,
        1 => 320usize,
        _ => 576usize,
    };

    let prefix = neon_written_prefix(len);
    assert!(prefix <= (len / 4) * 3);
    assert_eq!(prefix % NEON_DECODE_BLOCK_OUTPUT_BYTES, 0);

    if len == 320 {
        assert_eq!(prefix, NEON_DECODE_GROUP_OUTPUT_BYTES);
    }
}

#[kani::proof]
fn neon_encode_prefix_consumes_only_full_blocks() {
    let len: usize = kani::any();
    kani::assume(len <= 1024);

    let prefix_in = neon_encode_prefix_input_len(len);
    let prefix_out = neon_encode_prefix_output_len(len);

    assert!(prefix_in <= len);
    assert_eq!(prefix_in % 48, 0);
    assert_eq!(prefix_out, (prefix_in / 3) * 4);
}

#[kani::proof]
fn neon_encode_required_input_matches_block_boundaries() {
    let rem: usize = kani::any();
    kani::assume(rem <= 1024);

    if neon_can_run_encode_block(rem) {
        assert!(rem >= 48);
    }
    if neon_can_run_encode_pair(rem) {
        assert!(rem >= 96);
    }
}

#[kani::proof]
fn wasm_ds64_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 160];
    let mut out_buf = [0u8; 160];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 160);
    kani::assume(in_b <= 160);
    kani::assume(out_a <= 160);
    kani::assume(out_b <= 160);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_STORE_OFFSETS[0]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_STORE_OFFSETS[1]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_STORE_OFFSETS[2]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_STORE_OFFSETS[3]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
    }
}

#[kani::proof]
fn wasm_ds64_double_store_offsets_fit_guarded_output() {
    let in_buf = [0u8; 192];
    let mut out_buf = [0u8; 192];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 192);
    kani::assume(in_b <= 192);
    kani::assume(out_a <= 192);
    kani::assume(out_b <= 192);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_ds64_double(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[0]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[1]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[2]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[3]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[4]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[5]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[6]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
        assert!(can_write(
            unsafe { out_ptr.add(WASM_DS64_DOUBLE_STORE_OFFSETS[7]) },
            out_end,
            WASM_STORE_WIDTH_BYTES
        ));
    }
}

#[kani::proof]
fn wasm_tail16_store_fits_guarded_output() {
    let in_buf = [0u8; 64];
    let mut out_buf = [0u8; 64];
    let in_a: usize = kani::any();
    let in_b: usize = kani::any();
    let out_a: usize = kani::any();
    let out_b: usize = kani::any();
    kani::assume(in_a <= 64);
    kani::assume(in_b <= 64);
    kani::assume(out_a <= 64);
    kani::assume(out_b <= 64);

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(in_a) };
    let safe_end = unsafe { in_base.add(in_b) };
    let out_ptr = unsafe { out_base.add(out_a) };
    let out_end = unsafe { out_base.add(out_b) };

    if can_process_tail16(in_ptr, safe_end, out_ptr, out_end) {
        assert!(can_write(out_ptr, out_end, WASM_STORE_WIDTH_BYTES));
    }
}

#[kani::proof]
fn wasm_non_strict_schedule_model_matches_documented_lanes() {
    let len_choice: u8 = kani::any();
    let len = if len_choice & 1 == 0 {
        128usize
    } else {
        192usize
    };
    let pos: usize = kani::any();
    kani::assume(pos < len);

    let checked = wasm_non_strict_checks_offset(len, pos);
    let unchecked = match len {
        128 => (16..64).contains(&pos),
        192 => (16..64).contains(&pos) || (80..128).contains(&pos),
        _ => false,
    };
    assert_eq!(checked, !unchecked);
}

#[kani::proof]
fn wasm_written_prefix_model_is_bounded_by_decoded_output() {
    let len_choice: u8 = kani::any();
    let len = match len_choice % 3 {
        0 => 20usize,
        1 => 100usize,
        _ => 168usize,
    };

    let prefix = wasm_written_prefix(len);
    assert!(prefix <= (len / 4) * 3);
    assert_eq!(prefix % 12, 0);

    if len == 100 {
        assert!(prefix >= WASM_DS64_OUTPUT_BYTES);
    }
}

#[kani::proof]
fn wasm_pshufb_model_matches_low_nibble_spec() {
    let ctrl: u8 = kani::any();
    let table = *b"0123456789abcdef";
    let got = wasm_pshufb_lookup_byte(table, ctrl);

    if ctrl & 0x80 != 0 {
        assert_eq!(got, 0);
    } else {
        assert_eq!(got, table[(ctrl & 0x0f) as usize]);
    }

    if ctrl < 16 {
        assert_eq!(
            wasm_pshufb_select_index(ctrl),
            wasm_swizzle_select_index(ctrl)
        );
    }
}

#[kani::proof]
fn wasm_encode_prefix_consumes_only_full_blocks() {
    let len: usize = kani::any();
    kani::assume(len <= 1024);

    let prefix_in = wasm_encode_prefix_input_len(len);
    let prefix_out = wasm_encode_prefix_output_len(len);

    assert!(prefix_in <= len);
    assert_eq!(prefix_in % 12, 0);
    assert_eq!(prefix_out, (prefix_in / 3) * 4);
}

#[kani::proof]
fn wasm_encode_required_input_matches_block_boundaries() {
    let rem: usize = kani::any();
    kani::assume(rem <= 1024);

    if wasm_can_run_encode_main(rem) {
        assert!(rem >= WASM_ENCODE_MAIN_REQUIRED_INPUT);
    }
    if wasm_can_run_encode_drain(rem) {
        assert!(rem >= WASM_ENCODE_DRAIN_REQUIRED_INPUT);
    }
    if wasm_can_run_encode_tail(rem) {
        assert!(rem >= WASM_ENCODE_TAIL_REQUIRED_INPUT);
    }
}
