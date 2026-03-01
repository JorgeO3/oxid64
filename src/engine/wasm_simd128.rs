//! WASM SIMD128 Base64 codec — port of the SSSE3 Turbo-Base64 engine.
//!
//! This module provides a high-performance WASM SIMD128 Base64 decoder and
//! encoder ported from the SSSE3 engine, which is itself derived from
//! [Turbo-Base64](https://github.com/powturbo/Turbo-Base64).
//!
//! # Design goals
//!
//! - **Zero-alloc / `core`-only**: uses no allocator and depends only on
//!   `core`, making it suitable for minimal `.wasm` bundles.
//! - **Minimal bundle size**: no alignment preambles (WASM loads/stores have
//!   no alignment requirements), no runtime feature detection (the binary is
//!   compiled with or without SIMD128).
//!
//! # Decoder
//!
//! [`WasmSimd128Decoder`] supports two validation modes controlled by
//! [`DecodeOpts`]:
//!
//! - **Non-strict** (`strict: false`). Validates only the first of every four
//!   16-byte vectors per DS64 block (`CHECK0` mode).
//! - **Strict** (`strict: true`, default). Validates all four vectors per
//!   DS64 block (`CHECK1` mode).
//!
//! # `pmaddubsw` emulation
//!
//! WASM SIMD128 lacks a direct equivalent of the SSSE3 `pmaddubsw`
//! instruction. When compiled with `target_feature = "relaxed-simd"`,
//! `i16x8_relaxed_dot_i8x16_i7x16` is used (single instruction). Otherwise,
//! a manual 5-op emulation using extend + multiply + add is used.
//!
//! # Encoder
//!
//! [`WasmSimd128Decoder::encode_to_slice`] encodes raw bytes to Base64 ASCII
//! using WASM SIMD128 vectorised bit-manipulation, falling back to scalar for
//! the tail. The encoder is independent of [`DecodeOpts`].

use super::scalar::{decoded_len_strict, encode_base64_fast};
use super::{Base64Decoder, DecodeOpts, b2i};

use core::arch::wasm32::*;

/// WASM SIMD128 Base64 decoder and encoder.
///
/// The decoder validation mode is controlled by [`DecodeOpts`]:
///
/// - `WasmSimd128Decoder::new()` — strict mode (default, all vectors validated).
/// - `WasmSimd128Decoder::with_opts(opts)` — custom configuration.
///
/// The encoder ([`encode_to_slice`](Self::encode_to_slice)) does not depend
/// on decoder options.
pub struct WasmSimd128Decoder {
    opts: DecodeOpts,
}

impl WasmSimd128Decoder {
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
    /// Returns `None` if the input contains invalid Base64 characters.
    #[inline]
    pub fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        let out_len = decoded_len_strict(input)?;
        if out.len() < out_len {
            return None;
        }

        let engine_fn = if self.opts.strict {
            decode_engine::decode_wasm_strict
                as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
        } else {
            decode_engine::decode_wasm as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
        };
        super::dispatch_decode(input, out, engine_fn)
    }

    /// Encode raw bytes to Base64 ASCII, returning the number of bytes written.
    #[inline]
    pub fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        let needed = input.len().div_ceil(3) * 4;
        assert!(
            out.len() >= needed,
            "encode_to_slice output too small: need {}, have {}",
            needed,
            out.len()
        );

        let (consumed, mut written) = unsafe {
            // SAFETY: WASM SIMD128 is a compile-time feature; availability is
            // guaranteed by #[target_feature(enable = "simd128")] on the callee.
            encode_engine::encode_base64_wasm(input, out)
        };

        if consumed < input.len() {
            let tail_written = encode_base64_fast(&input[consumed..], &mut out[written..]);
            written += tail_written;
        }
        written
    }
}

impl Default for WasmSimd128Decoder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Decoder for WasmSimd128Decoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        WasmSimd128Decoder::decode_to_slice(self, input, out)
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        WasmSimd128Decoder::encode_to_slice(self, input, out)
    }
}

// ---------------------------------------------------------------------------
// Shared decode dispatch
// ---------------------------------------------------------------------------

