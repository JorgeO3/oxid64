#![allow(unsafe_op_in_unsafe_fn)]

use super::Base64Decoder;
use super::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};

pub struct Avx2Decoder;

impl Avx2Decoder {
    #[inline]
    pub fn encode_to_slice(input: &[u8], mut out: &mut [u8]) -> usize {
        let mut consumed = 0usize;
        let mut written = 0usize;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::is_x86_feature_detected!("avx2") {
                let (c, w) = unsafe { avx2_engine::encode_base64_avx2(input, &mut out) };
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

impl Base64Decoder for Avx2Decoder {
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
        let written = Self::encode_to_slice(input, &mut out);
        out.truncate(written);
        out
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx2_engine {
    #[cfg(target_arch = "x86")]
    use core::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn fast_vpmulhuw(a: __m256i, b: __m256i) -> __m256i {
        let mut res: __m256i;
        std::arch::asm!(
            "vpmulhuw {res}, {a}, {b}",
            res = out(ymm_reg) res,
            a = in(ymm_reg) a,
            b = in(ymm_reg) b,
            options(pure, nomem, nostack)
        );
        res
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn fast_vpmullw(a: __m256i, b: __m256i) -> __m256i {
        let mut res: __m256i;
        std::arch::asm!(
            "vpmullw {res}, {a}, {b}",
            res = out(ymm_reg) res,
            a = in(ymm_reg) a,
            b = in(ymm_reg) b,
            options(pure, nomem, nostack)
        );
        res
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn process_block_avx2(mut v: __m256i) -> __m256i {
        let shuf = _mm256_set_epi8(
            10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1,
            10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1
        );
        v = _mm256_shuffle_epi8(v, shuf);

        let mask1 = _mm256_set1_epi32(0x0fc0fc00u32 as i32);
        let mulhi = _mm256_set1_epi32(0x04000040u32 as i32);
        let mut t0 = _mm256_and_si256(v, mask1);
        t0 = fast_vpmulhuw(t0, mulhi);

        let mask2 = _mm256_set1_epi32(0x003f03f0u32 as i32);
        let mullo = _mm256_set1_epi32(0x01000010u32 as i32);
        let mut t1 = _mm256_and_si256(v, mask2);
        t1 = fast_vpmullw(t1, mullo);

        v = _mm256_or_si256(t0, t1);
        
        let offsets = _mm256_set_epi8(
            0, 0, -16, -19, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, 71, 65,
            0, 0, -16, -19, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, 71, 65
        );

        let mut vidx = _mm256_subs_epu8(v, _mm256_set1_epi8(51));
        vidx = _mm256_sub_epi8(vidx, _mm256_cmpgt_epi8(v, _mm256_set1_epi8(25)));
        
        let translated_offset = _mm256_shuffle_epi8(offsets, vidx);
        _mm256_add_epi8(v, translated_offset)
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn encode_base64_avx2(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let mut in_idx = 0usize;
        let mut out_idx = 0usize;

        debug_assert!(out_data.len() >= ((in_data.len() + 2) / 3) * 4);

        if in_data.len() < 56 {
            return (0, 0);
        }
        let limit = in_data.len() - 56;

        // Unroll: 192 bytes in -> 256 bytes out
        if in_data.len() >= 248 {
            let unroll_limit = in_data.len() - 248;

            let mut u0_128 = _mm_loadu_si128(in_data.as_ptr() as *const __m128i);
            let mut u0 = _mm256_inserti128_si256(_mm256_castsi128_si256(u0_128), _mm_loadu_si128(in_data.as_ptr().add(12) as *const __m128i), 1);
            
            let mut u1_128 = _mm_loadu_si128(in_data.as_ptr().add(24) as *const __m128i);
            let mut u1 = _mm256_inserti128_si256(_mm256_castsi128_si256(u1_128), _mm_loadu_si128(in_data.as_ptr().add(36) as *const __m128i), 1);
            
            while in_idx <= unroll_limit {
                let ip = in_data.as_ptr().add(in_idx);
                let op = out_data.as_mut_ptr().add(out_idx);

                let mut v0_128 = _mm_loadu_si128(ip.add(48) as *const __m128i);
                let mut v0 = _mm256_inserti128_si256(_mm256_castsi128_si256(v0_128), _mm_loadu_si128(ip.add(60) as *const __m128i), 1);
                
                let mut v1_128 = _mm_loadu_si128(ip.add(72) as *const __m128i);
                let mut v1 = _mm256_inserti128_si256(_mm256_castsi128_si256(v1_128), _mm_loadu_si128(ip.add(84) as *const __m128i), 1);
                
                u0 = process_block_avx2(u0);
                u1 = process_block_avx2(u1);
                _mm256_storeu_si256(op as *mut __m256i, u0);
                _mm256_storeu_si256(op.add(32) as *mut __m256i, u1);

                u0_128 = _mm_loadu_si128(ip.add(96) as *const __m128i);
                u0 = _mm256_inserti128_si256(_mm256_castsi128_si256(u0_128), _mm_loadu_si128(ip.add(108) as *const __m128i), 1);
                
                u1_128 = _mm_loadu_si128(ip.add(120) as *const __m128i);
                u1 = _mm256_inserti128_si256(_mm256_castsi128_si256(u1_128), _mm_loadu_si128(ip.add(132) as *const __m128i), 1);
                
                v0 = process_block_avx2(v0);
                v1 = process_block_avx2(v1);
                _mm256_storeu_si256(op.add(64) as *mut __m256i, v0);
                _mm256_storeu_si256(op.add(96) as *mut __m256i, v1);

                v0_128 = _mm_loadu_si128(ip.add(144) as *const __m128i);
                v0 = _mm256_inserti128_si256(_mm256_castsi128_si256(v0_128), _mm_loadu_si128(ip.add(156) as *const __m128i), 1);
                
                v1_128 = _mm_loadu_si128(ip.add(168) as *const __m128i);
                v1 = _mm256_inserti128_si256(_mm256_castsi128_si256(v1_128), _mm_loadu_si128(ip.add(180) as *const __m128i), 1);

                u0 = process_block_avx2(u0);
                u1 = process_block_avx2(u1);
                _mm256_storeu_si256(op.add(128) as *mut __m256i, u0);
                _mm256_storeu_si256(op.add(160) as *mut __m256i, u1);

                u0_128 = _mm_loadu_si128(ip.add(192) as *const __m128i);
                u0 = _mm256_inserti128_si256(_mm256_castsi128_si256(u0_128), _mm_loadu_si128(ip.add(204) as *const __m128i), 1);
                
                u1_128 = _mm_loadu_si128(ip.add(216) as *const __m128i);
                u1 = _mm256_inserti128_si256(_mm256_castsi128_si256(u1_128), _mm_loadu_si128(ip.add(228) as *const __m128i), 1);

                v0 = process_block_avx2(v0);
                v1 = process_block_avx2(v1);
                _mm256_storeu_si256(op.add(192) as *mut __m256i, v0);
                _mm256_storeu_si256(op.add(224) as *mut __m256i, v1);

                in_idx += 192;
                out_idx += 256;
            }
        }

        while in_idx <= limit {
            let in_ptr = in_data.as_ptr().add(in_idx);
            let out_ptr = out_data.as_mut_ptr().add(out_idx);

            let v_128 = _mm_loadu_si128(in_ptr as *const __m128i);
            let v = _mm256_inserti128_si256(_mm256_castsi128_si256(v_128), _mm_loadu_si128(in_ptr.add(12) as *const __m128i), 1);

            let o0 = process_block_avx2(v);

            _mm256_storeu_si256(out_ptr as *mut __m256i, o0);

            in_idx += 24;
            out_idx += 32;
        }

        (in_idx, out_idx)
    }
}