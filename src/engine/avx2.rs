#![allow(unsafe_op_in_unsafe_fn)]

use super::Base64Decoder;
use super::scalar::{decode_base64_fast, encode_base64_fast};
use super::ssse3::DecodeOpts;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// AVX2 Base64 encoder and decoder.
///
/// The decoder validation mode is controlled by [`DecodeOpts`]:
///
/// - `Avx2Decoder::new()` — strict mode (default, all vectors validated).
/// - `Avx2Decoder::with_opts(opts)` — custom configuration.
pub struct Avx2Decoder {
    opts: DecodeOpts,
}

impl Avx2Decoder {
    /// Create a new decoder with default options (strict mode).
    #[inline]
    pub fn new() -> Self {
        Self {
            opts: DecodeOpts::default(),
        }
    }

    /// Create a new decoder with the given options.
    #[inline]
    pub fn with_opts(opts: DecodeOpts) -> Self {
        Self { opts }
    }

    /// Decode Base64 `input` into `out`, returning the number of bytes written.
    ///
    /// Dispatches to the strict or non-strict AVX2 engine based on
    /// `self.opts.strict`, falling back to scalar for the tail.
    ///
    /// Returns `None` if the input contains invalid Base64 characters.
    #[inline]
    pub fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        if std::is_x86_feature_detected!("avx2") {
            let engine_fn = if self.opts.strict {
                decode_engine::decode_avx2_strict
                    as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            } else {
                decode_engine::decode_avx2 as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            };
            return dispatch_decode(input, out, engine_fn);
        }

        decode_base64_fast(input, out)
    }

    /// Encode raw bytes to Base64 ASCII, returning the number of bytes written.
    ///
    /// Uses AVX2 vectorised encoding when available, falling back to scalar
    /// for the tail.
    #[inline]
    pub fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        let mut consumed = 0usize;
        let mut written = 0usize;

        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: feature gate checked above.
            let (c, w) = unsafe { avx2_engine::encode_base64_avx2(input, out) };
            consumed = c;
            written = w;
        }

        if consumed < input.len() {
            let tail_written = encode_base64_fast(&input[consumed..], &mut out[written..]);
            written += tail_written;
        }
        written
    }
}

impl Default for Avx2Decoder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Decoder for Avx2Decoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        Avx2Decoder::decode_to_slice(self, input, out)
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        Avx2Decoder::encode_to_slice(self, input, out)
    }
}

// ---------------------------------------------------------------------------
// Shared decode dispatch (mirrors ssse3::dispatch_decode)
// ---------------------------------------------------------------------------

/// Dispatches an AVX2 decode function, falling back to scalar for the tail.
#[allow(clippy::type_complexity)]
#[inline]
fn dispatch_decode(
    input: &[u8],
    out: &mut [u8],
    simd_fn: unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>,
) -> Option<usize> {
    let (consumed, mut written) = unsafe { simd_fn(input, out)? };

    if consumed < input.len() {
        let tail_written = decode_base64_fast(&input[consumed..], &mut out[written..])?;
        written += tail_written;
    }
    Some(written)
}