/// Dispatches the WASM SIMD decode function, falling back to scalar for the tail.
// ---------------------------------------------------------------------------
// Decode engine — all functions require `target_feature(enable = "simd128")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod decode_engine {
    use super::*;

    // -----------------------------------------------------------------------
    // pmaddubsw emulation
    // -----------------------------------------------------------------------

    /// Emulate SSSE3 `_mm_maddubs_epi16(a, b)` using relaxed-simd.
    ///
    /// `i16x8_relaxed_dot_i8x16_i7x16(a, b)` multiplies unsigned bytes from
    /// `a` by signed bytes from `b` (which must be in [-128, 127] but the
    /// "i7x16" name hints the second operand should be in [-64, 63] for
    /// guaranteed determinism — our constant `0x01400140` has bytes 1 and 64,
    /// both in [0, 127], so this is safe).
    #[cfg(target_feature = "relaxed-simd")]
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn maddubs(a: v128, b: v128) -> v128 {
        i16x8_relaxed_dot_i8x16_i7x16(a, b)
    }

    /// Emulate SSSE3 `_mm_maddubs_epi16(a, b)` using standard SIMD128 ops.
    ///
    /// Multiplies unsigned 8-bit lanes of `a` by signed 8-bit lanes of `b`,
    /// then horizontally adds adjacent 16-bit products.
    #[cfg(not(target_feature = "relaxed-simd"))]
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn maddubs(a: v128, b: v128) -> v128 {
        // Even lanes (0, 2, 4, ...): multiply and keep in 16-bit
        let mask_even = i8x16(-1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0);

        // Extract even bytes of a (unsigned) and b (signed) into 16-bit lanes
        let a_even = v128_and(a, mask_even); // zero odd bytes
        let b_even = v128_and(b, mask_even);
        let prod_even = i16x8_mul(a_even, b_even); // u8*i8 fits in i16 since a_even <= 255, b_even is already zero-extended in high byte

        // Extract odd bytes by shifting right 8 bits per 16-bit lane
        let a_odd = u16x8_shr(a, 8);
        // For signed odd bytes, arithmetic shift right
        let b_odd = i16x8_shr(b, 8);
        let prod_odd = i16x8_mul(a_odd, b_odd);

        // Sum even and odd products
        i16x8_add(prod_even, prod_odd)
    }

    /// SIMD lookup tables used by the Turbo-Base64 mapping and validation
    /// pipeline. Mirrors the SSSE3 `DecodeTables` using WASM `v128` type.
    struct DecodeTables {
        delta_asso: v128,
        delta_values: v128,
        check_asso: v128,
        check_values: v128,
        cpv: v128,
        madd_mul_1: v128,
        madd_mul_2: v128,
    }

    impl DecodeTables {
        #[inline]
        #[target_feature(enable = "simd128")]
        unsafe fn new() -> Self {
            Self {
                // _mm_setr_epi8 order (element 0 first) matches i8x16 parameter order
                delta_asso: i8x16(
                    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x0f, 0x00, 0x0f,
                ),
                delta_values: i8x16(
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
                check_asso: i8x16(
                    0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03, 0x07, 0x0b,
                    0x0b, 0x0b, 0x0f,
                ),
                check_values: i8x16(
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
                // _mm_set_epi8 is reverse order: _mm_set_epi8(15,14,...,1,0)
                // cpv was: _mm_set_epi8(-1,-1,-1,-1, 12,13,14, 8,9,10, 4,5,6, 0,1,2)
                // In setr order (i8x16): byte 0 = 2, byte 1 = 1, ..., byte 15 = -1
                cpv: i8x16(2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1),
                madd_mul_1: u32x4_splat(0x01400140),
                madd_mul_2: u32x4_splat(0x00011000),
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
    #[target_feature(enable = "simd128")]
    unsafe fn map_and_pack(iv: v128, t: &DecodeTables) -> (v128, v128) {
        let shifted = u32x4_shr(iv, 3);
        let delta_hash = u8x16_avgr(i8x16_swizzle(t.delta_asso, iv), shifted);
        let mut ov = i8x16_add(i8x16_swizzle(t.delta_values, delta_hash), iv);
        let merge_ab_bc = maddubs(ov, t.madd_mul_1);
        ov = i32x4_dot_i16x8(merge_ab_bc, t.madd_mul_2);
        ov = i8x16_swizzle(ov, t.cpv);
        (ov, shifted)
    }

    /// Like [`map_and_pack`] but accepts a pre-computed `shifted` value.
    ///
    /// Used in the strict path where `shifted` is computed earlier for
    /// validation and reused for mapping, avoiding a redundant shift.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn map_and_pack_with_shifted(iv: v128, shifted: v128, t: &DecodeTables) -> v128 {
        let delta_hash = u8x16_avgr(i8x16_swizzle(t.delta_asso, iv), shifted);
        let mut ov = i8x16_add(i8x16_swizzle(t.delta_values, delta_hash), iv);
        let merge_ab_bc = maddubs(ov, t.madd_mul_1);
        ov = i32x4_dot_i16x8(merge_ab_bc, t.madd_mul_2);
        i8x16_swizzle(ov, t.cpv)
    }

    /// Validate one input vector. Returns a vector where lanes with the sign
    /// bit set indicate invalid Base64 bytes.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn check_vec(iv: v128, shifted: v128, t: &DecodeTables) -> v128 {
        let check_hash = u8x16_avgr(i8x16_swizzle(t.check_asso, iv), shifted);
        i8x16_add_sat(i8x16_swizzle(t.check_values, check_hash), iv)
    }

    /// OR the check result of `iv` into `error_mask` (non-strict accumulator).
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn accumulate_check(
        iv: v128,
        shifted: v128,
        t: &DecodeTables,
        error_mask: v128,
    ) -> v128 {
        v128_or(error_mask, check_vec(iv, shifted, t))
    }

    /// Validate one vector and return the bitmask result (nonzero = invalid).
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn check_mask_bits(iv: v128, shifted: v128, t: &DecodeTables) -> u16 {
        i8x16_bitmask(check_vec(iv, shifted, t))
    }

    /// Validate a pair of vectors and return the combined bitmask result.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn check_mask_bits_pair(
        iv0: v128,
        shifted0: v128,
        iv1: v128,
        shifted1: v128,
        t: &DecodeTables,
    ) -> u16 {
        let chk0 = check_vec(iv0, shifted0, t);
        let chk1 = check_vec(iv1, shifted1, t);
        i8x16_bitmask(v128_or(chk0, chk1))
    }

    // -----------------------------------------------------------------------
    // DS64 block processors (64 input bytes -> 48 output bytes)
    // -----------------------------------------------------------------------

    /// Process one DS64 block in non-strict (CHECK0) mode.
    ///
    /// Decodes four 16-byte vectors but only validates the *first* vector,
    /// matching Turbo-Base64's default `CHECK0` behaviour.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn process_ds64_partial(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut v128,
        iu1: &mut v128,
        t: &DecodeTables,
        error_mask: &mut v128,
    ) {
        let iv0 = v128_load(in_ptr.add(32) as *const v128);
        let iv1 = v128_load(in_ptr.add(48) as *const v128);

        let (ou0, shifted0) = map_and_pack(*iu0, t);
        let (ou1, _shifted1) = map_and_pack(*iu1, t);

        v128_store(out_ptr as *mut v128, ou0);
        v128_store(out_ptr.add(12) as *mut v128, ou1);

        // CHECK0: only the first lane is validated per DS64 block.
        *error_mask = accumulate_check(*iu0, shifted0, t, *error_mask);

        *iu0 = v128_load(in_ptr.add(64) as *const v128);
        *iu1 = v128_load(in_ptr.add(80) as *const v128);

        let (ov2, _shifted2) = map_and_pack(iv0, t);
        let (ov3, _shifted3) = map_and_pack(iv1, t);

        v128_store(out_ptr.add(24) as *mut v128, ov2);
        v128_store(out_ptr.add(36) as *mut v128, ov3);
    }

    /// Process one DS64 block in strict (CHECK1) mode.
    ///
    /// Decodes and validates all four 16-byte vectors. Uses pre-computed
    /// `shifted` values (shared between map and check) to avoid redundant
    /// shift instructions.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn process_ds64_strict(
        in_ptr: *const u8,
        out_ptr: *mut u8,
        iu0: &mut v128,
        iu1: &mut v128,
        t: &DecodeTables,
        error_bits: &mut u16,
    ) {
        // --- First pair (pre-loaded iu0, iu1) ---
        let shifted0 = u32x4_shr(*iu0, 3);
        let shifted1 = u32x4_shr(*iu1, 3);
        let ou0 = map_and_pack_with_shifted(*iu0, shifted0, t);
        let ou1 = map_and_pack_with_shifted(*iu1, shifted1, t);

        v128_store(out_ptr as *mut v128, ou0);
        v128_store(out_ptr.add(12) as *mut v128, ou1);

        let m01 = check_mask_bits_pair(*iu0, shifted0, *iu1, shifted1, t);

        // --- Second pair (load from in_ptr+32, in_ptr+48) ---
        let iv0 = v128_load(in_ptr.add(32) as *const v128);
        let iv1 = v128_load(in_ptr.add(48) as *const v128);
        let shifted2 = u32x4_shr(iv0, 3);
        let shifted3 = u32x4_shr(iv1, 3);
        let ov2 = map_and_pack_with_shifted(iv0, shifted2, t);
        let ov3 = map_and_pack_with_shifted(iv1, shifted3, t);

        v128_store(out_ptr.add(24) as *mut v128, ov2);
        v128_store(out_ptr.add(36) as *mut v128, ov3);

        let m23 = check_mask_bits_pair(iv0, shifted2, iv1, shifted3, t);

        // --- Forward-load next iteration's first pair ---
        *iu0 = v128_load(in_ptr.add(64) as *const v128);
        *iu1 = v128_load(in_ptr.add(80) as *const v128);

        *error_bits |= m01 | m23;
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Compute the safe end pointer: last position where we can read 4 bytes
    /// without going out of bounds.
    #[inline]
    fn safe_in_end(in_data: &[u8]) -> *const u8 {
        if in_data.len() >= 4 {
            // SAFETY: pointer arithmetic within the allocated object.
            unsafe { in_data.as_ptr().add(in_data.len() - 4) }
        } else {
            in_data.as_ptr()
        }
    }

    // -----------------------------------------------------------------------
    // Public entry points
    // -----------------------------------------------------------------------

    /// Non-strict WASM SIMD128 decode (CHECK0 mode).
    ///
    /// # Safety
    ///
    /// Caller must ensure SIMD128 is available (compile-time guarantee for
    /// `wasm32` targets built with `+simd128`).
    #[inline]
    #[target_feature(enable = "simd128")]
    pub unsafe fn decode_wasm(in_data: &[u8], out_data: &mut [u8]) -> Option<(usize, usize)> {
        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let out_end = out_base.add(out_data.len());

        let mut in_ptr = in_base;
        let mut out_ptr = out_base;

        let safe_end = safe_in_end(in_data);

        // No alignment preamble needed — WASM loads/stores have no alignment
        // requirements and are always efficient at any offset.

        if (safe_end as usize).saturating_sub(in_ptr as usize) < 32
            || (out_end as usize).saturating_sub(out_ptr as usize) < 16
        {
            return Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base));
        }

        let t = DecodeTables::new();
        let mut error_mask = i8x16_splat(0);

        // Double-DS64 unrolled main loop (128 input bytes -> 96 output bytes)
        if in_ptr as usize + 32 + 64 <= safe_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let mut iu0 = v128_load(in_ptr as *const v128);
            let mut iu1 = v128_load(in_ptr.add(16) as *const v128);

            while in_ptr as usize + 32 + 2 * 64 <= safe_end as usize
                && out_ptr as usize + 96 + 4 <= out_end as usize
            {
                process_ds64_partial(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_mask);
                process_ds64_partial(
                    in_ptr.add(64),
                    out_ptr.add(48),
                    &mut iu0,
                    &mut iu1,
                    &t,
                    &mut error_mask,
                );
                in_ptr = in_ptr.add(128);
                out_ptr = out_ptr.add(96);
            }

            // Single-DS64 drain
            while in_ptr as usize + 32 + 64 <= safe_end as usize
                && out_ptr as usize + 48 + 4 <= out_end as usize
            {
                process_ds64_partial(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_mask);
                in_ptr = in_ptr.add(64);
                out_ptr = out_ptr.add(48);
            }
        }

        // Single-vector tail loop
        while in_ptr as usize + 16 <= safe_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = v128_load(in_ptr as *const v128);
            let (ov, shifted) = map_and_pack(iv, &t);
            v128_store(out_ptr as *mut v128, ov);
            error_mask = accumulate_check(iv, shifted, &t, error_mask);
            in_ptr = in_ptr.add(16);
            out_ptr = out_ptr.add(12);
        }

        if i8x16_bitmask(error_mask) != 0 {
            return None;
        }

        Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base))
    }

    /// Strict WASM SIMD128 decode (CHECK1 mode — all vectors validated).
    ///
    /// # Safety
    ///
    /// Caller must ensure SIMD128 is available.
    #[inline]
    #[target_feature(enable = "simd128")]
    pub unsafe fn decode_wasm_strict(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let out_end = out_base.add(out_data.len());

        let mut in_ptr = in_base;
        let mut out_ptr = out_base;

        let safe_end = safe_in_end(in_data);

        if (safe_end as usize).saturating_sub(in_ptr as usize) < 32
            || (out_end as usize).saturating_sub(out_ptr as usize) < 16
        {
            return Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base));
        }

        let t = DecodeTables::new();
        let mut error_bits = 0u16;

        // Double-DS64 unrolled main loop
        if in_ptr as usize + 32 + 64 <= safe_end as usize
            && out_ptr as usize + 48 + 4 <= out_end as usize
        {
            let mut iu0 = v128_load(in_ptr as *const v128);
            let mut iu1 = v128_load(in_ptr.add(16) as *const v128);

            while in_ptr as usize + 32 + 2 * 64 <= safe_end as usize
                && out_ptr as usize + 96 + 4 <= out_end as usize
            {
                process_ds64_strict(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_bits);
                process_ds64_strict(
                    in_ptr.add(64),
                    out_ptr.add(48),
                    &mut iu0,
                    &mut iu1,
                    &t,
                    &mut error_bits,
                );
                in_ptr = in_ptr.add(128);
                out_ptr = out_ptr.add(96);
            }

            // Single-DS64 drain
            while in_ptr as usize + 32 + 64 <= safe_end as usize
                && out_ptr as usize + 48 + 4 <= out_end as usize
            {
                process_ds64_strict(in_ptr, out_ptr, &mut iu0, &mut iu1, &t, &mut error_bits);
                in_ptr = in_ptr.add(64);
                out_ptr = out_ptr.add(48);
            }
        }

        // Single-vector tail loop
        while in_ptr as usize + 16 <= safe_end as usize
            && out_ptr as usize + 12 + 4 <= out_end as usize
        {
            let iv = v128_load(in_ptr as *const v128);
            let (ov, shifted) = map_and_pack(iv, &t);
            v128_store(out_ptr as *mut v128, ov);
            error_bits |= check_mask_bits(iv, shifted, &t);
            in_ptr = in_ptr.add(16);
            out_ptr = out_ptr.add(12);
        }

        if error_bits != 0 {
            return None;
        }

        Some(crate::engine::offsets(in_ptr, out_ptr, in_base, out_base))
    }
}

