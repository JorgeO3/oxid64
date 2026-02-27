#![allow(unsafe_op_in_unsafe_fn)]

use super::scalar::decode_base64_fast;

pub struct Ssse3CStyleDecoder;
pub struct Ssse3CStyleStrictDecoder;
pub struct Ssse3CStyleStrictRangeDecoder;
pub struct Ssse3CStyleStrictArithDecoder;

impl Ssse3CStyleDecoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], out: &mut [u8]) -> usize {
        super::ssse3::Ssse3Decoder::encode_to_slice(input, out)
    }

    #[inline]
    pub fn decode_to_slice(input: &[u8], out: &mut [u8]) -> Option<usize> {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                if let Some((c, w)) =
                    unsafe { ssse3_cstyle_engine::decode_base64_ssse3_cstyle(input, out) }
                {
                    consumed = c;
                    written = w;
                } else {
                    return None;
                }
            }
        }

        if consumed < input.len() {
            let tail_written = decode_base64_fast(&input[consumed..], &mut out[written..])?;
            written += tail_written;
        }
        Some(written)
    }
}

impl Ssse3CStyleStrictDecoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], out: &mut [u8]) -> usize {
        super::ssse3::Ssse3Decoder::encode_to_slice(input, out)
    }

    #[inline]
    pub fn decode_to_slice(input: &[u8], out: &mut [u8]) -> Option<usize> {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                if let Some((c, w)) =
                    unsafe { ssse3_cstyle_engine::decode_base64_ssse3_cstyle_strict(input, out) }
                {
                    consumed = c;
                    written = w;
                } else {
                    return None;
                }
            }
        }

        if consumed < input.len() {
            let tail_written = decode_base64_fast(&input[consumed..], &mut out[written..])?;
            written += tail_written;
        }
        Some(written)
    }
}

impl Ssse3CStyleStrictRangeDecoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], out: &mut [u8]) -> usize {
        super::ssse3::Ssse3Decoder::encode_to_slice(input, out)
    }

    #[inline]
    pub fn decode_to_slice(input: &[u8], out: &mut [u8]) -> Option<usize> {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                if let Some((c, w)) = unsafe {
                    ssse3_cstyle_engine::decode_base64_ssse3_cstyle_strict_range(input, out)
                } {
                    consumed = c;
                    written = w;
                } else {
                    return None;
                }
            }
        }

        if consumed < input.len() {
            let tail_written = decode_base64_fast(&input[consumed..], &mut out[written..])?;
            written += tail_written;
        }
        Some(written)
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Ssse3CStyleStrictArithDecoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], out: &mut [u8]) -> usize {
        super::ssse3::Ssse3Decoder::encode_to_slice(input, out)
    }

    #[inline]
    pub fn decode_to_slice(input: &[u8], out: &mut [u8]) -> Option<usize> {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                if let Some((c, w)) = unsafe {
                    ssse3_cstyle_engine::decode_base64_ssse3_cstyle_strict_arith(input, out)
                } {
                    consumed = c;
                    written = w;
                } else {
                    return None;
                }
            }
        }

        if consumed < input.len() {
            let tail_written = decode_base64_fast(&input[consumed..], &mut out[written..])?;
            written += tail_written;
        }
        Some(written)
    }
}