// ---------------------------------------------------------------------------
// Decode engine — all functions require `target_feature(enable = "avx2")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod decode_engine {
    use super::*;

    /// SIMD lookup tables for the AVX2 Turbo-Base64 decode pipeline.
    ///
    /// Each table is a `__m256i` with the same 16-byte pattern duplicated in
    /// both 128-bit lanes (because `vpshufb` operates per-lane).
    struct DecodeTables {
        /// First LUT for delta-mapping hash.
        delta_asso: __m256i,
        /// Second LUT: converts ASCII -> 6-bit values.
        delta_values: __m256i,
        /// First LUT for validity check hash.
        check_asso: __m256i,
        /// Second LUT for validity check.
        check_values: __m256i,
        /// Cross-pack vector: compacts 4x(3-byte-in-32-bit-lane) -> 12 contiguous bytes.
        cpv: __m256i,
    }

    impl DecodeTables {
        /// Initialise all SIMD constants.
        ///
        /// # Safety
        ///
        /// Caller must ensure the AVX2 feature is available.
        #[inline]
        #[target_feature(enable = "avx2")]
        unsafe fn new() -> Self {
            Self {
                delta_asso: _mm256_setr_epi8(
                    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x0f, 0x00, 0x0f, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x0f, 0x00, 0x0f,
                ),
                delta_values: _mm256_setr_epi8(
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
                ),
                check_asso: _mm256_setr_epi8(
                    0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03, 0x07, 0x0b,
                    0x0b, 0x0b, 0x0f, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
                    0x03, 0x07, 0x0b, 0x0b, 0x0b, 0x0f,
                ),
                check_values: _mm256_setr_epi8(
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
                ),
                cpv: _mm256_set_epi8(
                    -1, -1, -1, -1, 12, 13, 14, 8, 9, 10, 4, 5, 6, 0, 1, 2, -1, -1, -1, -1, 12, 13,
                    14, 8, 9, 10, 4, 5, 6, 0, 1, 2,
                ),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Core SIMD helpers — AVX2 (256-bit)
    // -----------------------------------------------------------------------

    /// Map a pair of 32-byte Base64 input vectors to decoded output and compute
    /// `shifted` values for both (interleaved for ILP).
    ///
    /// Port of C macro `BITMAP256V8_6x`.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn bitmap256v8_6x(
        iv0: __m256i,
        iv1: __m256i,
        t: &DecodeTables,
    ) -> (__m256i, __m256i, __m256i, __m256i) {
        let mut delta_hash0 = _mm256_shuffle_epi8(t.delta_asso, iv0);
        let mut delta_hash1 = _mm256_shuffle_epi8(t.delta_asso, iv1);
        let shifted0 = _mm256_srli_epi32(iv0, 3);
        delta_hash0 = _mm256_avg_epu8(delta_hash0, shifted0);
        let shifted1 = _mm256_srli_epi32(iv1, 3);
        delta_hash1 = _mm256_avg_epu8(delta_hash1, shifted1);
        let ov0 = _mm256_add_epi8(_mm256_shuffle_epi8(t.delta_values, delta_hash0), iv0);
        let ov1 = _mm256_add_epi8(_mm256_shuffle_epi8(t.delta_values, delta_hash1), iv1);
        (ov0, shifted0, ov1, shifted1)
    }

    /// Bit-pack a pair of mapped vectors: merge 6-bit pairs into 24-bit output
    /// values and compact via cross-pack shuffle (interleaved for ILP).
    ///
    /// Port of C macro `BITPACK256V8_6x`.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn bitpack256v8_6x(v0: __m256i, v1: __m256i, cpv: __m256i) -> (__m256i, __m256i) {
        let mul1 = _mm256_set1_epi32(0x01400140);
        let mul2 = _mm256_set1_epi32(0x00011000);
        let merge0 = _mm256_maddubs_epi16(v0, mul1);
        let merge1 = _mm256_maddubs_epi16(v1, mul1);
        let packed0 = _mm256_madd_epi16(merge0, mul2);
        let packed1 = _mm256_madd_epi16(merge1, mul2);
        (
            _mm256_shuffle_epi8(packed0, cpv),
            _mm256_shuffle_epi8(packed1, cpv),
        )
    }

    /// Map a single 32-byte Base64 input vector.
    ///
    /// Port of C macro `BITMAP256V8_6`.
    #[allow(dead_code)]
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn bitmap256v8_6(iv: __m256i, t: &DecodeTables) -> (__m256i, __m256i) {
        let shifted = _mm256_srli_epi32(iv, 3);
        let delta_hash = _mm256_avg_epu8(_mm256_shuffle_epi8(t.delta_asso, iv), shifted);
        let ov = _mm256_add_epi8(_mm256_shuffle_epi8(t.delta_values, delta_hash), iv);
        (ov, shifted)
    }

    /// Bit-pack a single mapped vector.
    ///
    /// Port of C macro `BITPACK256V8_6`.
    #[allow(dead_code)]
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn bitpack256v8_6(v: __m256i, cpv: __m256i) -> __m256i {
        let mul1 = _mm256_set1_epi32(0x01400140);
        let mul2 = _mm256_set1_epi32(0x00011000);
        let merge = _mm256_maddubs_epi16(v, mul1);
        let packed = _mm256_madd_epi16(merge, mul2);
        _mm256_shuffle_epi8(packed, cpv)
    }

    /// Accumulate validity check into `vx` (OR-accumulator).
    ///
    /// Port of C macro `B64CHK256`.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn b64chk256(iv: __m256i, shifted: __m256i, t: &DecodeTables, vx: __m256i) -> __m256i {
        let check_hash = _mm256_avg_epu8(_mm256_shuffle_epi8(t.check_asso, iv), shifted);
        let chk = _mm256_adds_epi8(_mm256_shuffle_epi8(t.check_values, check_hash), iv);
        _mm256_or_si256(vx, chk)
    }

    // -----------------------------------------------------------------------
    // 128-bit (SSE) helpers for the tail loop
    // -----------------------------------------------------------------------

    /// Map + pack a 16-byte input vector using the low lanes of the 256-bit
    /// tables. Port of C macros `BITMAP128V8_6` + `BITPACK128V8_6`.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn map_and_pack_128(
        iv: __m128i,
        delta_asso_lo: __m128i,
        delta_values_lo: __m128i,
        cpv_lo: __m128i,
    ) -> (__m128i, __m128i) {
        let shifted = _mm_srli_epi32(iv, 3);
        let delta_hash = _mm_avg_epu8(_mm_shuffle_epi8(delta_asso_lo, iv), shifted);
        let mut ov = _mm_add_epi8(_mm_shuffle_epi8(delta_values_lo, delta_hash), iv);
        let mul1 = _mm_set1_epi32(0x01400140);
        let mul2 = _mm_set1_epi32(0x00011000);
        let merge = _mm_maddubs_epi16(ov, mul1);
        ov = _mm_madd_epi16(merge, mul2);
        ov = _mm_shuffle_epi8(ov, cpv_lo);
        (ov, shifted)
    }

    /// Validity check for a 128-bit vector, accumulating into `_vx`.
    /// Port of C macro `B64CHK128`.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn b64chk128(
        iv: __m128i,
        shifted: __m128i,
        check_asso_lo: __m128i,
        check_values_lo: __m128i,
        _vx: __m128i,
    ) -> __m128i {
        let check_hash = _mm_avg_epu8(_mm_shuffle_epi8(check_asso_lo, iv), shifted);
        let chk = _mm_adds_epi8(_mm_shuffle_epi8(check_values_lo, check_hash), iv);
        _mm_or_si128(_vx, chk)
    }

    // -----------------------------------------------------------------------
    // DS128 block processors
    // -----------------------------------------------------------------------

    /// Process one DS128 block in non-strict (CHECK0) mode.
    ///
    /// 128 input bytes -> 96 output bytes. Direct port of the C `DS128(_i_)`
    /// macro with CHECK0 semantics: only the first vector of the first pair
    /// is validated; the second pair is always validated.
    ///
    /// `iu0`/`iu1` are forwarded: on entry they hold the current block's first
    /// pair; on exit they hold the next block's first pair.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn process_ds128_partial(
        ip: *const u8,
        op: *mut u8,
        base: usize,
        iu0: &mut __m256i,
        iu1: &mut __m256i,
        t: &DecodeTables,
        vx: &mut __m256i,
    ) {
        // Load second pair of this block
        let iv0 = _mm256_loadu_si256(ip.add(64 + base) as *const __m256i);
        let iv1 = _mm256_loadu_si256(ip.add(64 + base + 32) as *const __m256i);

        // Map + pack first pair (iu0, iu1) — interleaved
        let (ou0, shiftedu0, ou1, _shiftedu1) = bitmap256v8_6x(*iu0, *iu1, t);
        let (ou0, ou1) = bitpack256v8_6x(ou0, ou1, t.cpv);

        // CHECK0: validate only first vector
        *vx = b64chk256(*iu0, shiftedu0, t, *vx);

        // Forward-load next iteration's first pair
        *iu0 = _mm256_loadu_si256(ip.add(64 + base + 64) as *const __m256i);
        // CHECK1 on iu1 is skipped in non-strict mode
        *iu1 = _mm256_loadu_si256(ip.add(64 + base + 96) as *const __m256i);

        // Store first pair as 4 x 128-bit halves
        let ob = base / 4 * 3; // 128 input -> 96 output offset
        _mm_storeu_si128(op.add(ob) as *mut __m128i, _mm256_castsi256_si128(ou0));
        _mm_storeu_si128(
            op.add(ob + 12) as *mut __m128i,
            _mm256_extracti128_si256(ou0, 1),
        );
        _mm_storeu_si128(op.add(ob + 24) as *mut __m128i, _mm256_castsi256_si128(ou1));
        _mm_storeu_si128(
            op.add(ob + 36) as *mut __m128i,
            _mm256_extracti128_si256(ou1, 1),
        );

        // Map + pack second pair (iv0, iv1)
        let (ov0, shiftedv0, ov1, shiftedv1) = bitmap256v8_6x(iv0, iv1, t);
        let (ov0, ov1) = bitpack256v8_6x(ov0, ov1, t.cpv);

        // CHECK1: second pair is always validated
        *vx = b64chk256(iv0, shiftedv0, t, *vx);
        *vx = b64chk256(iv1, shiftedv1, t, *vx);

        // Store second pair
        _mm_storeu_si128(op.add(ob + 48) as *mut __m128i, _mm256_castsi256_si128(ov0));
        _mm_storeu_si128(
            op.add(ob + 60) as *mut __m128i,
            _mm256_extracti128_si256(ov0, 1),
        );
        _mm_storeu_si128(op.add(ob + 72) as *mut __m128i, _mm256_castsi256_si128(ov1));
        _mm_storeu_si128(
            op.add(ob + 84) as *mut __m128i,
            _mm256_extracti128_si256(ov1, 1),
        );
    }

    /// Process one DS128 block in strict (B64CHECK) mode.
    ///
    /// Same as [`process_ds128_partial`] but validates ALL four vectors
    /// (both pairs), matching Turbo-Base64's B64CHECK behaviour.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn process_ds128_strict(
        ip: *const u8,
        op: *mut u8,
        base: usize,
        iu0: &mut __m256i,
        iu1: &mut __m256i,
        t: &DecodeTables,
        vx: &mut __m256i,
    ) {
        // Load second pair of this block
        let iv0 = _mm256_loadu_si256(ip.add(64 + base) as *const __m256i);
        let iv1 = _mm256_loadu_si256(ip.add(64 + base + 32) as *const __m256i);

        // Map + pack first pair (iu0, iu1) — interleaved
        let (ou0, shiftedu0, ou1, shiftedu1) = bitmap256v8_6x(*iu0, *iu1, t);
        let (ou0, ou1) = bitpack256v8_6x(ou0, ou1, t.cpv);

        // CHECK0: validate first vector
        *vx = b64chk256(*iu0, shiftedu0, t, *vx);

        // Forward-load next iteration's first pair
        *iu0 = _mm256_loadu_si256(ip.add(64 + base + 64) as *const __m256i);

        // CHECK1: validate second vector (strict mode)
        *vx = b64chk256(*iu1, shiftedu1, t, *vx);

        *iu1 = _mm256_loadu_si256(ip.add(64 + base + 96) as *const __m256i);

        // Store first pair as 4 x 128-bit halves
        let ob = base / 4 * 3;
        _mm_storeu_si128(op.add(ob) as *mut __m128i, _mm256_castsi256_si128(ou0));
        _mm_storeu_si128(
            op.add(ob + 12) as *mut __m128i,
            _mm256_extracti128_si256(ou0, 1),
        );
        _mm_storeu_si128(op.add(ob + 24) as *mut __m128i, _mm256_castsi256_si128(ou1));
        _mm_storeu_si128(
            op.add(ob + 36) as *mut __m128i,
            _mm256_extracti128_si256(ou1, 1),
        );

        // Map + pack second pair (iv0, iv1)
        let (ov0, shiftedv0, ov1, shiftedv1) = bitmap256v8_6x(iv0, iv1, t);
        let (ov0, ov1) = bitpack256v8_6x(ov0, ov1, t.cpv);

        // CHECK1: validate both vectors of second pair
        *vx = b64chk256(iv0, shiftedv0, t, *vx);
        *vx = b64chk256(iv1, shiftedv1, t, *vx);

        // Store second pair
        _mm_storeu_si128(op.add(ob + 48) as *mut __m128i, _mm256_castsi256_si128(ov0));
        _mm_storeu_si128(
            op.add(ob + 60) as *mut __m128i,
            _mm256_extracti128_si256(ov0, 1),
        );
        _mm_storeu_si128(op.add(ob + 72) as *mut __m128i, _mm256_castsi256_si128(ov1));
        _mm_storeu_si128(
            op.add(ob + 84) as *mut __m128i,
            _mm256_extracti128_si256(ov1, 1),
        );
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    /// Return `(consumed, written)` offsets from the base pointers.
    #[inline]
    fn offsets(
        in_ptr: *const u8,
        out_ptr: *const u8,
        in_base: *const u8,
        out_base: *const u8,
    ) -> (usize, usize) {
        (
            in_ptr as usize - in_base as usize,
            out_ptr as usize - out_base as usize,
        )
    }

    // -----------------------------------------------------------------------
    // Public entry points
    // -----------------------------------------------------------------------

    /// Non-strict AVX2 decode (CHECK0 mode).
    ///
    /// Direct port of `tb64v256dec` from `turbob64v256.c` with default
    /// CHECK0/CHECK1 semantics.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX2 is available.
    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn decode_avx2(in_data: &[u8], out_data: &mut [u8]) -> Option<(usize, usize)> {
        let inlen = in_data.len();
        if inlen & 3 != 0 {
            return None;
        }

        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let in_ = in_base.add(inlen);
        let mut ip = in_base;
        let mut op = out_base;
        let mut vx = _mm256_setzero_si256();

        let t = DecodeTables::new();
        let mut _vx: __m128i;

        if inlen >= 64 + 128 + 4 {
            let mut iu0 = _mm256_loadu_si256(ip as *const __m256i);
            let mut iu1 = _mm256_loadu_si256(ip.add(32) as *const __m256i);

            // Double-DS128 unrolled loop: 256 input -> 192 output
            while ip < in_.sub(64 + 2 * 128 + 4) {
                process_ds128_partial(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                process_ds128_partial(ip, op, 128, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(256);
                op = op.add(192);
            }

            // Single-DS128 drain: 128 input -> 96 output
            if ip < in_.sub(64 + 128 + 4) {
                process_ds128_partial(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(128);
                op = op.add(96);
            }

            // Collapse 256-bit error accumulator to 128-bit for tail
            // CHECK0: collapse is active
            _vx = _mm_or_si128(_mm256_extracti128_si256(vx, 1), _mm256_castsi256_si128(vx));
        } else {
            // CHECK0: start with zero
            _vx = _mm_setzero_si128();
            if inlen == 0 {
                return Some((0, 0));
            }
        }

        // SSE tail loop: 16 bytes at a time
        let delta_asso_lo = _mm256_castsi256_si128(t.delta_asso);
        let delta_values_lo = _mm256_castsi256_si128(t.delta_values);
        let cpv_lo = _mm256_castsi256_si128(t.cpv);
        let check_asso_lo = _mm256_castsi256_si128(t.check_asso);
        let check_values_lo = _mm256_castsi256_si128(t.check_values);

        while ip < in_.sub(16 + 4) {
            let iv = _mm_loadu_si128(ip as *const __m128i);
            let (ov, vsh) = map_and_pack_128(iv, delta_asso_lo, delta_values_lo, cpv_lo);
            _mm_storeu_si128(op as *mut __m128i, ov);
            // CHECK0: validate in tail
            _vx = b64chk128(iv, vsh, check_asso_lo, check_values_lo, _vx);
            ip = ip.add(16);
            op = op.add(12);
        }

        // Return (consumed, written) — let dispatch_decode handle the scalar tail.
        // But first check the SIMD error accumulator.
        if _mm_movemask_epi8(_vx) != 0 {
            return None;
        }

        Some(offsets(ip, op, in_base, out_base))
    }

    /// Strict AVX2 decode (B64CHECK mode — all vectors validated).
    ///
    /// Same structure as [`decode_avx2`] but uses the strict
    /// `process_ds128_strict` which validates all 4 vectors per block.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX2 is available.
    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn decode_avx2_strict(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let inlen = in_data.len();
        if inlen & 3 != 0 {
            return None;
        }

        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let in_ = in_base.add(inlen);
        let mut ip = in_base;
        let mut op = out_base;
        let mut vx = _mm256_setzero_si256();

        let t = DecodeTables::new();
        let mut _vx: __m128i;

        if inlen >= 64 + 128 + 4 {
            let mut iu0 = _mm256_loadu_si256(ip as *const __m256i);
            let mut iu1 = _mm256_loadu_si256(ip.add(32) as *const __m256i);

            // Double-DS128 unrolled loop: 256 input -> 192 output
            while ip < in_.sub(64 + 2 * 128 + 4) {
                process_ds128_strict(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                process_ds128_strict(ip, op, 128, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(256);
                op = op.add(192);
            }

            // Single-DS128 drain
            if ip < in_.sub(64 + 128 + 4) {
                process_ds128_strict(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(128);
                op = op.add(96);
            }

            // Collapse 256-bit error accumulator to 128-bit
            _vx = _mm_or_si128(_mm256_extracti128_si256(vx, 1), _mm256_castsi256_si128(vx));
        } else {
            _vx = _mm_setzero_si128();
            if inlen == 0 {
                return Some((0, 0));
            }
        }

        // SSE tail loop
        let delta_asso_lo = _mm256_castsi256_si128(t.delta_asso);
        let delta_values_lo = _mm256_castsi256_si128(t.delta_values);
        let cpv_lo = _mm256_castsi256_si128(t.cpv);
        let check_asso_lo = _mm256_castsi256_si128(t.check_asso);
        let check_values_lo = _mm256_castsi256_si128(t.check_values);

        while ip < in_.sub(16 + 4) {
            let iv = _mm_loadu_si128(ip as *const __m128i);
            let (ov, vsh) = map_and_pack_128(iv, delta_asso_lo, delta_values_lo, cpv_lo);
            _mm_storeu_si128(op as *mut __m128i, ov);
            _vx = b64chk128(iv, vsh, check_asso_lo, check_values_lo, _vx);
            ip = ip.add(16);
            op = op.add(12);
        }

        if _mm_movemask_epi8(_vx) != 0 {
            return None;
        }

        Some(offsets(ip, op, in_base, out_base))
    }
}

// ---------------------------------------------------------------------------
// Encode engine — all functions require `target_feature(enable = "avx2")`
// ---------------------------------------------------------------------------
mod avx2_engine {
    use super::*;

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
            10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1, 10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3,
            4, 1, 2, 0, 1,
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
            0, 0, -16, -19, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, 71, 65, 0, 0, -16, -19, -4, -4,
            -4, -4, -4, -4, -4, -4, -4, -4, 71, 65,
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

        debug_assert!(out_data.len() >= in_data.len().div_ceil(3) * 4);

        if in_data.len() < 56 {
            return (0, 0);
        }
        let limit = in_data.len() - 56;

        // Unroll: 192 bytes in -> 256 bytes out
        if in_data.len() >= 248 {
            let unroll_limit = in_data.len() - 248;

            let u0_128 = _mm_loadu_si128(in_data.as_ptr() as *const __m128i);
            let mut u0 = _mm256_inserti128_si256(
                _mm256_castsi128_si256(u0_128),
                _mm_loadu_si128(in_data.as_ptr().add(12) as *const __m128i),
                1,
            );

            let u1_128 = _mm_loadu_si128(in_data.as_ptr().add(24) as *const __m128i);
            let mut u1 = _mm256_inserti128_si256(
                _mm256_castsi128_si256(u1_128),
                _mm_loadu_si128(in_data.as_ptr().add(36) as *const __m128i),
                1,
            );

            while in_idx <= unroll_limit {
                let ip = in_data.as_ptr().add(in_idx);
                let op = out_data.as_mut_ptr().add(out_idx);

                let v0_128 = _mm_loadu_si128(ip.add(48) as *const __m128i);
                let mut v0 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(v0_128),
                    _mm_loadu_si128(ip.add(60) as *const __m128i),
                    1,
                );

                let v1_128 = _mm_loadu_si128(ip.add(72) as *const __m128i);
                let mut v1 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(v1_128),
                    _mm_loadu_si128(ip.add(84) as *const __m128i),
                    1,
                );

                u0 = process_block_avx2(u0);
                u1 = process_block_avx2(u1);
                _mm256_storeu_si256(op as *mut __m256i, u0);
                _mm256_storeu_si256(op.add(32) as *mut __m256i, u1);

                let u0_128 = _mm_loadu_si128(ip.add(96) as *const __m128i);
                u0 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(u0_128),
                    _mm_loadu_si128(ip.add(108) as *const __m128i),
                    1,
                );

                let u1_128 = _mm_loadu_si128(ip.add(120) as *const __m128i);
                u1 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(u1_128),
                    _mm_loadu_si128(ip.add(132) as *const __m128i),
                    1,
                );

                v0 = process_block_avx2(v0);
                v1 = process_block_avx2(v1);
                _mm256_storeu_si256(op.add(64) as *mut __m256i, v0);
                _mm256_storeu_si256(op.add(96) as *mut __m256i, v1);

                let v0_128 = _mm_loadu_si128(ip.add(144) as *const __m128i);
                v0 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(v0_128),
                    _mm_loadu_si128(ip.add(156) as *const __m128i),
                    1,
                );

                let v1_128 = _mm_loadu_si128(ip.add(168) as *const __m128i);
                v1 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(v1_128),
                    _mm_loadu_si128(ip.add(180) as *const __m128i),
                    1,
                );

                u0 = process_block_avx2(u0);
                u1 = process_block_avx2(u1);
                _mm256_storeu_si256(op.add(128) as *mut __m256i, u0);
                _mm256_storeu_si256(op.add(160) as *mut __m256i, u1);

                let u0_128 = _mm_loadu_si128(ip.add(192) as *const __m128i);
                u0 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(u0_128),
                    _mm_loadu_si128(ip.add(204) as *const __m128i),
                    1,
                );

                let u1_128 = _mm_loadu_si128(ip.add(216) as *const __m128i);
                u1 = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(u1_128),
                    _mm_loadu_si128(ip.add(228) as *const __m128i),
                    1,
                );

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
            let v = _mm256_inserti128_si256(
                _mm256_castsi128_si256(v_128),
                _mm_loadu_si128(in_ptr.add(12) as *const __m128i),
                1,
            );

            let o0 = process_block_avx2(v);

            _mm256_storeu_si256(out_ptr as *mut __m256i, o0);

            in_idx += 24;
            out_idx += 32;
        }

        (in_idx, out_idx)
    }
}
