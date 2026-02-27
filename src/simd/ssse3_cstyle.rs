#![allow(unsafe_op_in_unsafe_fn)]

use super::scalar::decode_base64_fast;

pub struct Ssse3CStyleDecoder;
pub struct Ssse3CStyleStrictDecoder;
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
}
