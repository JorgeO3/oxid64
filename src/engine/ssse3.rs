//! SSSE3 Base64 codec — Turbo-Base64 port.
//!
//! This module provides a high-performance SSSE3 Base64 decoder and encoder
//! ported from [Turbo-Base64](https://github.com/powturbo/Turbo-Base64).
//!
//! # Decoder
//!
//! [`Ssse3Decoder`] supports two validation modes controlled by [`DecodeOpts`]:
//!
//! - **Non-strict** (`strict: false`, ~10.47 GiB/s). Validates only the first
//!   of every four 16-byte vectors per DS64 block, matching the C library's
//!   default `CHECK0`-only behaviour.
//!   Warning: this mode is intended for trusted input or benchmark parity with
//!   Turbo-Base64 C. It is not a full validator for untrusted Base64.
//!
//! - **Strict** (`strict: true`, default, ~7.89 GiB/s). Validates all four
//!   vectors per DS64 block. Beats the C library's `B64CHECK` mode (~7.49
//!   GiB/s) despite the additional pshufb port-5 pressure.
//!
//! # Decoder algorithm overview
//!
//! Each 16-byte input vector of ASCII Base64 characters is decoded in three
//! stages:
//!
//! 1. **Map**: A pshufb-based hash lookup converts each Base64 ASCII byte to
//!    its 6-bit value. Two lookup tables (`delta_asso` + `delta_values`) form
//!    the hash: `delta_hash = pavgb(pshufb(delta_asso, iv), iv >> 3)`, then
//!    `mapped = pshufb(delta_values, delta_hash) + iv`.
//!
//! 2. **Pack**: Three SIMD multiplies (`pmaddubsw`, `pmaddwd`) plus a final
//!    `pshufb` compact four 6-bit values into three 8-bit output bytes,
//!    producing 12 output bytes per 16-byte input vector.
//!
//! 3. **Check**: A parallel pshufb-based hash (using `check_asso` +
//!    `check_values`) produces a vector where negative (sign-bit-set) lanes
//!    indicate invalid input bytes. The strict decoder checks every vector;
//!    the non-strict decoder checks only one per DS64.
//!
//! # Encoder
//!
//! [`Ssse3Decoder::encode_to_slice`] encodes raw bytes to Base64 ASCII using
//! SSSE3 vectorised bit-manipulation, falling back to scalar for the tail.
//! The encoder is independent of [`DecodeOpts`].

use super::scalar::{decode_base64_fast, encode_base64_fast};
use super::{b2i, w2i, Base64Decoder, DecodeOpts};
use crate::engine::common::{
    assert_encode_capacity, can_advance, can_process_ds64, can_process_ds64_double,
    can_process_tail16, can_read, prepare_decode_output, remaining, safe_in_end_4,
};
use crate::engine::models::ssse3 as verify_model;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// SSSE3 Base64 decoder and encoder.
///
/// The decoder validation mode is controlled by [`DecodeOpts`]:
///
/// - `Ssse3Decoder::new()` — strict mode (default, all vectors validated).
/// - `Ssse3Decoder::with_opts(opts)` — custom configuration.
///
/// The encoder ([`encode_to_slice`](Self::encode_to_slice)) is a static
/// associated function that does not depend on decoder options.
pub struct Ssse3Decoder {
    opts: DecodeOpts,
}

impl Ssse3Decoder {
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
    /// Dispatches to the strict or non-strict SSSE3 engine based on
    /// `self.opts.strict`, falling back to scalar for the tail.
    ///
    /// Returns `None` if the input contains invalid Base64 characters.
    #[inline]
    pub fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        let _out_len = prepare_decode_output(input, out)?;

        if std::is_x86_feature_detected!("ssse3") {
            let engine_fn = if self.opts.strict {
                decode_engine::decode_ssse3_strict
                    as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            } else {
                decode_engine::decode_ssse3 as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            };
            return super::dispatch_decode(input, out, engine_fn);
        }

        decode_base64_fast(input, out)
    }

    /// Encode raw bytes to Base64 ASCII, returning the number of bytes written.
    ///
    /// Uses SSSE3 vectorised encoding when available, falling back to scalar
    /// for the tail. The encoder is independent of [`DecodeOpts`].
    #[inline]
    pub fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        assert_encode_capacity(input.len(), out.len());

        let mut consumed = 0usize;
        let mut written = 0usize;

        if std::is_x86_feature_detected!("ssse3") {
            // SAFETY: feature gate checked above.
            let (c, w) = unsafe { encode_engine::encode_base64_ssse3(input, out) };
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

impl Default for Ssse3Decoder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Decoder for Ssse3Decoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        Ssse3Decoder::decode_to_slice(self, input, out)
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        Ssse3Decoder::encode_to_slice(self, input, out)
    }
}