mod ssse3_cstyle_engine {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn map_and_pack(
        iv: __m128i,
        delta_asso: __m128i,
        delta_values: __m128i,
        cpv: __m128i,
        madd_mul_1: __m128i,
        madd_mul_2: __m128i,
    ) -> (__m128i, __m128i) {
        let shifted = _mm_srli_epi32(iv, 3);
        let delta_hash = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv), shifted);
        let mut ov = _mm_add_epi8(_mm_shuffle_epi8(delta_values, delta_hash), iv);
        let merge_ab_bc = _mm_maddubs_epi16(ov, madd_mul_1);
        ov = _mm_madd_epi16(merge_ab_bc, madd_mul_2);
        ov = _mm_shuffle_epi8(ov, cpv);
        (ov, shifted)
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn map_and_pack_with_shifted(
        iv: __m128i,
        shifted: __m128i,
        delta_asso: __m128i,
        delta_values: __m128i,
        cpv: __m128i,
        madd_mul_1: __m128i,
        madd_mul_2: __m128i,
    ) -> __m128i {
        let delta_hash = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv), shifted);
        let mut ov = _mm_add_epi8(_mm_shuffle_epi8(delta_values, delta_hash), iv);
        let merge_ab_bc = _mm_maddubs_epi16(ov, madd_mul_1);
        ov = _mm_madd_epi16(merge_ab_bc, madd_mul_2);
        _mm_shuffle_epi8(ov, cpv)
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_vec(
        iv: __m128i,
        shifted: __m128i,
        check_asso: __m128i,
        check_values: __m128i,
    ) -> __m128i {
        let check_hash = _mm_avg_epu8(_mm_shuffle_epi8(check_asso, iv), shifted);
        _mm_adds_epi8(_mm_shuffle_epi8(check_values, check_hash), iv)
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn accumulate_check(
        iv: __m128i,
        shifted: __m128i,
        check_asso: __m128i,
        check_values: __m128i,
        error_mask: __m128i,
    ) -> __m128i {
        _mm_or_si128(error_mask, check_vec(iv, shifted, check_asso, check_values))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_mask_bits(
        iv: __m128i,
        shifted: __m128i,
        check_asso: __m128i,
        check_values: __m128i,
    ) -> i32 {
        _mm_movemask_epi8(check_vec(iv, shifted, check_asso, check_values))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_mask_bits_pair(
        iv0: __m128i,
        shifted0: __m128i,
        iv1: __m128i,
        shifted1: __m128i,
        check_asso: __m128i,
        check_values: __m128i,
    ) -> i32 {
        let chk0 = check_vec(iv0, shifted0, check_asso, check_values);
        let chk1 = check_vec(iv1, shifted1, check_asso, check_values);
        _mm_movemask_epi8(_mm_or_si128(chk0, chk1))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_ds64_partial(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut __m128i,
        iu1: &mut __m128i,
        delta_asso: __m128i,
        delta_values: __m128i,
        check_asso: __m128i,
        check_values: __m128i,
        cpv: __m128i,
        madd_mul_1: __m128i,
        madd_mul_2: __m128i,
        error_mask: &mut __m128i,
    ) {
        let iv0 = _mm_loadu_si128(in_ptr.add(32) as *const __m128i);
        let iv1 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);

        let (ou0, shifted0) =
            map_and_pack(*iu0, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
        let (ou1, shifted1) =
            map_and_pack(*iu1, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);

        _mm_storeu_si128(out_ptr as *mut __m128i, ou0);
        _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, ou1);

        // C-equivalent default behavior (CHECK0/CHECK1): only the first lane is validated.
        *error_mask = accumulate_check(*iu0, shifted0, check_asso, check_values, *error_mask);
        let _ = shifted1;

        *iu0 = _mm_loadu_si128(in_ptr.add(64) as *const __m128i);
        *iu1 = _mm_loadu_si128(in_ptr.add(80) as *const __m128i);

        let (ov2, shifted2) =
            map_and_pack(iv0, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
        let (ov3, shifted3) =
            map_and_pack(iv1, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);

        _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, ov2);
        _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, ov3);

        let _ = shifted2;
        let _ = shifted3;
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_ds64_strict(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut __m128i,
        iu1: &mut __m128i,
        delta_asso: __m128i,
        delta_values: __m128i,
        check_asso: __m128i,
        check_values: __m128i,
        cpv: __m128i,
        madd_mul_1: __m128i,
        madd_mul_2: __m128i,
        error_bits: &mut i32,
    ) {
        let shifted0 = _mm_srli_epi32(*iu0, 3);
        let shifted1 = _mm_srli_epi32(*iu1, 3);
        let ou0 = map_and_pack_with_shifted(
            *iu0,
            shifted0,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );
        let ou1 = map_and_pack_with_shifted(
            *iu1,
            shifted1,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );

        _mm_storeu_si128(out_ptr as *mut __m128i, ou0);
        _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, ou1);

        let m01 = check_mask_bits_pair(*iu0, shifted0, *iu1, shifted1, check_asso, check_values);

        let iv0 = _mm_loadu_si128(in_ptr.add(32) as *const __m128i);
        let iv1 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);
        let shifted2 = _mm_srli_epi32(iv0, 3);
        let shifted3 = _mm_srli_epi32(iv1, 3);
        let ov2 = map_and_pack_with_shifted(
            iv0,
            shifted2,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );
        let ov3 = map_and_pack_with_shifted(
            iv1,
            shifted3,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );

        _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, ov2);
        _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, ov3);

        let m23 = check_mask_bits_pair(iv0, shifted2, iv1, shifted3, check_asso, check_values);

        *iu0 = _mm_loadu_si128(in_ptr.add(64) as *const __m128i);
        *iu1 = _mm_loadu_si128(in_ptr.add(80) as *const __m128i);

        *error_bits |= m01 | m23;
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_mask_bits_range_folded(
        iv: __m128i,
        letter_lo: __m128i,
        letter_hi: __m128i,
        digit_lo: __m128i,
        digit_hi: __m128i,
        plus_ch: __m128i,
        slash_ch: __m128i,
        to_lower: __m128i,
    ) -> i32 {
        // Fold letters to lowercase (A-Z -> a-z) to reduce comparisons.
        let folded = _mm_or_si128(iv, to_lower);

        let ge_letter = _mm_cmpgt_epi8(folded, letter_lo);
        let gt_letter_hi = _mm_cmpgt_epi8(folded, letter_hi);
        let letter_ok = _mm_andnot_si128(gt_letter_hi, ge_letter);

        let ge_digit = _mm_cmpgt_epi8(iv, digit_lo);
        let gt_digit_hi = _mm_cmpgt_epi8(iv, digit_hi);
        let digit_ok = _mm_andnot_si128(gt_digit_hi, ge_digit);

        let plus_ok = _mm_cmpeq_epi8(iv, plus_ch);
        let slash_ok = _mm_cmpeq_epi8(iv, slash_ch);

        let mut valid = _mm_or_si128(letter_ok, digit_ok);
        valid = _mm_or_si128(valid, plus_ok);
        valid = _mm_or_si128(valid, slash_ok);

        (!_mm_movemask_epi8(valid)) & 0xFFFF
    }

    /// Arithmetic validity check: returns a vector with 0x00 for valid bytes and
    /// nonzero for invalid bytes. Uses psubusb (no pshufb!) so it runs on
    /// FP0-FP3 without competing with the pshufb-heavy mapping pipeline.
    ///
    /// Valid base64: A-Z(65-90), a-z(97-122), 0-9(48-57), +(43), /(47)
    /// Ranges checked (positive logic with case-fold):
    ///   letter_ok : (iv | 0x20) in [97, 122]  → covers A-Z and a-z
    ///   digit_slash_ok : iv in [47, 57]        → covers / and 0-9
    ///   plus_ok : iv == 43                     → covers +
    ///   valid = letter_ok | digit_slash_ok | plus_ok
    ///   invalid = ~valid  (any byte not 0xFF is invalid)
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn arith_check_vec(
        iv: __m128i,
        to_lower: __m128i,
        lo_letter: __m128i,
        hi_letter: __m128i,
        lo_digit_slash: __m128i,
        hi_digit_slash: __m128i,
        plus_val: __m128i,
        zero: __m128i,
    ) -> __m128i {
        // Letters: fold A-Z to a-z, then range-check [97, 122]
        let folded = _mm_or_si128(iv, to_lower);
        let letter_under = _mm_subs_epu8(lo_letter, folded); // nonzero if folded < 97
        let letter_over = _mm_subs_epu8(folded, hi_letter); // nonzero if folded > 122
        let letter_out = _mm_or_si128(letter_under, letter_over);
        let letter_ok = _mm_cmpeq_epi8(letter_out, zero); // 0xFF if in [97,122]

        // Digits + slash: range-check [47, 57]
        let ds_under = _mm_subs_epu8(lo_digit_slash, iv); // nonzero if iv < 47
        let ds_over = _mm_subs_epu8(iv, hi_digit_slash); // nonzero if iv > 57
        let ds_out = _mm_or_si128(ds_under, ds_over);
        let ds_ok = _mm_cmpeq_epi8(ds_out, zero); // 0xFF if in [47,57]

        // Plus: exact match 43
        let plus_ok = _mm_cmpeq_epi8(iv, plus_val); // 0xFF if == 43

        // Combine: valid = letter_ok | ds_ok | plus_ok
        let valid = _mm_or_si128(_mm_or_si128(letter_ok, ds_ok), plus_ok);

        // Return inverted: nonzero lanes = invalid bytes
        // ~valid: 0x00 where valid, 0xFF where invalid
        // We use andnot(valid, all_ones) but cheaper: cmpeq(valid, zero)
        // If valid == 0xFF → cmpeq gives 0x00 (valid). If valid == 0x00 → cmpeq gives 0xFF (invalid).
        _mm_cmpeq_epi8(valid, zero)
    }

    /// Accumulate arith check for a pair of vectors into an error accumulator.
    /// Returns the OR of existing errors and new check results.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn arith_check_accumulate_pair(
        iv0: __m128i,
        iv1: __m128i,
        to_lower: __m128i,
        lo_letter: __m128i,
        hi_letter: __m128i,
        lo_digit_slash: __m128i,
        hi_digit_slash: __m128i,
        plus_val: __m128i,
        zero: __m128i,
        error_acc: __m128i,
    ) -> __m128i {
        let chk0 = arith_check_vec(
            iv0,
            to_lower,
            lo_letter,
            hi_letter,
            lo_digit_slash,
            hi_digit_slash,
            plus_val,
            zero,
        );
        let chk1 = arith_check_vec(
            iv1,
            to_lower,
            lo_letter,
            hi_letter,
            lo_digit_slash,
            hi_digit_slash,
            plus_val,
            zero,
        );
        _mm_or_si128(error_acc, _mm_or_si128(chk0, chk1))
    }

    /// Process a DS64 block (64 input bytes → 48 output bytes) with arithmetic
    /// strict checking. Uses the same map+pack path as the table-based strict
    /// variant but replaces the pshufb-based check with psubusb arithmetic.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_ds64_strict_arith(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut __m128i,
        iu1: &mut __m128i,
        delta_asso: __m128i,
        delta_values: __m128i,
        cpv: __m128i,
        madd_mul_1: __m128i,
        madd_mul_2: __m128i,
        to_lower: __m128i,
        lo_letter: __m128i,
        hi_letter: __m128i,
        lo_digit_slash: __m128i,
        hi_digit_slash: __m128i,
        plus_val: __m128i,
        zero: __m128i,
        error_acc: &mut __m128i,
    ) {
        // Map+pack first pair (iu0, iu1) — uses pshufb-heavy path
        let shifted0 = _mm_srli_epi32(*iu0, 3);
        let shifted1 = _mm_srli_epi32(*iu1, 3);
        let ou0 = map_and_pack_with_shifted(
            *iu0,
            shifted0,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );
        let ou1 = map_and_pack_with_shifted(
            *iu1,
            shifted1,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );
        _mm_storeu_si128(out_ptr as *mut __m128i, ou0);
        _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, ou1);

        // Arith check first pair — runs on FP0/FP3, overlaps with pshufb on FP1/FP2
        *error_acc = arith_check_accumulate_pair(
            *iu0,
            *iu1,
            to_lower,
            lo_letter,
            hi_letter,
            lo_digit_slash,
            hi_digit_slash,
            plus_val,
            zero,
            *error_acc,
        );

        // Load next pair
        let iv0 = _mm_loadu_si128(in_ptr.add(32) as *const __m128i);
        let iv1 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);

        // Map+pack second pair
        let shifted2 = _mm_srli_epi32(iv0, 3);
        let shifted3 = _mm_srli_epi32(iv1, 3);
        let ov2 = map_and_pack_with_shifted(
            iv0,
            shifted2,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );
        let ov3 = map_and_pack_with_shifted(
            iv1,
            shifted3,
            delta_asso,
            delta_values,
            cpv,
            madd_mul_1,
            madd_mul_2,
        );
        _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, ov2);
        _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, ov3);

        // Arith check second pair
        *error_acc = arith_check_accumulate_pair(
            iv0,
            iv1,
            to_lower,
            lo_letter,
            hi_letter,
            lo_digit_slash,
            hi_digit_slash,
            plus_val,
            zero,
            *error_acc,
        );

        // Prefetch next iteration's input
        *iu0 = _mm_loadu_si128(in_ptr.add(64) as *const __m128i);
        *iu1 = _mm_loadu_si128(in_ptr.add(80) as *const __m128i);
    }

    /// Full strict decode using arithmetic (psubusb-based) validity checking.
    ///
    /// This variant replaces the 2×pshufb + pavgb + paddsb check per vector with
    /// psubusb/pcmpeqb/por arithmetic that executes on FP0-FP3 instead of
    /// competing for FP1/FP2 shuffle ports with the mapping pshufb's.
    ///
    /// Constants: 5 map (delta_asso, delta_values, cpv, madd1, madd2) +
    ///            6 check (to_lower, lo_letter, hi_letter, lo_ds, hi_ds, plus) +
    ///            zero (free via pxor) = 11 XMM regs, leaving 5 temps.
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_base64_ssse3_cstyle_strict_arith(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());
        let out_end = out_ptr.add(out_data.len());

        // Scalar alignment preamble
        while (out_ptr as usize) & 15 != 0 {
            if in_ptr.add(4) > in_end {
                return Some((
                    in_ptr as usize - in_data.as_ptr() as usize,
                    out_ptr as usize - out_data.as_ptr() as usize,
                ));
            }
            let u = crate::simd::scalar::decode_tail_3(
                &*(in_ptr as *const [u8; 4]),
                &mut *(out_ptr as *mut [u8; 3]),
            );
            if let Some((written, cu)) = u {
                if cu == u32::MAX {
                    return None;
                }
                in_ptr = in_ptr.add(4);
                out_ptr = out_ptr.add(written);
                if written < 3 {
                    return Some((
                        in_ptr as usize - in_data.as_ptr() as usize,
                        out_ptr as usize - out_data.as_ptr() as usize,
                    ));
                }
            } else {
                return None;
            }
        }

        let safe_in_end = if in_data.len() >= 4 {
            in_data.as_ptr().add(in_data.len() - 4)
        } else {
            in_data.as_ptr()
        };

        if (safe_in_end as usize).saturating_sub(in_ptr as usize) < 32
            || (out_end as usize).saturating_sub(out_ptr as usize) < 16
        {
            return Some((
                in_ptr as usize - in_data.as_ptr() as usize,
                out_ptr as usize - out_data.as_ptr() as usize,
            ));
        }

        // === Map constants (5 regs) ===
        let delta_asso = _mm_setr_epi8(
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f,
            0x00, 0x0f,
        );
        let delta_values = _mm_setr_epi8(
            0x00,
            0x00,
            0x00,
            0x13,
            0x04,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
            0x00,
            0x10,
            0xc3_u8 as i8,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
        );
        let cpv = _mm_set_epi8(-1, -1, -1, -1, 12, 13, 14, 8, 9, 10, 4, 5, 6, 0, 1, 2);
        let madd_mul_1 = _mm_set1_epi32(0x01400140);
        let madd_mul_2 = _mm_set1_epi32(0x00011000);

        // === Arith check constants (6 regs + zero free) ===
        let to_lower = _mm_set1_epi8(0x20);
        let lo_letter = _mm_set1_epi8(97_u8 as i8); // 'a'
        let hi_letter = _mm_set1_epi8(122_u8 as i8); // 'z'
        let lo_digit_slash = _mm_set1_epi8(47_u8 as i8); // '/'
        let hi_digit_slash = _mm_set1_epi8(57_u8 as i8); // '9'
        let plus_val = _mm_set1_epi8(43_u8 as i8); // '+'
        let zero = _mm_setzero_si128();

        // Error accumulator — nonzero lanes mean invalid bytes detected
        let mut error_acc = _mm_setzero_si128();

        // Double-DS64 unrolled main loop (128 input bytes per iteration)
        if in_ptr as usize + 32 + 64 <= safe_in_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let mut iu0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let mut iu1 = _mm_loadu_si128(in_ptr.add(16) as *const __m128i);

            while in_ptr as usize + 32 + 2 * 64 <= safe_in_end as usize
                && out_ptr as usize + 96 + 4 <= out_end as usize
            {
                process_ds64_strict_arith(
                    in_ptr,
                    out_ptr,
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    to_lower,
                    lo_letter,
                    hi_letter,
                    lo_digit_slash,
                    hi_digit_slash,
                    plus_val,
                    zero,
                    &mut error_acc,
                );
                process_ds64_strict_arith(
                    in_ptr.add(64),
                    out_ptr.add(48),
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    to_lower,
                    lo_letter,
                    hi_letter,
                    lo_digit_slash,
                    hi_digit_slash,
                    plus_val,
                    zero,
                    &mut error_acc,
                );
                in_ptr = in_ptr.add(128);
                out_ptr = out_ptr.add(96);
            }

            // Single-DS64 drain loop
            while in_ptr as usize + 32 + 64 <= safe_in_end as usize
                && out_ptr as usize + 48 + 4 <= out_end as usize
            {
                process_ds64_strict_arith(
                    in_ptr,
                    out_ptr,
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    to_lower,
                    lo_letter,
                    hi_letter,
                    lo_digit_slash,
                    hi_digit_slash,
                    plus_val,
                    zero,
                    &mut error_acc,
                );
                in_ptr = in_ptr.add(64);
                out_ptr = out_ptr.add(48);
            }
        }

        // Scalar tail loop (single-vector)
        while in_ptr as usize + 16 <= safe_in_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);
            let (ov, _shifted) =
                map_and_pack(iv, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
            _mm_storeu_si128(out_ptr as *mut __m128i, ov);
            let chk = arith_check_vec(
                iv,
                to_lower,
                lo_letter,
                hi_letter,
                lo_digit_slash,
                hi_digit_slash,
                plus_val,
                zero,
            );
            error_acc = _mm_or_si128(error_acc, chk);
            in_ptr = in_ptr.add(16);
            out_ptr = out_ptr.add(12);
        }

        // Final check: any nonzero lane in error_acc means invalid input
        if _mm_movemask_epi8(error_acc) != 0 {
            return None;
        }

        Some((
            in_ptr as usize - in_data.as_ptr() as usize,
            out_ptr as usize - out_data.as_ptr() as usize,
        ))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_base64_ssse3_cstyle(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());
        let out_end = out_ptr.add(out_data.len());

        while (out_ptr as usize) & 15 != 0 {
            if in_ptr.add(4) > in_end {
                return Some((
                    in_ptr as usize - in_data.as_ptr() as usize,
                    out_ptr as usize - out_data.as_ptr() as usize,
                ));
            }
            let u = crate::simd::scalar::decode_tail_3(
                &*(in_ptr as *const [u8; 4]),
                &mut *(out_ptr as *mut [u8; 3]),
            );
            if let Some((written, cu)) = u {
                if cu == u32::MAX {
                    return None;
                }
                in_ptr = in_ptr.add(4);
                out_ptr = out_ptr.add(written);
                if written < 3 {
                    return Some((
                        in_ptr as usize - in_data.as_ptr() as usize,
                        out_ptr as usize - out_data.as_ptr() as usize,
                    ));
                }
            } else {
                return None;
            }
        }

        let safe_in_end = if in_data.len() >= 4 {
            in_data.as_ptr().add(in_data.len() - 4)
        } else {
            in_data.as_ptr()
        };

        if (safe_in_end as usize).saturating_sub(in_ptr as usize) < 32
            || (out_end as usize).saturating_sub(out_ptr as usize) < 16
        {
            return Some((
                in_ptr as usize - in_data.as_ptr() as usize,
                out_ptr as usize - out_data.as_ptr() as usize,
            ));
        }

        let delta_asso = _mm_setr_epi8(
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f,
            0x00, 0x0f,
        );
        let delta_values = _mm_setr_epi8(
            0x00,
            0x00,
            0x00,
            0x13,
            0x04,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
            0x00,
            0x10,
            0xc3_u8 as i8,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
        );
        let check_asso = _mm_setr_epi8(
            0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03, 0x07, 0x0b, 0x0b,
            0x0b, 0x0f,
        );
        let check_values = _mm_setr_epi8(
            0x80_u8 as i8,
            0x80_u8 as i8,
            0x80_u8 as i8,
            0x80_u8 as i8,
            0xcf_u8 as i8,
            0xbf_u8 as i8,
            0xd5_u8 as i8,
            0xa6_u8 as i8,
            0xb5_u8 as i8,
            0x86_u8 as i8,
            0xd1_u8 as i8,
            0x80_u8 as i8,
            0xb1_u8 as i8,
            0x80_u8 as i8,
            0x91_u8 as i8,
            0x80_u8 as i8,
        );
        let cpv = _mm_set_epi8(-1, -1, -1, -1, 12, 13, 14, 8, 9, 10, 4, 5, 6, 0, 1, 2);
        let madd_mul_1 = _mm_set1_epi32(0x01400140);
        let madd_mul_2 = _mm_set1_epi32(0x00011000);
        let mut error_mask = _mm_setzero_si128();

        if in_ptr as usize + 32 + 64 <= safe_in_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let mut iu0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let mut iu1 = _mm_loadu_si128(in_ptr.add(16) as *const __m128i);

            while in_ptr as usize + 32 + 2 * 64 <= safe_in_end as usize
                && out_ptr as usize + 96 + 4 <= out_end as usize
            {
                process_ds64_partial(
                    in_ptr,
                    out_ptr,
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    check_asso,
                    check_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    &mut error_mask,
                );
                process_ds64_partial(
                    in_ptr.add(64),
                    out_ptr.add(48),
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    check_asso,
                    check_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    &mut error_mask,
                );
                in_ptr = in_ptr.add(128);
                out_ptr = out_ptr.add(96);
            }

            while in_ptr as usize + 32 + 64 <= safe_in_end as usize
                && out_ptr as usize + 48 + 4 <= out_end as usize
            {
                process_ds64_partial(
                    in_ptr,
                    out_ptr,
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    check_asso,
                    check_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    &mut error_mask,
                );
                in_ptr = in_ptr.add(64);
                out_ptr = out_ptr.add(48);
            }
        }

        while in_ptr as usize + 16 <= safe_in_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);
            let (ov, shifted) =
                map_and_pack(iv, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
            _mm_storeu_si128(out_ptr as *mut __m128i, ov);
            error_mask = accumulate_check(iv, shifted, check_asso, check_values, error_mask);
            in_ptr = in_ptr.add(16);
            out_ptr = out_ptr.add(12);
        }

        if _mm_movemask_epi8(error_mask) != 0 {
            return None;
        }

        Some((
            in_ptr as usize - in_data.as_ptr() as usize,
            out_ptr as usize - out_data.as_ptr() as usize,
        ))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_base64_ssse3_cstyle_strict(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());
        let out_end = out_ptr.add(out_data.len());

        while (out_ptr as usize) & 15 != 0 {
            if in_ptr.add(4) > in_end {
                return Some((
                    in_ptr as usize - in_data.as_ptr() as usize,
                    out_ptr as usize - out_data.as_ptr() as usize,
                ));
            }
            let u = crate::simd::scalar::decode_tail_3(
                &*(in_ptr as *const [u8; 4]),
                &mut *(out_ptr as *mut [u8; 3]),
            );
            if let Some((written, cu)) = u {
                if cu == u32::MAX {
                    return None;
                }
                in_ptr = in_ptr.add(4);
                out_ptr = out_ptr.add(written);
                if written < 3 {
                    return Some((
                        in_ptr as usize - in_data.as_ptr() as usize,
                        out_ptr as usize - out_data.as_ptr() as usize,
                    ));
                }
            } else {
                return None;
            }
        }

        let safe_in_end = if in_data.len() >= 4 {
            in_data.as_ptr().add(in_data.len() - 4)
        } else {
            in_data.as_ptr()
        };

        if (safe_in_end as usize).saturating_sub(in_ptr as usize) < 32
            || (out_end as usize).saturating_sub(out_ptr as usize) < 16
        {
            return Some((
                in_ptr as usize - in_data.as_ptr() as usize,
                out_ptr as usize - out_data.as_ptr() as usize,
            ));
        }

        let delta_asso = _mm_setr_epi8(
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f,
            0x00, 0x0f,
        );
        let delta_values = _mm_setr_epi8(
            0x00,
            0x00,
            0x00,
            0x13,
            0x04,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
            0x00,
            0x10,
            0xc3_u8 as i8,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
        );
        let check_asso = _mm_setr_epi8(
            0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03, 0x07, 0x0b, 0x0b,
            0x0b, 0x0f,
        );
        let check_values = _mm_setr_epi8(
            0x80_u8 as i8,
            0x80_u8 as i8,
            0x80_u8 as i8,
            0x80_u8 as i8,
            0xcf_u8 as i8,
            0xbf_u8 as i8,
            0xd5_u8 as i8,
            0xa6_u8 as i8,
            0xb5_u8 as i8,
            0x86_u8 as i8,
            0xd1_u8 as i8,
            0x80_u8 as i8,
            0xb1_u8 as i8,
            0x80_u8 as i8,
            0x91_u8 as i8,
            0x80_u8 as i8,
        );
        let cpv = _mm_set_epi8(-1, -1, -1, -1, 12, 13, 14, 8, 9, 10, 4, 5, 6, 0, 1, 2);
        let madd_mul_1 = _mm_set1_epi32(0x01400140);
        let madd_mul_2 = _mm_set1_epi32(0x00011000);
        let mut error_bits = 0i32;

        if in_ptr as usize + 32 + 64 <= safe_in_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let mut iu0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let mut iu1 = _mm_loadu_si128(in_ptr.add(16) as *const __m128i);

            while in_ptr as usize + 32 + 2 * 64 <= safe_in_end as usize
                && out_ptr as usize + 96 + 4 <= out_end as usize
            {
                process_ds64_strict(
                    in_ptr,
                    out_ptr,
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    check_asso,
                    check_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    &mut error_bits,
                );
                process_ds64_strict(
                    in_ptr.add(64),
                    out_ptr.add(48),
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    check_asso,
                    check_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    &mut error_bits,
                );
                in_ptr = in_ptr.add(128);
                out_ptr = out_ptr.add(96);
            }

            while in_ptr as usize + 32 + 64 <= safe_in_end as usize
                && out_ptr as usize + 48 + 4 <= out_end as usize
            {
                process_ds64_strict(
                    in_ptr,
                    out_ptr,
                    &mut iu0,
                    &mut iu1,
                    delta_asso,
                    delta_values,
                    check_asso,
                    check_values,
                    cpv,
                    madd_mul_1,
                    madd_mul_2,
                    &mut error_bits,
                );
                in_ptr = in_ptr.add(64);
                out_ptr = out_ptr.add(48);
            }
        }

        while in_ptr as usize + 16 <= safe_in_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);
            let (ov, shifted) =
                map_and_pack(iv, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
            _mm_storeu_si128(out_ptr as *mut __m128i, ov);
            error_bits |= check_mask_bits(iv, shifted, check_asso, check_values);
            in_ptr = in_ptr.add(16);
            out_ptr = out_ptr.add(12);
        }

        if error_bits != 0 {
            return None;
        }

        Some((
            in_ptr as usize - in_data.as_ptr() as usize,
            out_ptr as usize - out_data.as_ptr() as usize,
        ))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_base64_ssse3_cstyle_strict_range(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());
        let out_end = out_ptr.add(out_data.len());

        while (out_ptr as usize) & 15 != 0 {
            if in_ptr.add(4) > in_end {
                return Some((
                    in_ptr as usize - in_data.as_ptr() as usize,
                    out_ptr as usize - out_data.as_ptr() as usize,
                ));
            }
            let u = crate::simd::scalar::decode_tail_3(
                &*(in_ptr as *const [u8; 4]),
                &mut *(out_ptr as *mut [u8; 3]),
            );
            if let Some((written, cu)) = u {
                if cu == u32::MAX {
                    return None;
                }
                in_ptr = in_ptr.add(4);
                out_ptr = out_ptr.add(written);
                if written < 3 {
                    return Some((
                        in_ptr as usize - in_data.as_ptr() as usize,
                        out_ptr as usize - out_data.as_ptr() as usize,
                    ));
                }
            } else {
                return None;
            }
        }

        let safe_in_end = if in_data.len() >= 4 {
            in_data.as_ptr().add(in_data.len() - 4)
        } else {
            in_data.as_ptr()
        };

        if (safe_in_end as usize).saturating_sub(in_ptr as usize) < 32
            || (out_end as usize).saturating_sub(out_ptr as usize) < 16
        {
            return Some((
                in_ptr as usize - in_data.as_ptr() as usize,
                out_ptr as usize - out_data.as_ptr() as usize,
            ));
        }

        let delta_asso = _mm_setr_epi8(
            0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f,
            0x00, 0x0f,
        );
        let delta_values = _mm_setr_epi8(
            0x00,
            0x00,
            0x00,
            0x13,
            0x04,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
            0x00,
            0x10,
            0xc3_u8 as i8,
            0xbf_u8 as i8,
            0xbf_u8 as i8,
            0xb9_u8 as i8,
            0xb9_u8 as i8,
        );
        let cpv = _mm_set_epi8(-1, -1, -1, -1, 12, 13, 14, 8, 9, 10, 4, 5, 6, 0, 1, 2);
        let madd_mul_1 = _mm_set1_epi32(0x01400140);
        let madd_mul_2 = _mm_set1_epi32(0x00011000);

        let letter_lo = _mm_set1_epi8((b'a' - 1) as i8);
        let letter_hi = _mm_set1_epi8(b'z' as i8);
        let digit_lo = _mm_set1_epi8((b'0' - 1) as i8);
        let digit_hi = _mm_set1_epi8(b'9' as i8);
        let plus_ch = _mm_set1_epi8(b'+' as i8);
        let slash_ch = _mm_set1_epi8(b'/' as i8);
        let to_lower = _mm_set1_epi8(0x20_u8 as i8);
        let mut error_bits = 0i32;

        if in_ptr as usize + 32 + 64 <= safe_in_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let mut iu0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let mut iu1 = _mm_loadu_si128(in_ptr.add(16) as *const __m128i);

            while in_ptr as usize + 32 + 64 <= safe_in_end as usize
                && out_ptr as usize + 48 + 4 <= out_end as usize
            {
                let iv0 = _mm_loadu_si128(in_ptr.add(32) as *const __m128i);
                let iv1 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);

                let (ou0, _) =
                    map_and_pack(iu0, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
                let (ou1, _) =
                    map_and_pack(iu1, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
                _mm_storeu_si128(out_ptr as *mut __m128i, ou0);
                _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, ou1);
                error_bits |= check_mask_bits_range_folded(
                    iu0, letter_lo, letter_hi, digit_lo, digit_hi, plus_ch, slash_ch, to_lower,
                );
                error_bits |= check_mask_bits_range_folded(
                    iu1, letter_lo, letter_hi, digit_lo, digit_hi, plus_ch, slash_ch, to_lower,
                );

                iu0 = _mm_loadu_si128(in_ptr.add(64) as *const __m128i);
                iu1 = _mm_loadu_si128(in_ptr.add(80) as *const __m128i);

                let (ov2, _) =
                    map_and_pack(iv0, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
                let (ov3, _) =
                    map_and_pack(iv1, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
                _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, ov2);
                _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, ov3);
                error_bits |= check_mask_bits_range_folded(
                    iv0, letter_lo, letter_hi, digit_lo, digit_hi, plus_ch, slash_ch, to_lower,
                );
                error_bits |= check_mask_bits_range_folded(
                    iv1, letter_lo, letter_hi, digit_lo, digit_hi, plus_ch, slash_ch, to_lower,
                );

                in_ptr = in_ptr.add(64);
                out_ptr = out_ptr.add(48);
            }
        }

        while in_ptr as usize + 16 <= safe_in_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);
            let (ov, _) = map_and_pack(iv, delta_asso, delta_values, cpv, madd_mul_1, madd_mul_2);
            _mm_storeu_si128(out_ptr as *mut __m128i, ov);
            error_bits |= check_mask_bits_range_folded(
                iv, letter_lo, letter_hi, digit_lo, digit_hi, plus_ch, slash_ch, to_lower,
            );
            in_ptr = in_ptr.add(16);
            out_ptr = out_ptr.add(12);
        }

        if error_bits != 0 {
            return None;
        }

        Some((
            in_ptr as usize - in_data.as_ptr() as usize,
            out_ptr as usize - out_data.as_ptr() as usize,
        ))
    }
}
