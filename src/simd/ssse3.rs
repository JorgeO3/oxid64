#![allow(unsafe_op_in_unsafe_fn)]

use super::Base64Decoder;
use super::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};

pub struct Ssse3Decoder;

impl Ssse3Decoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], out: &mut [u8]) -> usize {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                let (c, w) = unsafe { ssse3_engine::encode_base64_ssse3(input, out) };
                consumed = c;
                written = w;
            }
        }

        if consumed < input.len() {
            let tail_written = encode_base64_fast(&input[consumed..], &mut out[written..]);
            written += tail_written;
        }
        written
    }

    #[inline]
    pub fn decode_to_slice(input: &[u8], out: &mut [u8]) -> Option<usize> {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                if let Some((c, w)) = unsafe { ssse3_engine::decode_base64_ssse3(input, out) } {
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

impl Base64Decoder for Ssse3Decoder {
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>> {
        let out_len = decoded_len_strict(input)?;
        let mut out = vec![0u8; out_len];
        let written = Self::decode_to_slice(input, &mut out)?;
        debug_assert_eq!(written, out_len);
        Some(out)
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let out_len = ((input.len() + 2) / 3) * 4;
        let mut out = Vec::<u8>::with_capacity(out_len);
        unsafe {
            out.set_len(out_len);
        }
        let written = Self::encode_to_slice(input, &mut out);
        out.truncate(written);
        debug_assert_eq!(written, out_len);
        out
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod ssse3_engine {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn fast_pmul_x2(
        a0: __m128i,
        a1: __m128i,
        mulhi: __m128i,
        mullo: __m128i,
        mask1: __m128i,
        mask2: __m128i,
    ) -> (__m128i, __m128i) {
        let t0_0 = _mm_and_si128(a0, mask1);
        let t0_1 = _mm_and_si128(a1, mask1);
        let t1_0 = _mm_and_si128(a0, mask2);
        let t1_1 = _mm_and_si128(a1, mask2);

        let hi0 = _mm_mulhi_epu16(t0_0, mulhi);
        let hi1 = _mm_mulhi_epu16(t0_1, mulhi);
        let lo0 = _mm_mullo_epi16(t1_0, mullo);
        let lo1 = _mm_mullo_epi16(t1_1, mullo);

        (_mm_or_si128(hi0, lo0), _mm_or_si128(hi1, lo1))
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_block_x2(
        mut v0: __m128i,
        mut v1: __m128i,
        shuf: __m128i,
        mulhi: __m128i,
        mullo: __m128i,
        mask1: __m128i,
        mask2: __m128i,
        offsets: __m128i,
        subs_val: __m128i,
        cmpgt_val: __m128i,
    ) -> (__m128i, __m128i) {
        v0 = _mm_shuffle_epi8(v0, shuf);
        v1 = _mm_shuffle_epi8(v1, shuf);

        let (res0, res1) = fast_pmul_x2(v0, v1, mulhi, mullo, mask1, mask2);
        v0 = res0;
        v1 = res1;

        let mut vidx_0 = _mm_subs_epu8(v0, subs_val);
        let mut vidx_1 = _mm_subs_epu8(v1, subs_val);
        vidx_0 = _mm_sub_epi8(vidx_0, _mm_cmpgt_epi8(v0, cmpgt_val));
        vidx_1 = _mm_sub_epi8(vidx_1, _mm_cmpgt_epi8(v1, cmpgt_val));

        v0 = _mm_add_epi8(v0, _mm_shuffle_epi8(offsets, vidx_0));
        v1 = _mm_add_epi8(v1, _mm_shuffle_epi8(offsets, vidx_1));

        (v0, v1)
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_base64_ssse3(
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

        while in_ptr as usize + 64 <= safe_in_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let iv0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let iv1 = _mm_loadu_si128(in_ptr.add(16) as *const __m128i);

            let shifted0 = _mm_srli_epi32(iv0, 3);
            let shifted1 = _mm_srli_epi32(iv1, 3);

            let d_hash0 = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv0), shifted0);
            let d_hash1 = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv1), shifted1);

            let c_hash0 = _mm_avg_epu8(_mm_shuffle_epi8(check_asso, iv0), shifted0);
            let c_hash1 = _mm_avg_epu8(_mm_shuffle_epi8(check_asso, iv1), shifted1);

            let mut ov0 = _mm_add_epi8(_mm_shuffle_epi8(delta_values, d_hash0), iv0);
            let mut ov1 = _mm_add_epi8(_mm_shuffle_epi8(delta_values, d_hash1), iv1);

            let chk0 = _mm_adds_epi8(_mm_shuffle_epi8(check_values, c_hash0), iv0);
            let chk1 = _mm_adds_epi8(_mm_shuffle_epi8(check_values, c_hash1), iv1);

            error_mask = _mm_or_si128(error_mask, _mm_or_si128(chk0, chk1));

            let merge0 = _mm_maddubs_epi16(ov0, madd_mul_1);
            let merge1 = _mm_maddubs_epi16(ov1, madd_mul_1);

            ov0 = _mm_madd_epi16(merge0, madd_mul_2);
            ov1 = _mm_madd_epi16(merge1, madd_mul_2);

            _mm_storeu_si128(out_ptr as *mut __m128i, _mm_shuffle_epi8(ov0, cpv));
            _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, _mm_shuffle_epi8(ov1, cpv));

            let iv2 = _mm_loadu_si128(in_ptr.add(32) as *const __m128i);
            let iv3 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);

            let shifted2 = _mm_srli_epi32(iv2, 3);
            let shifted3 = _mm_srli_epi32(iv3, 3);

            let d_hash2 = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv2), shifted2);
            let d_hash3 = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv3), shifted3);

            let c_hash2 = _mm_avg_epu8(_mm_shuffle_epi8(check_asso, iv2), shifted2);
            let c_hash3 = _mm_avg_epu8(_mm_shuffle_epi8(check_asso, iv3), shifted3);

            let mut ov2 = _mm_add_epi8(_mm_shuffle_epi8(delta_values, d_hash2), iv2);
            let mut ov3 = _mm_add_epi8(_mm_shuffle_epi8(delta_values, d_hash3), iv3);

            let chk2 = _mm_adds_epi8(_mm_shuffle_epi8(check_values, c_hash2), iv2);
            let chk3 = _mm_adds_epi8(_mm_shuffle_epi8(check_values, c_hash3), iv3);

            error_mask = _mm_or_si128(error_mask, _mm_or_si128(chk2, chk3));

            let merge2 = _mm_maddubs_epi16(ov2, madd_mul_1);
            let merge3 = _mm_maddubs_epi16(ov3, madd_mul_1);

            ov2 = _mm_madd_epi16(merge2, madd_mul_2);
            ov3 = _mm_madd_epi16(merge3, madd_mul_2);

            _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, _mm_shuffle_epi8(ov2, cpv));
            _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, _mm_shuffle_epi8(ov3, cpv));

            in_ptr = in_ptr.add(64);
            out_ptr = out_ptr.add(48);
        }

        while in_ptr as usize + 16 <= safe_in_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);

            let shifted = _mm_srli_epi32(iv, 3);
            let delta_hash = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso, iv), shifted);
            let mut ov = _mm_add_epi8(_mm_shuffle_epi8(delta_values, delta_hash), iv);
            let check_hash = _mm_avg_epu8(_mm_shuffle_epi8(check_asso, iv), shifted);
            let chk = _mm_adds_epi8(_mm_shuffle_epi8(check_values, check_hash), iv);
            error_mask = _mm_or_si128(error_mask, chk);
            let merge_ab_bc = _mm_maddubs_epi16(ov, madd_mul_1);
            ov = _mm_madd_epi16(merge_ab_bc, madd_mul_2);

            _mm_storeu_si128(out_ptr as *mut __m128i, _mm_shuffle_epi8(ov, cpv));
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
    pub unsafe fn encode_base64_ssse3(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());

        while (out_ptr as usize) & 15 != 0 {
            if in_ptr.add(3) > in_end {
                return (
                    in_ptr as usize - in_data.as_ptr() as usize,
                    out_ptr as usize - out_data.as_ptr() as usize,
                );
            }
            let a = *in_ptr;
            let b = *in_ptr.add(1);
            let c = *in_ptr.add(2);
            crate::simd::scalar::encode_block_3_to_4_ptr(a, b, c, out_ptr);
            in_ptr = in_ptr.add(3);
            out_ptr = out_ptr.add(4);
        }

        if (in_end as usize).saturating_sub(in_ptr as usize) < 52 {
            return (
                in_ptr as usize - in_data.as_ptr() as usize,
                out_ptr as usize - out_data.as_ptr() as usize,
            );
        }

        let shuf = _mm_set_epi8(10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1);
        let mask1 = _mm_set1_epi32(0x0fc0fc00u32 as i32);
        let mulhi = _mm_set1_epi32(0x04000040u32 as i32);
        let mask2 = _mm_set1_epi32(0x003f03f0u32 as i32);
        let mullo = _mm_set1_epi32(0x01000010u32 as i32);
        let offsets = _mm_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        );
        let subs_val = _mm_set1_epi8(51);
        let cmpgt_val = _mm_set1_epi8(25);

        while in_ptr as usize + 108 <= in_end as usize {
            let u0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let u1 = _mm_loadu_si128(in_ptr.add(12) as *const __m128i);

            let (o0, o1) = process_block_x2(
                u0, u1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let v0 = _mm_loadu_si128(in_ptr.add(24) as *const __m128i);
            let v1 = _mm_loadu_si128(in_ptr.add(36) as *const __m128i);

            _mm_store_si128(out_ptr as *mut __m128i, o0);
            _mm_store_si128(out_ptr.add(16) as *mut __m128i, o1);

            let (o2, o3) = process_block_x2(
                v0, v1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let u2 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);
            let u3 = _mm_loadu_si128(in_ptr.add(60) as *const __m128i);

            _mm_store_si128(out_ptr.add(32) as *mut __m128i, o2);
            _mm_store_si128(out_ptr.add(48) as *mut __m128i, o3);

            let (o4, o5) = process_block_x2(
                u2, u3, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let v2 = _mm_loadu_si128(in_ptr.add(72) as *const __m128i);
            let v3 = _mm_loadu_si128(in_ptr.add(84) as *const __m128i);

            _mm_store_si128(out_ptr.add(64) as *mut __m128i, o4);
            _mm_store_si128(out_ptr.add(80) as *mut __m128i, o5);

            let (o6, o7) = process_block_x2(
                v2, v3, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            _mm_store_si128(out_ptr.add(96) as *mut __m128i, o6);
            _mm_store_si128(out_ptr.add(112) as *mut __m128i, o7);

            in_ptr = in_ptr.add(96);
            out_ptr = out_ptr.add(128);
        }

        while in_ptr as usize + 60 <= in_end as usize {
            let u0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let u1 = _mm_loadu_si128(in_ptr.add(12) as *const __m128i);

            let (o0, o1) = process_block_x2(
                u0, u1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let v0 = _mm_loadu_si128(in_ptr.add(24) as *const __m128i);
            let v1 = _mm_loadu_si128(in_ptr.add(36) as *const __m128i);

            _mm_store_si128(out_ptr as *mut __m128i, o0);
            _mm_store_si128(out_ptr.add(16) as *mut __m128i, o1);

            let (o2, o3) = process_block_x2(
                v0, v1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            _mm_store_si128(out_ptr.add(32) as *mut __m128i, o2);
            _mm_store_si128(out_ptr.add(48) as *mut __m128i, o3);

            in_ptr = in_ptr.add(48);
            out_ptr = out_ptr.add(64);
        }

        while in_ptr as usize + 16 <= in_end as usize {
            let v = _mm_loadu_si128(in_ptr as *const __m128i);
            let (res, _) = process_block_x2(
                v,
                _mm_setzero_si128(),
                shuf,
                mulhi,
                mullo,
                mask1,
                mask2,
                offsets,
                subs_val,
                cmpgt_val,
            );
            _mm_storeu_si128(out_ptr as *mut __m128i, res);
            in_ptr = in_ptr.add(12);
            out_ptr = out_ptr.add(16);
        }

        (
            in_ptr as usize - in_data.as_ptr() as usize,
            out_ptr as usize - out_data.as_ptr() as usize,
        )
    }
}
