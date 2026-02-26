#![allow(unsafe_op_in_unsafe_fn)]

use super::Base64Decoder;
use super::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};

pub struct Sse42Decoder;

impl Sse42Decoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], out: &mut [u8]) -> usize {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                let (c, w) = unsafe { sse_engine::encode_base64_ssse3(input, out) };
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
}

impl Base64Decoder for Sse42Decoder {
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>> {
        let out_len = decoded_len_strict(input)?;
        let mut out = vec![0u8; out_len];
        let written = decode_base64_fast(input, &mut out)?;
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
        debug_assert_eq!(written, out_len);
        out
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse_engine {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    #[inline(always)]
    unsafe fn fast_pmul_x2(
        a0: __m128i,
        a1: __m128i,
        mulhi: __m128i,
        mullo: __m128i,
        mask1: __m128i,
        mask2: __m128i,
    ) -> (__m128i, __m128i) {
        let mut t0_0 = _mm_and_si128(a0, mask1);
        let mut t0_1 = _mm_and_si128(a1, mask1);
        let mut t1_0 = _mm_and_si128(a0, mask2);
        let mut t1_1 = _mm_and_si128(a1, mask2);

        std::arch::asm!(
            "pmulhuw {t0_0}, {mulh}",
            "pmulhuw {t0_1}, {mulh}",
            "pmullw {t1_0}, {mull}",
            "pmullw {t1_1}, {mull}",
            t0_0 = inout(xmm_reg) t0_0,
            t0_1 = inout(xmm_reg) t0_1,
            t1_0 = inout(xmm_reg) t1_0,
            t1_1 = inout(xmm_reg) t1_1,
            mulh = in(xmm_reg) mulhi,
            mull = in(xmm_reg) mullo,
            options(pure, nomem, nostack)
        );

        (_mm_or_si128(t0_0, t1_0), _mm_or_si128(t0_1, t1_1))
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

        while in_ptr <= in_end.offset(-96 - 12) {
            let u0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let u1 = _mm_loadu_si128(in_ptr.add(12) as *const __m128i);
            let v0 = _mm_loadu_si128(in_ptr.add(24) as *const __m128i);
            let v1 = _mm_loadu_si128(in_ptr.add(36) as *const __m128i);

            let (o0, o1) = process_block_x2(
                u0, u1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );
            let (o2, o3) = process_block_x2(
                v0, v1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            _mm_store_si128(out_ptr as *mut __m128i, o0);
            _mm_store_si128(out_ptr.add(16) as *mut __m128i, o1);
            _mm_store_si128(out_ptr.add(32) as *mut __m128i, o2);
            _mm_store_si128(out_ptr.add(48) as *mut __m128i, o3);

            let u2 = _mm_loadu_si128(in_ptr.add(48) as *const __m128i);
            let u3 = _mm_loadu_si128(in_ptr.add(60) as *const __m128i);
            let v2 = _mm_loadu_si128(in_ptr.add(72) as *const __m128i);
            let v3 = _mm_loadu_si128(in_ptr.add(84) as *const __m128i);

            let (o4, o5) = process_block_x2(
                u2, u3, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );
            let (o6, o7) = process_block_x2(
                v2, v3, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            _mm_store_si128(out_ptr.add(64) as *mut __m128i, o4);
            _mm_store_si128(out_ptr.add(80) as *mut __m128i, o5);
            _mm_store_si128(out_ptr.add(96) as *mut __m128i, o6);
            _mm_store_si128(out_ptr.add(112) as *mut __m128i, o7);

            in_ptr = in_ptr.add(96);
            out_ptr = out_ptr.add(128);
        }

        while in_ptr <= in_end.offset(-48 - 12) {
            let u0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let u1 = _mm_loadu_si128(in_ptr.add(12) as *const __m128i);
            let v0 = _mm_loadu_si128(in_ptr.add(24) as *const __m128i);
            let v1 = _mm_loadu_si128(in_ptr.add(36) as *const __m128i);

            let (o0, o1) = process_block_x2(
                u0, u1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );
            let (o2, o3) = process_block_x2(
                v0, v1, shuf, mulhi, mullo, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            _mm_store_si128(out_ptr as *mut __m128i, o0);
            _mm_store_si128(out_ptr.add(16) as *mut __m128i, o1);
            _mm_store_si128(out_ptr.add(32) as *mut __m128i, o2);
            _mm_store_si128(out_ptr.add(48) as *mut __m128i, o3);

            in_ptr = in_ptr.add(48);
            out_ptr = out_ptr.add(64);
        }

        while in_ptr <= in_end.offset(-12 - 4) {
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
