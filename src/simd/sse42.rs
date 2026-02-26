#![allow(unsafe_op_in_unsafe_fn)]

use super::Base64Decoder;
use super::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};

pub struct Sse42Decoder;

impl Sse42Decoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], mut out: &mut [u8]) -> usize {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                let (c, w) = unsafe { sse_engine::encode_base64_ssse3(input, &mut out) };
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
        let mut out = vec![0u8; out_len];

        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("ssse3") {
                let (c, w) = unsafe { sse_engine::encode_base64_ssse3(input, &mut out) };
                consumed = c;
                written = w;
            }
        }

        if consumed < input.len() {
            let tail_written = encode_base64_fast(&input[consumed..], &mut out[written..]);
            written += tail_written;
        }

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
    unsafe fn fast_pmulhuw(a: __m128i, b: __m128i) -> __m128i {
        let mut res: __m128i;
        std::arch::asm!(
            "pmulhuw {a}, {b}",
            a = inout(xmm_reg) a => res,
            b = in(xmm_reg) b,
            options(pure, nomem, nostack)
        );
        res
    }

    #[inline(always)]
    unsafe fn fast_pmullw(a: __m128i, b: __m128i) -> __m128i {
        let mut res: __m128i;
        std::arch::asm!(
            "pmullw {a}, {b}",
            a = inout(xmm_reg) a => res,
            b = in(xmm_reg) b,
            options(pure, nomem, nostack)
        );
        res
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_block_x2(mut v0: __m128i, mut v1: __m128i) -> (__m128i, __m128i) {
        let shuf = _mm_set_epi8(10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1);
        v0 = _mm_shuffle_epi8(v0, shuf);
        v1 = _mm_shuffle_epi8(v1, shuf);

        let mask1 = _mm_set1_epi32(0x0fc0fc00u32 as i32);
        let mut t0_0 = _mm_and_si128(v0, mask1);
        let mut t0_1 = _mm_and_si128(v1, mask1);
        let mulhi = _mm_set1_epi32(0x04000040u32 as i32);
        t0_0 = fast_pmulhuw(t0_0, mulhi);
        t0_1 = fast_pmulhuw(t0_1, mulhi);

        let mask2 = _mm_set1_epi32(0x003f03f0u32 as i32);
        let mut t1_0 = _mm_and_si128(v0, mask2);
        let mut t1_1 = _mm_and_si128(v1, mask2);
        let mullo = _mm_set1_epi32(0x01000010u32 as i32);
        t1_0 = fast_pmullw(t1_0, mullo);
        t1_1 = fast_pmullw(t1_1, mullo);

        v0 = _mm_or_si128(t0_0, t1_0);
        v1 = _mm_or_si128(t0_1, t1_1);

        let offsets = _mm_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        );

        let mut vidx_0 = _mm_subs_epu8(v0, _mm_set1_epi8(51));
        let mut vidx_1 = _mm_subs_epu8(v1, _mm_set1_epi8(51));
        vidx_0 = _mm_sub_epi8(vidx_0, _mm_cmpgt_epi8(v0, _mm_set1_epi8(25)));
        vidx_1 = _mm_sub_epi8(vidx_1, _mm_cmpgt_epi8(v1, _mm_set1_epi8(25)));

        let trans_off_0 = _mm_shuffle_epi8(offsets, vidx_0);
        let trans_off_1 = _mm_shuffle_epi8(offsets, vidx_1);
        v0 = _mm_add_epi8(v0, trans_off_0);
        v1 = _mm_add_epi8(v1, trans_off_1);

        (v0, v1)
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_block(v: __m128i) -> __m128i {
        let shuf = _mm_set_epi8(10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1);
        let mut v = _mm_shuffle_epi8(v, shuf);

        // Debido a un bug de optimización en LLVM (scalarization de simd_mul),
        // evitamos el uso del intrínseco _mm_mulhi_epu16 y usamos ensamblador
        // en línea para forzar las instrucciones pmulhuw y pmullw correctas.
        // Esto evita el uso de hacks como core::hint::black_box y es la
        // forma idiomática de lidiar con un backend deficiente en Rust.
        let mask1 = _mm_set1_epi32(0x0fc0fc00u32 as i32);
        let mulhi = _mm_set1_epi32(0x04000040u32 as i32);
        let mut t0 = _mm_and_si128(v, mask1);
        t0 = fast_pmulhuw(t0, mulhi);

        let mask2 = _mm_set1_epi32(0x003f03f0u32 as i32);
        let mullo = _mm_set1_epi32(0x01000010u32 as i32);
        let mut t1 = _mm_and_si128(v, mask2);
        t1 = fast_pmullw(t1, mullo);

        v = _mm_or_si128(t0, t1);

        let offsets = _mm_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        );

        let mut vidx = _mm_subs_epu8(v, _mm_set1_epi8(51));
        vidx = _mm_sub_epi8(vidx, _mm_cmpgt_epi8(v, _mm_set1_epi8(25)));

        let translated_offset = _mm_shuffle_epi8(offsets, vidx);
        _mm_add_epi8(v, translated_offset)
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn encode_base64_ssse3(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let mut in_idx = 0usize;
        let mut out_idx = 0usize;

        debug_assert!(out_data.len() >= ((in_data.len() + 2) / 3) * 4);

        if in_data.len() < 52 {
            return (0, 0);
        }
        let limit = in_data.len() - 52;

        if in_data.len() >= 144 {
            let unroll_limit = in_data.len() - 144;
            let mut u0 = _mm_loadu_si128(in_data.as_ptr() as *const __m128i);
            let mut u1 = _mm_loadu_si128(in_data.as_ptr().add(12) as *const __m128i);

            while in_idx <= unroll_limit {
                let ip = in_data.as_ptr().add(in_idx);
                let op = out_data.as_mut_ptr().add(out_idx);

                let mut v0 = _mm_loadu_si128(ip.add(24) as *const __m128i);
                let mut v1 = _mm_loadu_si128(ip.add(36) as *const __m128i);
                let (new_u0, new_u1) = process_block_x2(u0, u1);
                u0 = new_u0;
                u1 = new_u1;
                _mm_storeu_si128(op as *mut __m128i, u0);
                _mm_storeu_si128(op.add(16) as *mut __m128i, u1);

                u0 = _mm_loadu_si128(ip.add(48) as *const __m128i);
                u1 = _mm_loadu_si128(ip.add(60) as *const __m128i);
                let (new_v0, new_v1) = process_block_x2(v0, v1);
                v0 = new_v0;
                v1 = new_v1;
                _mm_storeu_si128(op.add(32) as *mut __m128i, v0);
                _mm_storeu_si128(op.add(48) as *mut __m128i, v1);

                v0 = _mm_loadu_si128(ip.add(72) as *const __m128i);
                v1 = _mm_loadu_si128(ip.add(84) as *const __m128i);
                let (new_u0, new_u1) = process_block_x2(u0, u1);
                u0 = new_u0;
                u1 = new_u1;
                _mm_storeu_si128(op.add(64) as *mut __m128i, u0);
                _mm_storeu_si128(op.add(80) as *mut __m128i, u1);

                u0 = _mm_loadu_si128(ip.add(96) as *const __m128i);
                u1 = _mm_loadu_si128(ip.add(108) as *const __m128i);
                let (new_v0, new_v1) = process_block_x2(v0, v1);
                v0 = new_v0;
                v1 = new_v1;
                _mm_storeu_si128(op.add(96) as *mut __m128i, v0);
                _mm_storeu_si128(op.add(112) as *mut __m128i, v1);

                in_idx += 96;
                out_idx += 128;
            }
        }

        while in_idx <= limit {
            let in_ptr = in_data.as_ptr().add(in_idx);
            let out_ptr = out_data.as_mut_ptr().add(out_idx);

            let v0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let v1 = _mm_loadu_si128(in_ptr.add(12) as *const __m128i);
            let v2 = _mm_loadu_si128(in_ptr.add(24) as *const __m128i);
            let v3 = _mm_loadu_si128(in_ptr.add(36) as *const __m128i);

            let o0 = process_block(v0);
            let o1 = process_block(v1);
            let o2 = process_block(v2);
            let o3 = process_block(v3);

            _mm_storeu_si128(out_ptr as *mut __m128i, o0);
            _mm_storeu_si128(out_ptr.add(16) as *mut __m128i, o1);
            _mm_storeu_si128(out_ptr.add(32) as *mut __m128i, o2);
            _mm_storeu_si128(out_ptr.add(48) as *mut __m128i, o3);

            in_idx += 48;
            out_idx += 64;
        }

        (in_idx, out_idx)
    }
}
