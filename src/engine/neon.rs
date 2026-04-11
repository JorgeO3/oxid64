//! NEON Base64 codec — Turbo-Base64 port (AArch64).
//!
//! This module provides a high-performance NEON Base64 decoder and encoder
//! ported from [Turbo-Base64](https://github.com/powturbo/Turbo-Base64).
//!
//! # Decoder
//!
//! [`NeonDecoder`] supports two validation modes controlled by [`DecodeOpts`]:
//!
//! - **Non-strict** (`strict: false`). Validates only one of every four
//!   64-byte blocks per DN=256 iteration, matching the C library's default
//!   `CHECK0`-only behaviour.
//!
//! - **Strict** (`strict: true`, default). Validates all blocks per iteration.
//!
//! # Decoder algorithm overview
//!
//! Each 64-byte input chunk is loaded with `vld4q_u8`, which hardware
//! de-interleaves 64 bytes into four 16-byte lanes (A, B, C, D of each
//! Base64 quad).
//!
//! 1. **De-lookup**: A two-pass 128-byte table lookup converts each ASCII
//!    byte to its 6-bit value. The LUT is split into two 64-byte halves
//!    (`vlut0` for indices 0–63, `vlut1` for indices 64–127). For each byte:
//!    - `vqtbl4q_u8(vlut1, byte ^ 0x40)` resolves bytes 0x40–0x7F.
//!    - `vqtbx4q_u8(result, vlut0, byte)` resolves bytes 0x00–0x3F,
//!      keeping the vlut1 result for out-of-range indices.
//!      Invalid bytes produce `0xFF` (bit 7 set).
//!
//! 2. **Pack**: Byte-wise shifts and ORs combine 4× 6-bit values into
//!    3× 8-bit output bytes. The result is stored with `vst3q_u8`.
//!
//! 3. **Check**: An OR-accumulator collects any `0xFF` markers. After
//!    all blocks, `vaddvq_u8(vshrq_n_u8(xv, 7))` detects if any byte
//!    had bit 7 set (invalid input).
//!
//! # Encoder
//!
//! [`NeonDecoder::encode_to_slice`] encodes raw bytes to Base64 ASCII using
//! NEON interleaved loads (`vld3q_u8`) and a direct 64-byte `vqtbl4q_u8`
//! forward lookup, falling back to scalar for the tail.

use super::scalar::{decode_base64_fast, encode_base64_fast};
use super::{Base64Decoder, DecodeOpts};
use crate::engine::common::remaining;
use crate::engine::common::{assert_encode_capacity, prepare_decode_output};
use crate::engine::models::neon as verify_model;

use core::arch::aarch64::*;

#[inline]
fn has_neon_backend() -> bool {
    std::arch::is_aarch64_feature_detected!("neon")
}

/// NEON Base64 decoder and encoder (AArch64).
///
/// The decoder validation mode is controlled by [`DecodeOpts`]:
///
/// - `NeonDecoder::new()` — strict mode (default, all blocks validated).
/// - `NeonDecoder::with_opts(opts)` — custom configuration.
///
/// The encoder ([`encode_to_slice`](Self::encode_to_slice)) is independent
/// of decoder options.
pub struct NeonDecoder {
    opts: DecodeOpts,
}

impl NeonDecoder {
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
    /// Dispatches to the strict or non-strict NEON engine based on
    /// `self.opts.strict`, falling back to scalar for the tail.
    ///
    /// In strict mode, returns `None` if the input contains invalid Base64.
    ///
    /// In non-strict mode, this is a trusted-input `CHECK0` contract and does
    /// not validate every SIMD-processed block.
    #[inline]
    pub fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        let _out_len = prepare_decode_output(input, out)?;

        if has_neon_backend() {
            let engine_fn = if self.opts.strict {
                decode_engine::decode_neon_strict
                    as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            } else {
                decode_engine::decode_neon as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            };
            return super::dispatch_decode(input, out, engine_fn);
        }