// ---------------------------------------------------------------------------
// Shared decode dispatch
// ---------------------------------------------------------------------------

/// Dispatches a SSSE3 decode function, falling back to scalar for the tail.
///
/// The SIMD engine processes as many full 16-byte (or DS64) blocks as possible,
/// returning `(consumed, written)`. This helper calls the scalar fallback for
/// any remaining bytes.
// ---------------------------------------------------------------------------
// Decode engine — all functions require `target_feature(enable = "ssse3")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod decode_engine {
    use super::*;

    /// SIMD lookup tables used by the Turbo-Base64 mapping and validation
    /// pipeline. All seven vectors are held in XMM registers for the duration
    /// of the hot loop — changing the number of fields here directly affects
    /// register pressure (currently 7 of 16 XMM regs).
    struct DecodeTables {
        /// First lookup table for the delta-mapping hash.
        /// `delta_hash = pavgb(pshufb(delta_asso, iv), iv >> 3)`
        delta_asso: __m128i,
        /// Second lookup table: `mapped = pshufb(delta_values, delta_hash) + iv`
        /// converts ASCII Base64 to 6-bit values.
        delta_values: __m128i,
        /// First lookup table for the check (validity) hash. Same structure as
        /// the delta hash but produces a vector where sign-bit-set lanes
        /// indicate invalid input bytes.
        check_asso: __m128i,
        /// Second lookup table for the check hash.
        check_values: __m128i,
        /// Cross-pack vector: `pshufb(packed, cpv)` compacts four 32-bit lanes
        /// (each holding 3 decoded bytes in the low 24 bits) into 12 contiguous
        /// output bytes.
        cpv: __m128i,
        /// First multiply constant: `pmaddubsw(mapped, 0x01400140)` merges
        /// adjacent 6-bit pairs into 12-bit values.
        madd_mul_1: __m128i,
        /// Second multiply constant: `pmaddwd(merged, 0x00011000)` merges
        /// 12-bit pairs into 24-bit output values in each 32-bit lane.
        madd_mul_2: __m128i,
    }

    impl DecodeTables {
        /// Initialise all seven SIMD constants.
        ///
        /// # Safety
        ///
        /// Caller must ensure the SSSE3 feature is available.
        #[inline]
        #[target_feature(enable = "ssse3")]
        unsafe fn new() -> Self {
            Self {
                delta_asso: _mm_setr_epi8(
                    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x0f, 0x00, 0x0f,
                ),
                delta_values: _mm_setr_epi8(
                    0x00,
                    0x00,
                    0x00,
                    0x13,
                    0x04,
                    b2i(0xbf),
                    b2i(0xbf),
                    b2i(0xb9),
                    b2i(0xb9),
                    0x00,
                    0x10,
                    b2i(0xc3),
                    b2i(0xbf),
                    b2i(0xbf),
                    b2i(0xb9),
                    b2i(0xb9),
                ),
                check_asso: _mm_setr_epi8(
                    0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03, 0x07, 0x0b,
                    0x0b, 0x0b, 0x0f,
                ),
                check_values: _mm_setr_epi8(
                    b2i(0x80),
                    b2i(0x80),
                    b2i(0x80),
                    b2i(0x80),
                    b2i(0xcf),
                    b2i(0xbf),
                    b2i(0xd5),
                    b2i(0xa6),
                    b2i(0xb5),
                    b2i(0x86),
                    b2i(0xd1),
                    b2i(0x80),
                    b2i(0xb1),
                    b2i(0x80),
                    b2i(0x91),
                    b2i(0x80),
                ),
                cpv: _mm_set_epi8(-1, -1, -1, -1, 12, 13, 14, 8, 9, 10, 4, 5, 6, 0, 1, 2),
                madd_mul_1: _mm_set1_epi32(0x01400140),
                madd_mul_2: _mm_set1_epi32(0x00011000),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Core SIMD helpers
    // -----------------------------------------------------------------------

    /// Map a 16-byte Base64 input vector to 12 decoded bytes and compute the
    /// `shifted = iv >> 3` value needed by the check stage.
    ///
    /// Returns `(decoded_12B, shifted)`.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn map_and_pack(iv: __m128i, t: &DecodeTables) -> (__m128i, __m128i) {
        // SAFETY: all intrinsics require SSSE3, guaranteed by target_feature.
        let shifted = _mm_srli_epi32(iv, 3);
        let delta_hash = _mm_avg_epu8(_mm_shuffle_epi8(t.delta_asso, iv), shifted);
        let mut ov = _mm_add_epi8(_mm_shuffle_epi8(t.delta_values, delta_hash), iv);
        let merge_ab_bc = _mm_maddubs_epi16(ov, t.madd_mul_1);
        ov = _mm_madd_epi16(merge_ab_bc, t.madd_mul_2);
        ov = _mm_shuffle_epi8(ov, t.cpv);
        (ov, shifted)
    }

    /// Like [`map_and_pack`] but accepts a pre-computed `shifted` value.
    ///
    /// Used in the strict path where `shifted` is computed earlier for
    /// validation and reused for mapping, avoiding a redundant `psrld`.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn map_and_pack_with_shifted(
        iv: __m128i,
        shifted: __m128i,
        t: &DecodeTables,
    ) -> __m128i {
        // SAFETY: all intrinsics require SSSE3, guaranteed by target_feature.
        let delta_hash = _mm_avg_epu8(_mm_shuffle_epi8(t.delta_asso, iv), shifted);
        let mut ov = _mm_add_epi8(_mm_shuffle_epi8(t.delta_values, delta_hash), iv);
        let merge_ab_bc = _mm_maddubs_epi16(ov, t.madd_mul_1);
        ov = _mm_madd_epi16(merge_ab_bc, t.madd_mul_2);
        _mm_shuffle_epi8(ov, t.cpv)
    }

    /// Validate one input vector. Returns a vector where lanes with the sign
    /// bit set indicate invalid Base64 bytes.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_vec(iv: __m128i, shifted: __m128i, t: &DecodeTables) -> __m128i {
        // SAFETY: all intrinsics require SSSE3, guaranteed by target_feature.
        let check_hash = _mm_avg_epu8(_mm_shuffle_epi8(t.check_asso, iv), shifted);
        _mm_adds_epi8(_mm_shuffle_epi8(t.check_values, check_hash), iv)
    }

    /// OR the check result of `iv` into `error_mask` (non-strict accumulator).
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn accumulate_check(
        iv: __m128i,
        shifted: __m128i,
        t: &DecodeTables,
        error_mask: __m128i,
    ) -> __m128i {
        // SAFETY: all intrinsics require SSSE3, guaranteed by target_feature.
        _mm_or_si128(error_mask, check_vec(iv, shifted, t))
    }

    /// Validate one vector and return the pmovmskb result (nonzero = invalid).
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_mask_bits(iv: __m128i, shifted: __m128i, t: &DecodeTables) -> i32 {
        // SAFETY: all intrinsics require SSSE3, guaranteed by target_feature.
        _mm_movemask_epi8(check_vec(iv, shifted, t))
    }

    /// Validate a pair of vectors and return the combined pmovmskb result.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn check_mask_bits_pair(
        iv0: __m128i,
        shifted0: __m128i,
        iv1: __m128i,
        shifted1: __m128i,
        t: &DecodeTables,
    ) -> i32 {
        // SAFETY: all intrinsics require SSSE3, guaranteed by target_feature.
        let chk0 = check_vec(iv0, shifted0, t);
        let chk1 = check_vec(iv1, shifted1, t);
        _mm_movemask_epi8(_mm_or_si128(chk0, chk1))
    }

    // -----------------------------------------------------------------------
    // DS64 block processors (64 input bytes -> 48 output bytes)
    // -----------------------------------------------------------------------

    /// Process one DS64 block in non-strict (CHECK0) mode.
    ///
    /// Decodes four 16-byte vectors but only validates the *first* vector of
    /// the block, matching Turbo-Base64's default `CHECK0` behaviour.
    /// The `iu0`/`iu1` registers are "forwarded" — on entry they hold the
    /// first two vectors (pre-loaded by the caller or previous iteration),
    /// and on exit they hold the first two vectors of the *next* block.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_ds64_partial(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut __m128i,
        iu1: &mut __m128i,
        t: &DecodeTables,
        error_mask: &mut __m128i,
    ) {
        // SAFETY: caller guarantees in_ptr..in_ptr+96 and out_ptr..out_ptr+48
        // are valid, and SSSE3 is available.
        let iv0 =
            _mm_loadu_si128(in_ptr.add(2 * verify_model::TAIL16_INPUT_BYTES) as *const __m128i);
        let iv1 =
            _mm_loadu_si128(in_ptr.add(3 * verify_model::TAIL16_INPUT_BYTES) as *const __m128i);

        let (ou0, shifted0) = map_and_pack(*iu0, t);
        let (ou1, _shifted1) = map_and_pack(*iu1, t);

        _mm_storeu_si128(out_ptr as *mut __m128i, ou0);
        _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, ou1);

        // CHECK0: only the first lane is validated per DS64 block.
        *error_mask = accumulate_check(*iu0, shifted0, t, *error_mask);

        *iu0 = _mm_loadu_si128(in_ptr.add(verify_model::DS64_INPUT_BYTES) as *const __m128i);
        *iu1 = _mm_loadu_si128(
            in_ptr.add(verify_model::DS64_INPUT_BYTES + verify_model::TAIL16_INPUT_BYTES)
                as *const __m128i,
        );

        let (ov2, _shifted2) = map_and_pack(iv0, t);
        let (ov3, _shifted3) = map_and_pack(iv1, t);

        _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, ov2);
        _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, ov3);
    }

    /// Process one DS64 block in strict (CHECK1) mode.
    ///
    /// Decodes and validates all four 16-byte vectors. Uses pre-computed
    /// `shifted` values (shared between map and check) to avoid redundant
    /// `psrld` instructions. Error bits are accumulated via OR into
    /// `error_bits` (a scalar i32, not a vector — uses `pmovmskb` pairs).
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn process_ds64_strict(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut __m128i,
        iu1: &mut __m128i,
        t: &DecodeTables,
        error_bits: &mut i32,
    ) {
        // SAFETY: caller guarantees in_ptr..in_ptr+96 and out_ptr..out_ptr+48
        // are valid, and SSSE3 is available.

        // --- First pair (pre-loaded iu0, iu1) ---
        let shifted0 = _mm_srli_epi32(*iu0, 3);
        let shifted1 = _mm_srli_epi32(*iu1, 3);
        let ou0 = map_and_pack_with_shifted(*iu0, shifted0, t);
        let ou1 = map_and_pack_with_shifted(*iu1, shifted1, t);

        _mm_storeu_si128(out_ptr as *mut __m128i, ou0);
        _mm_storeu_si128(out_ptr.add(12) as *mut __m128i, ou1);

        let m01 = check_mask_bits_pair(*iu0, shifted0, *iu1, shifted1, t);

        // --- Second pair (load from in_ptr+32, in_ptr+48) ---
        let iv0 =
            _mm_loadu_si128(in_ptr.add(2 * verify_model::TAIL16_INPUT_BYTES) as *const __m128i);
        let iv1 =
            _mm_loadu_si128(in_ptr.add(3 * verify_model::TAIL16_INPUT_BYTES) as *const __m128i);
        let shifted2 = _mm_srli_epi32(iv0, 3);
        let shifted3 = _mm_srli_epi32(iv1, 3);
        let ov2 = map_and_pack_with_shifted(iv0, shifted2, t);
        let ov3 = map_and_pack_with_shifted(iv1, shifted3, t);

        _mm_storeu_si128(out_ptr.add(24) as *mut __m128i, ov2);
        _mm_storeu_si128(out_ptr.add(36) as *mut __m128i, ov3);

        let m23 = check_mask_bits_pair(iv0, shifted2, iv1, shifted3, t);

        // --- Forward-load next iteration's first pair ---
        *iu0 = _mm_loadu_si128(in_ptr.add(verify_model::DS64_INPUT_BYTES) as *const __m128i);
        *iu1 = _mm_loadu_si128(
            in_ptr.add(verify_model::DS64_INPUT_BYTES + verify_model::TAIL16_INPUT_BYTES)
                as *const __m128i,
        );

        *error_bits |= m01 | m23;
    }

    // -----------------------------------------------------------------------
    // Scalar alignment preamble (shared by both decoders)
    // -----------------------------------------------------------------------

    /// Decode 4-byte groups one at a time until `out_ptr` is 16-byte aligned.
    ///
    /// Returns `Some((in_ptr, out_ptr))` on success, or `None` if an invalid
    /// character is encountered.
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn align_output(
        mut in_ptr: *const u8,
        mut out_ptr: *mut u8,
        in_end: *const u8,
    ) -> Option<(*const u8, *mut u8)> {
        while (out_ptr as usize) & 15 != 0 {
            if !can_read(in_ptr, in_end, 4) {
                return Some((in_ptr, out_ptr));
            }
            // Use a stack temporary for the output to avoid forming a
            // `&mut [u8; 3]` reference that may extend past the valid
            // output allocation (e.g. when only 1 or 2 bytes remain).
            let mut tmp = [0u8; 3];
            let in4 = &*(in_ptr as *const [u8; 4]);
            let u = crate::engine::scalar::decode_tail_3(in4, &mut tmp);
            match u {
                Some((written, cu)) => {
                    if cu == u32::MAX {
                        return None;
                    }
                    // SAFETY: written is 1, 2, or 3 and the caller
                    // guarantees out_ptr has at least `decoded_len`
                    // bytes remaining, which is >= written.
                    core::ptr::copy_nonoverlapping(tmp.as_ptr(), out_ptr, written);
                    in_ptr = in_ptr.add(4);
                    out_ptr = out_ptr.add(written);
                    if written < 3 {
                        return Some((in_ptr, out_ptr));
                    }
                }
                None => return None,
            }
        }
        Some((in_ptr, out_ptr))
    }

    // -----------------------------------------------------------------------
    // Public entry points
    // -----------------------------------------------------------------------

    /// Non-strict SSSE3 decode (CHECK0 mode).
    ///
    /// # Safety
    ///
    /// Caller must ensure SSSE3 is available.
    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_ssse3(in_data: &[u8], out_data: &mut [u8]) -> Option<(usize, usize)> {
        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let in_end = in_base.add(in_data.len());
        let out_end = out_base.add(out_data.len());

        // SAFETY: align_output only dereferences within the input/output slices.
        let (mut in_ptr, mut out_ptr) = align_output(in_base, out_base, in_end)?;

        let safe_end = safe_in_end_4(in_data);

        if !can_advance(
            in_ptr,
            safe_end,
            verify_model::SIMD_PRELOAD_BYTES,
            out_ptr,
            out_end,
            verify_model::STORE_WIDTH_BYTES,
        ) {
            return Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base));
        }

        // SAFETY: DecodeTables::new() requires SSSE3, guaranteed by target_feature.
        let t = DecodeTables::new();
        let mut error_mask = _mm_setzero_si128();

        if can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
            let mut iu0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let mut iu1 =
                _mm_loadu_si128(in_ptr.add(verify_model::TAIL16_INPUT_BYTES) as *const __m128i);

            while can_process_ds64_double(in_ptr, safe_end, out_ptr, out_end) {
                process_ds64_partial(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_mask);
                process_ds64_partial(
                    in_ptr.add(verify_model::DS64_INPUT_BYTES),
                    out_ptr.add(verify_model::DS64_OUTPUT_BYTES),
                    &mut iu0,
                    &mut iu1,
                    &t,
                    &mut error_mask,
                );
                in_ptr = in_ptr.add(2 * verify_model::DS64_INPUT_BYTES);
                out_ptr = out_ptr.add(2 * verify_model::DS64_OUTPUT_BYTES);
            }

            while can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
                process_ds64_partial(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_mask);
                in_ptr = in_ptr.add(verify_model::DS64_INPUT_BYTES);
                out_ptr = out_ptr.add(verify_model::DS64_OUTPUT_BYTES);
            }
        }

        // Single-vector tail loop
        while can_process_tail16(in_ptr, safe_end, out_ptr, out_end) {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);
            let (ov, shifted) = map_and_pack(iv, &t);
            _mm_storeu_si128(out_ptr as *mut __m128i, ov);
            error_mask = accumulate_check(iv, shifted, &t, error_mask);
            in_ptr = in_ptr.add(verify_model::TAIL16_INPUT_BYTES);
            out_ptr = out_ptr.add(verify_model::TAIL16_OUTPUT_BYTES);
        }

        if _mm_movemask_epi8(error_mask) != 0 {
            return None;
        }

        Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base))
    }

    /// Strict SSSE3 decode (CHECK1 mode — all vectors validated).
    ///
    /// # Safety
    ///
    /// Caller must ensure SSSE3 is available.
    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn decode_ssse3_strict(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let in_end = in_base.add(in_data.len());
        let out_end = out_base.add(out_data.len());

        // SAFETY: align_output only dereferences within the input/output slices.
        let (mut in_ptr, mut out_ptr) = align_output(in_base, out_base, in_end)?;

        let safe_end = safe_in_end_4(in_data);

        if !can_advance(
            in_ptr,
            safe_end,
            verify_model::SIMD_PRELOAD_BYTES,
            out_ptr,
            out_end,
            verify_model::STORE_WIDTH_BYTES,
        ) {
            return Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base));
        }

        // SAFETY: DecodeTables::new() requires SSSE3, guaranteed by target_feature.
        let t = DecodeTables::new();
        let mut error_bits = 0i32;

        if can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
            let mut iu0 = _mm_loadu_si128(in_ptr as *const __m128i);
            let mut iu1 =
                _mm_loadu_si128(in_ptr.add(verify_model::TAIL16_INPUT_BYTES) as *const __m128i);

            while can_process_ds64_double(in_ptr, safe_end, out_ptr, out_end) {
                process_ds64_strict(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_bits);
                process_ds64_strict(
                    in_ptr.add(verify_model::DS64_INPUT_BYTES),
                    out_ptr.add(verify_model::DS64_OUTPUT_BYTES),
                    &mut iu0,
                    &mut iu1,
                    &t,
                    &mut error_bits,
                );
                in_ptr = in_ptr.add(2 * verify_model::DS64_INPUT_BYTES);
                out_ptr = out_ptr.add(2 * verify_model::DS64_OUTPUT_BYTES);
            }

            while can_process_ds64(in_ptr, safe_end, out_ptr, out_end) {
                process_ds64_strict(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_bits);
                in_ptr = in_ptr.add(verify_model::DS64_INPUT_BYTES);
                out_ptr = out_ptr.add(verify_model::DS64_OUTPUT_BYTES);
            }
        }

        // Single-vector tail loop
        while can_process_tail16(in_ptr, safe_end, out_ptr, out_end) {
            let iv = _mm_loadu_si128(in_ptr as *const __m128i);
            let (ov, shifted) = map_and_pack(iv, &t);
            _mm_storeu_si128(out_ptr as *mut __m128i, ov);
            error_bits |= check_mask_bits(iv, shifted, &t);
            in_ptr = in_ptr.add(verify_model::TAIL16_INPUT_BYTES);
            out_ptr = out_ptr.add(verify_model::TAIL16_OUTPUT_BYTES);
        }

        if error_bits != 0 {
            return None;
        }

        Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base))
    }
}