// ---------------------------------------------------------------------------
// Encode engine — all functions require `target_feature(enable = "simd128")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod encode_engine {
    use super::*;

    // -----------------------------------------------------------------------
    // mulhi_epu16 emulation
    // -----------------------------------------------------------------------

    /// Emulate `_mm_mulhi_epu16(a, b)` — unsigned 16-bit multiply, keep high
    /// 16 bits of each 32-bit product.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn mulhi_epu16(a: v128, b: v128) -> v128 {
        // Widen to 32-bit, multiply, shift right 16, narrow back.
        let lo32 = u32x4_extmul_low_u16x8(a, b);
        let hi32 = u32x4_extmul_high_u16x8(a, b);
        let lo_shifted = u32x4_shr(lo32, 16);
        let hi_shifted = u32x4_shr(hi32, 16);
        // Interleave the 4 low results and 4 high results back into 8x u16.
        // lo_shifted has lanes [0,1,2,3] from the low half
        // hi_shifted has lanes [4,5,6,7] from the high half
        // We need to pack them back: use i8x16_shuffle to pick the low 16
        // bits of each 32-bit lane.
        //
        // lo_shifted: [r0_lo, r0_hi, r1_lo, r1_hi, r2_lo, r2_hi, r3_lo, r3_hi] (as u16)
        // hi_shifted: [r4_lo, r4_hi, r5_lo, r5_hi, r6_lo, r6_hi, r7_lo, r7_hi] (as u16)
        // We want bytes [0,1, 4,5, 8,9, 12,13] from lo_shifted
        // and bytes [0,1, 4,5, 8,9, 12,13] from hi_shifted (as lanes 16+)
        i8x16_shuffle::<0, 1, 4, 5, 8, 9, 12, 13, 16, 17, 20, 21, 24, 25, 28, 29>(
            lo_shifted, hi_shifted,
        )
    }

    /// Vectorised unsigned 16-bit multiply for two register pairs.
    ///
    /// Computes `(hi_part | lo_part)` for each pair, where `hi_part` and
    /// `lo_part` are extracted via the provided masks.
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn fast_pmul_x2(
        a0: v128,
        a1: v128,
        mulhi_c: v128,
        mullo_c: v128,
        mask1: v128,
        mask2: v128,
    ) -> (v128, v128) {
        let t0_0 = v128_and(a0, mask1);
        let t0_1 = v128_and(a1, mask1);
        let t1_0 = v128_and(a0, mask2);
        let t1_1 = v128_and(a1, mask2);

        let hi0 = mulhi_epu16(t0_0, mulhi_c);
        let hi1 = mulhi_epu16(t0_1, mulhi_c);
        let lo0 = i16x8_mul(t1_0, mullo_c);
        let lo1 = i16x8_mul(t1_1, mullo_c);

        (v128_or(hi0, lo0), v128_or(hi1, lo1))
    }

    /// Encode a pair of 12-byte input blocks into two 16-byte Base64 vectors.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    #[target_feature(enable = "simd128")]
    unsafe fn process_block_x2(
        mut v0: v128,
        mut v1: v128,
        shuf: v128,
        mulhi_c: v128,
        mullo_c: v128,
        mask1: v128,
        mask2: v128,
        offsets: v128,
        subs_val: v128,
        cmpgt_val: v128,
    ) -> (v128, v128) {
        v0 = i8x16_swizzle(v0, shuf);
        v1 = i8x16_swizzle(v1, shuf);

        let (res0, res1) = fast_pmul_x2(v0, v1, mulhi_c, mullo_c, mask1, mask2);
        v0 = res0;
        v1 = res1;

        let mut vidx_0 = u8x16_sub_sat(v0, subs_val);
        let mut vidx_1 = u8x16_sub_sat(v1, subs_val);
        vidx_0 = i8x16_sub(vidx_0, i8x16_gt(v0, cmpgt_val));
        vidx_1 = i8x16_sub(vidx_1, i8x16_gt(v1, cmpgt_val));

        v0 = i8x16_add(v0, i8x16_swizzle(offsets, vidx_0));
        v1 = i8x16_add(v1, i8x16_swizzle(offsets, vidx_1));

        (v0, v1)
    }

    /// WASM SIMD128 Base64 encoder.
    ///
    /// Processes input in 96-byte blocks, producing 128-byte output blocks,
    /// then drains 48-byte and 12-byte tails. Returns `(consumed, written)`;
    /// the caller handles remaining bytes with the scalar fallback.
    ///
    /// No alignment preamble — WASM loads/stores are always unaligned.
    ///
    /// # Safety
    ///
    /// Caller must ensure SIMD128 is available.
    #[inline]
    #[target_feature(enable = "simd128")]
    pub unsafe fn encode_base64_wasm(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let mut in_ptr = in_data.as_ptr();
        let mut out_ptr = out_data.as_mut_ptr();
        let in_end = in_ptr.add(in_data.len());

        // No alignment preamble needed for WASM.

        if (in_end as usize).saturating_sub(in_ptr as usize) < 52 {
            return (0, 0);
        }

        // _mm_set_epi8(10,11,9,10,7,8,6,7,4,5,3,4,1,2,0,1) in reverse order →
        // _mm_setr_epi8(1,0,2,1,4,3,5,4,7,6,8,7,10,9,11,10)
        let shuf = i8x16(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);
        let mask1 = u32x4_splat(0x0fc0fc00);
        let mulhi_c = u32x4_splat(0x04000040);
        let mask2 = u32x4_splat(0x003f03f0);
        let mullo_c = u32x4_splat(0x01000010);
        let offsets = i8x16(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        );
        let subs_val = i8x16_splat(51);
        let cmpgt_val = i8x16_splat(25);

        // Main loop: 96 input bytes → 128 output bytes (8 × 12→16 blocks, 4 pairs)
        while in_ptr as usize + 108 <= in_end as usize {
            let u0 = v128_load(in_ptr as *const v128);
            let u1 = v128_load(in_ptr.add(12) as *const v128);

            let (o0, o1) = process_block_x2(
                u0, u1, shuf, mulhi_c, mullo_c, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let w0 = v128_load(in_ptr.add(24) as *const v128);
            let w1 = v128_load(in_ptr.add(36) as *const v128);

            v128_store(out_ptr as *mut v128, o0);
            v128_store(out_ptr.add(16) as *mut v128, o1);

            let (o2, o3) = process_block_x2(
                w0, w1, shuf, mulhi_c, mullo_c, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let u2 = v128_load(in_ptr.add(48) as *const v128);
            let u3 = v128_load(in_ptr.add(60) as *const v128);

            v128_store(out_ptr.add(32) as *mut v128, o2);
            v128_store(out_ptr.add(48) as *mut v128, o3);

            let (o4, o5) = process_block_x2(
                u2, u3, shuf, mulhi_c, mullo_c, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let w2 = v128_load(in_ptr.add(72) as *const v128);
            let w3 = v128_load(in_ptr.add(84) as *const v128);

            v128_store(out_ptr.add(64) as *mut v128, o4);
            v128_store(out_ptr.add(80) as *mut v128, o5);

            let (o6, o7) = process_block_x2(
                w2, w3, shuf, mulhi_c, mullo_c, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            v128_store(out_ptr.add(96) as *mut v128, o6);
            v128_store(out_ptr.add(112) as *mut v128, o7);

            in_ptr = in_ptr.add(96);
            out_ptr = out_ptr.add(128);
        }

        // 48-byte drain: 48 input → 64 output (4 blocks, 2 pairs)
        while in_ptr as usize + 60 <= in_end as usize {
            let u0 = v128_load(in_ptr as *const v128);
            let u1 = v128_load(in_ptr.add(12) as *const v128);

            let (o0, o1) = process_block_x2(
                u0, u1, shuf, mulhi_c, mullo_c, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            let w0 = v128_load(in_ptr.add(24) as *const v128);
            let w1 = v128_load(in_ptr.add(36) as *const v128);

            v128_store(out_ptr as *mut v128, o0);
            v128_store(out_ptr.add(16) as *mut v128, o1);

            let (o2, o3) = process_block_x2(
                w0, w1, shuf, mulhi_c, mullo_c, mask1, mask2, offsets, subs_val, cmpgt_val,
            );

            v128_store(out_ptr.add(32) as *mut v128, o2);
            v128_store(out_ptr.add(48) as *mut v128, o3);

            in_ptr = in_ptr.add(48);
            out_ptr = out_ptr.add(64);
        }

        // 12-byte tail: one block at a time
        while in_ptr as usize + 16 <= in_end as usize {
            let v = v128_load(in_ptr as *const v128);
            let (res, _) = process_block_x2(
                v,
                i8x16_splat(0),
                shuf,
                mulhi_c,
                mullo_c,
                mask1,
                mask2,
                offsets,
                subs_val,
                cmpgt_val,
            );
            v128_store(out_ptr as *mut v128, res);
            in_ptr = in_ptr.add(12);
            out_ptr = out_ptr.add(16);
        }

        (
            in_ptr as usize - in_data.as_ptr() as usize,
            out_ptr as usize - out_data.as_ptr() as usize,
        )
    }
}