        decode_base64_fast(input, out)
    }

    /// Encode raw bytes to Base64 ASCII, returning the number of bytes written.
    ///
    /// Uses NEON vectorised encoding when available, falling back to scalar
    /// for the tail. The encoder is independent of [`DecodeOpts`].
    #[inline]
    pub fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        assert_encode_capacity(input.len(), out.len());

        let mut consumed = 0usize;
        let mut written = 0usize;

        if has_neon_backend() {
            // SAFETY: feature gate checked above.
            let (c, w) = unsafe { encode_engine::encode_base64_neon(input, out) };
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

impl Default for NeonDecoder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Decoder for NeonDecoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        NeonDecoder::decode_to_slice(self, input, out)
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        NeonDecoder::encode_to_slice(self, input, out)
    }
}

// ---------------------------------------------------------------------------
// Shared decode dispatch
// ---------------------------------------------------------------------------

// Dispatches a NEON decode function, falling back to scalar for the tail.
//
// The SIMD engine processes as many full blocks as possible, returning
// `(consumed, written)`. This helper calls the scalar fallback for any
// remaining bytes.
// ---------------------------------------------------------------------------
// Decode LUT — 128-byte ASCII → 6-bit value (0xFF = invalid)
// ---------------------------------------------------------------------------

/// Base64 decode lookup table: ASCII byte → 6-bit value.
///
/// Invalid entries are `0xFF`. The table is split into two 64-byte halves
/// for the two-pass NEON table-lookup strategy.
#[rustfmt::skip]
static DECODE_LUT: [u8; 128] = [
    // 0x00 – 0x0F
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    // 0x10 – 0x1F
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    // 0x20 – 0x2F                       +              /
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF,   62, 0xFF, 0xFF, 0xFF,   63,
    // 0x30 – 0x3F  0-9
      52,   53,   54,   55,   56,   57,   58,   59,
      60,   61, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    // 0x40 – 0x4F  @  A-O
    0xFF,    0,    1,    2,    3,    4,    5,    6,
       7,    8,    9,   10,   11,   12,   13,   14,
    // 0x50 – 0x5F  P-Z  [ \ ] ^ _
      15,   16,   17,   18,   19,   20,   21,   22,
      23,   24,   25, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    // 0x60 – 0x6F  `  a-o
    0xFF,   26,   27,   28,   29,   30,   31,   32,
      33,   34,   35,   36,   37,   38,   39,   40,
    // 0x70 – 0x7F  p-z  { | } ~ DEL
      41,   42,   43,   44,   45,   46,   47,   48,
    49,   50,   51, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

// ---------------------------------------------------------------------------
// Decode engine — all functions require `target_feature(enable = "neon")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod decode_engine {
    use super::*;

    // -----------------------------------------------------------------------
    // De-lookup: ASCII → 6-bit value using two-pass NEON table lookup
    // -----------------------------------------------------------------------

    /// Perform the two-pass de-lookup on a single 16-byte lane.
    ///
    /// Uses `vqtbl4q_u8(vlut1, byte ^ 0x40)` for the upper half (0x40–0x7F),
    /// then `vqtbx4q_u8(result, vlut0, byte)` for the lower half (0x00–0x3F).
    /// Invalid bytes end up as `0xFF`.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn delookup(
        v: uint8x16_t,
        vlut0: uint8x16x4_t,
        vlut1: uint8x16x4_t,
        cv40: uint8x16_t,
    ) -> uint8x16_t {
        let upper = vqtbx4q_u8(vdupq_n_u8(0xFF), vlut1, veorq_u8(v, cv40));
        vqtbx4q_u8(upper, vlut0, v)
    }

    /// Accumulate invalid-byte markers from a decoded quad into the error
    /// accumulator `xv`. After de-lookup, valid bytes have values 0–63 (bit 7
    /// clear) and invalid bytes have `0xFF` (bit 7 set). ORing all four lanes
    /// and the accumulator preserves any set bit 7.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn b64chk(
        v0: uint8x16_t,
        v1: uint8x16_t,
        v2: uint8x16_t,
        v3: uint8x16_t,
        xv: uint8x16_t,
    ) -> uint8x16_t {
        vorrq_u8(xv, vorrq_u8(vorrq_u8(v0, v1), vorrq_u8(v2, v3)))
    }

    /// Check the error accumulator. Returns `true` if any byte had bit 7 set
    /// (i.e. an invalid Base64 character was encountered).
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn has_error(xv: uint8x16_t) -> bool {
        vaddvq_u8(vshrq_n_u8::<7>(xv)) != 0
    }

    // -----------------------------------------------------------------------
    // Bit-pack: four 6-bit lanes → three 8-bit lanes
    // -----------------------------------------------------------------------

    /// Pack four de-looked-up 16-byte lanes (6-bit values each) into three
    /// 8-bit output lanes, suitable for `vst3q_u8`.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn bitpack(
        v0: uint8x16_t,
        v1: uint8x16_t,
        v2: uint8x16_t,
        v3: uint8x16_t,
    ) -> uint8x16x3_t {
        uint8x16x3_t(
            // byte0 = (A << 2) | (B >> 4)
            vorrq_u8(vshlq_n_u8::<2>(v0), vshrq_n_u8::<4>(v1)),
            // byte1 = (B << 4) | (C >> 2)
            vorrq_u8(vshlq_n_u8::<4>(v1), vshrq_n_u8::<2>(v2)),
            // byte2 = (C << 6) | D
            vorrq_u8(vshlq_n_u8::<6>(v2), v3),
        )
    }

    // -----------------------------------------------------------------------
    // Process one 64-byte block: load, de-lookup, pack, store
    // -----------------------------------------------------------------------

    /// Decode one 64-byte block: `vld4q_u8` → de-lookup → bitpack → `vst3q_u8`.
    ///
    /// Returns the four de-looked-up lanes (before packing) so the caller can
    /// optionally feed them to `b64chk`.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn decode_block(
        ip: *const u8,
        op: *mut u8,
        vlut0: uint8x16x4_t,
        vlut1: uint8x16x4_t,
        cv40: uint8x16_t,
    ) -> (uint8x16_t, uint8x16_t, uint8x16_t, uint8x16_t) {
        let iv = vld4q_u8(ip);

        let d0 = delookup(iv.0, vlut0, vlut1, cv40);
        let d1 = delookup(iv.1, vlut0, vlut1, cv40);
        let d2 = delookup(iv.2, vlut0, vlut1, cv40);
        let d3 = delookup(iv.3, vlut0, vlut1, cv40);

        let ov = bitpack(d0, d1, d2, d3);
        vst3q_u8(op, ov);

        (d0, d1, d2, d3)
    }

    // -----------------------------------------------------------------------
    // Main decode functions
    // -----------------------------------------------------------------------

    /// NEON Base64 decode — **non-strict** mode.
    ///
    /// Validates only one of every four 64-byte blocks per DN=256 iteration
    /// (matching the C library's default `CHECK0`-only behaviour).
    ///
    /// Returns `(consumed_input, written_output)` or `None` on error.
    ///
    /// # Safety
    ///
    /// Caller must ensure the NEON CPU feature is available.
    #[inline]
    #[target_feature(enable = "neon")]
    pub unsafe fn decode_neon(in_data: &[u8], out_data: &mut [u8]) -> Option<(usize, usize)> {
        let ip_start = in_data.as_ptr();
        let op_start = out_data.as_mut_ptr();
        let inlen = in_data.len();
        let in_end = ip_start.add(inlen);

        // Load the two halves of the 128-byte decode LUT.
        let vlut0 = vld1q_u8_x4(DECODE_LUT.as_ptr());
        let vlut1 = vld1q_u8_x4(DECODE_LUT.as_ptr().add(64));
        let cv40 = vdupq_n_u8(0x40);
        let mut xv = vdupq_n_u8(0); // error accumulator

        let mut ip = ip_start;
        let mut op = op_start;

        // DN=256: process 256 input bytes → 192 output bytes per iteration.
        const DN: usize = 256;

        while remaining(ip, in_end) > DN {
            // Block 0 (ip + 0): CHECK1 in non-strict → skip check
            let (d0, d1, d2, d3) = decode_block(ip, op, vlut0, vlut1, cv40);
            // Block 1 (ip + 64): CHECK1 → skip check
            let _ = decode_block(ip.add(64), op.add(48), vlut0, vlut1, cv40);
            // Block 2 (ip + 128): CHECK1 → skip check
            let _ = decode_block(ip.add(128), op.add(96), vlut0, vlut1, cv40);
            // Block 3 (ip + 192): CHECK0 → always check
            let (e0, e1, e2, e3) = decode_block(ip.add(192), op.add(144), vlut0, vlut1, cv40);

            // Non-strict: only check the last block (CHECK0) plus
            // accumulate block 0 in the same pattern as the C code.
            // The C code does: CHECK1 on block 0, CHECK1 on block 1,
            // CHECK1 on block 2, CHECK0 on block 3.
            // With default CHECK0/CHECK1: only block 0 (CHECK1→nop when
            // DN>128) and block 3 (CHECK0→active) are checked. But
            // re-reading the C code more carefully:
            // - When DN>128: CHECK1(block0) is active, CHECK1(block1) is
            //   active, CHECK1(block2) is active, CHECK0(block3) is active.
            // Wait — in the default (non-strict) build: CHECK0(x) = x,
            // CHECK1(x) = nothing. So only CHECK0 fires. CHECK0 is on
            // block3 (last block). Let's match that exactly.
            xv = b64chk(e0, e1, e2, e3, xv);
            // Also accumulate block 0 per the C pattern (CHECK1 when
            // DN>128 is still a no-op in default mode, so skip it).
            let _ = (d0, d1, d2, d3);

            ip = ip.add(DN);
            op = op.add((DN / 4) * 3);
        }

        // Cleanup loop: 64 bytes at a time, always leaving the final block for
        // scalar so the semantic last quantum (and any padding) is never
        // handled by the raw SIMD block decoder.
        while remaining(ip, in_end) > verify_model::DECODE_BLOCK_INPUT_BYTES {
            let (d0, d1, d2, d3) = decode_block(ip, op, vlut0, vlut1, cv40);
            xv = b64chk(d0, d1, d2, d3, xv);
            ip = ip.add(64);
            op = op.add(48);
        }

        // Check for errors accumulated in SIMD path.
        if has_error(xv) {
            return None;
        }

        Some(crate::engine::offsets(ip, op, ip_start, op_start))
    }

    /// NEON Base64 decode — **strict** mode.
    ///
    /// Validates all four 64-byte blocks per DN=256 iteration.
    ///
    /// Returns `(consumed_input, written_output)` or `None` on error.
    ///
    /// # Safety
    ///
    /// Caller must ensure the NEON CPU feature is available.
    #[inline]
    #[target_feature(enable = "neon")]
    pub unsafe fn decode_neon_strict(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let ip_start = in_data.as_ptr();
        let op_start = out_data.as_mut_ptr();
        let inlen = in_data.len();
        let in_end = ip_start.add(inlen);

        // Load the two halves of the 128-byte decode LUT.
        let vlut0 = vld1q_u8_x4(DECODE_LUT.as_ptr());
        let vlut1 = vld1q_u8_x4(DECODE_LUT.as_ptr().add(64));
        let cv40 = vdupq_n_u8(0x40);
        let mut xv = vdupq_n_u8(0); // error accumulator

        let mut ip = ip_start;
        let mut op = op_start;

        // DN=256: process 256 input bytes → 192 output bytes per iteration.
        const DN: usize = 256;

        while remaining(ip, in_end) > DN {
            // Block 0 (ip + 0): validate
            let (d0, d1, d2, d3) = decode_block(ip, op, vlut0, vlut1, cv40);
            xv = b64chk(d0, d1, d2, d3, xv);

            // Block 1 (ip + 64): validate
            let (d0, d1, d2, d3) = decode_block(ip.add(64), op.add(48), vlut0, vlut1, cv40);
            xv = b64chk(d0, d1, d2, d3, xv);

            // Block 2 (ip + 128): validate
            let (d0, d1, d2, d3) = decode_block(ip.add(128), op.add(96), vlut0, vlut1, cv40);
            xv = b64chk(d0, d1, d2, d3, xv);

            // Block 3 (ip + 192): validate
            let (d0, d1, d2, d3) = decode_block(ip.add(192), op.add(144), vlut0, vlut1, cv40);
            xv = b64chk(d0, d1, d2, d3, xv);

            ip = ip.add(DN);
            op = op.add((DN / 4) * 3);
        }

        // Cleanup loop: 64 bytes at a time, always leaving the final block for
        // scalar so the semantic last quantum (and any padding) is never
        // handled by the raw SIMD block decoder.
        while remaining(ip, in_end) > verify_model::DECODE_BLOCK_INPUT_BYTES {
            let (d0, d1, d2, d3) = decode_block(ip, op, vlut0, vlut1, cv40);
            xv = b64chk(d0, d1, d2, d3, xv);
            ip = ip.add(64);
            op = op.add(48);
        }

        // Check for errors accumulated in SIMD path.
        if has_error(xv) {
            return None;
        }

        Some(crate::engine::offsets(ip, op, ip_start, op_start))
    }
}