// ---------------------------------------------------------------------------
// Encode engine — all functions require `target_feature(enable = "ssse3")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod encode_engine {
    use super::*;

    /// Vectorised unsigned 16-bit multiply for two register pairs.
    ///
    /// Computes `(hi_part | lo_part)` for each pair, where `hi_part` and
    /// `lo_part` are extracted via the provided masks. This is the core of the
    /// Turbo-Base64 6-bit-to-ASCII mapping's bit-field extraction step.
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

    /// Encode a pair of 12-byte input blocks into two 16-byte Base64 vectors.
    ///
    /// Each block is shuffled to spread 3 input bytes across 4 output lanes,
    /// then the 6-bit fields are extracted via [`fast_pmul_x2`] and mapped to
    /// ASCII via a pshufb offset table.
    #[allow(clippy::too_many_arguments)]
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

    /// SSSE3 Base64 encoder.
    ///
    /// Processes input in 96-byte (8x12) blocks, producing 128-byte (8x16)
    /// output blocks, then drains 48-byte and 12-byte tails. Returns
    /// `(consumed, written)` byte counts; the caller handles any remaining
    /// bytes with the scalar fallback.
    ///
    /// # Safety
    ///
    /// Caller must ensure SSSE3 is available.
    #[inline]
    #[target_feature(enable = "ssse3")]
    pub unsafe fn encode_base64_ssse3(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());

        if (out_ptr as usize) & 3 != 0 {
            return (0, 0);
        }

        while (out_ptr as usize) & 15 != 0 {
            if remaining(in_ptr, in_end) < 3 {
                return (
                    in_ptr as usize - in_data.as_ptr() as usize,
                    out_ptr as usize - out_data.as_ptr() as usize,
                );
            }
            let a = *in_ptr;
            let b = *in_ptr.add(1);
            let c = *in_ptr.add(2);
            crate::engine::scalar::encode_block_3_to_4_ptr(a, b, c, out_ptr);
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
        let mask1 = _mm_set1_epi32(w2i(0x0fc0fc00));
        let mulhi = _mm_set1_epi32(w2i(0x04000040));
        let mask2 = _mm_set1_epi32(w2i(0x003f03f0));
        let mullo = _mm_set1_epi32(w2i(0x01000010));
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