// ---------------------------------------------------------------------------
// Encode engine — all functions require `target_feature(enable = "neon")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod encode_engine {
    use super::*;

    /// The standard Base64 alphabet used for encoding.
    static ENCODE_LUT: [u8; 64] =
        *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Bit-unpack three 8-bit input lanes into four 6-bit lanes, then
    /// forward-lookup each 6-bit value through the Base64 alphabet LUT.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn encode_block(ip: *const u8, op: *mut u8, vlut: uint8x16x4_t, cv3f: uint8x16_t) {
        let iv = vld3q_u8(ip);

        // Bit-unpack: 3 bytes → 4 six-bit values
        let a = vshrq_n_u8::<2>(iv.0);
        let b = vandq_u8(vorrq_u8(vshlq_n_u8::<4>(iv.0), vshrq_n_u8::<4>(iv.1)), cv3f);
        let c = vandq_u8(vorrq_u8(vshlq_n_u8::<2>(iv.1), vshrq_n_u8::<6>(iv.2)), cv3f);
        let d = vandq_u8(iv.2, cv3f);

        // Forward lookup: 6-bit → ASCII
        let ov = uint8x16x4_t(
            vqtbl4q_u8(vlut, a),
            vqtbl4q_u8(vlut, b),
            vqtbl4q_u8(vlut, c),
            vqtbl4q_u8(vlut, d),
        );

        vst4q_u8(op, ov);
    }

    /// NEON Base64 encode.
    ///
    /// Processes input in blocks of 96 bytes (-> 128 output bytes), two blocks
    /// per main-loop iteration (EN=128 output bytes per pair processed
    /// sequentially). Returns `(consumed_input, written_output)`.
    ///
    /// # Safety
    ///
    /// Caller must ensure the NEON CPU feature is available.
    #[inline]
    #[target_feature(enable = "neon")]
    pub unsafe fn encode_base64_neon(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let ip_start = in_data.as_ptr();
        let op_start = out_data.as_mut_ptr();
        let inlen = in_data.len();
        let in_end = ip_start.add(inlen);

        let vlut = vld1q_u8_x4(ENCODE_LUT.as_ptr());
        let cv3f = vdupq_n_u8(0x3F);

        let mut ip = ip_start;
        let mut op = op_start;

        // EN=128: process 128 output bytes (96 input bytes) per iteration,
        // split into 2× 48→64 sub-blocks.
        while verify_model::can_run_encode_pair(remaining(ip, in_end)) {
            encode_block(ip, op, vlut, cv3f);
            encode_block(ip.add(48), op.add(64), vlut, cv3f);
            ip = ip.add(verify_model::ENCODE_PAIR_INPUT_BYTES);
            op = op.add(verify_model::ENCODE_PAIR_OUTPUT_BYTES);
        }

        // Cleanup loop: 64 output bytes (48 input bytes) at a time.
        while verify_model::can_run_encode_block(remaining(ip, in_end)) {
            encode_block(ip, op, vlut, cv3f);
            ip = ip.add(verify_model::ENCODE_BLOCK_INPUT_BYTES);
            op = op.add(verify_model::ENCODE_BLOCK_OUTPUT_BYTES);
        }

        crate::engine::offsets(ip, op, ip_start, op_start)
    }
}
