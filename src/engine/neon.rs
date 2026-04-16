//! NEON Base64 codec — Turbo-Base64 port for AArch64.
//!
//! High-performance NEON Base64 decoder and encoder, ported from
//! Turbo-Base64 (https://github.com/powturbo/Turbo-Base64).
//!
//! Decoder modes (controlled by [`DecodeOpts`]):
//!
//! - **Non-strict** (`strict: false`): validates one of every four 64-byte
//!   blocks per DN=256 iteration (Turbo-Base64's CHECK0 behavior).
//! - **Strict** (`strict: true`, default): validates every processed block.
//!
//! Decoder algorithm overview:
//!
//! - Each 64-byte input chunk is loaded with `vld4q_u8`, which hardware
//!   de-interleaves bytes into four 16-byte lanes (A, B, C, D) — the four
//!   positions of a Base64 quad.
//!
//! 1. **De-lookup**: A two-pass 128-byte table lookup converts ASCII bytes
//!    to 6-bit values. The LUT is split into two 64-byte halves:
//!    `vlut0` (0x00–0x3F) and `vlut1` (0x40–0x7F). For each lane:
//!    - `vqtbl4q_u8(vlut1, byte ^ 0x40)` resolves indices 0x40–0x7F.
//!    - `vqtbx4q_u8(upper, vlut0, byte)` resolves 0x00–0x3F, keeping the
//!      upper result for out-of-range bytes. Invalid inputs map to `0xFF`.
//!
//! 2. **Pack**: Shift and OR operations combine four 6-bit values into
//!    three 8-bit output bytes; the result is written with `vst3q_u8`.
//!
//! 3. **Check**: An OR-accumulator gathers any `0xFF` markers. After SIMD
//!    processing, `vaddvq_u8(vshrq_n_u8(xv, 7))` detects any invalid byte.
//!
//! Encoder:
//!
//! [`NeonDecoder::encode_to_slice`] encodes raw bytes to Base64 ASCII using
//! interleaved NEON loads (`vld3q_u8`) and a direct 64-byte `vqtbl4q_u8`
//! forward lookup, falling back to the scalar encoder for the tail.

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

/// NEON Base64 decoder and encoder for AArch64.
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
// Shared decode dispatch & LUT
// ---------------------------------------------------------------------------
//
// Dispatches a NEON decode function and falls back to the scalar path for
// any remaining bytes. The SIMD engine processes as many full SIMD blocks
// as possible and returns `(consumed, written)` describing how many input
// bytes were consumed and how many output bytes were produced. Callers are
// responsible for handling the scalar tail.
//
// The following section defines the 128-byte decode LUT used by the
// SIMD de-lookup implementation (ASCII -> 6-bit value, 0xFF = invalid).
// ---------------------------------------------------------------------------

/// Wrapper to ensure the decode LUT is aligned to a 64-byte cache line.
#[repr(C, align(64))]
struct AlignedLut([u8; 128]);

/// Base64 decode lookup table: ASCII byte → 6-bit value.
///
/// Invalid entries are `0xFF`. The table is split into two 64-byte halves
/// for the two-pass NEON table-lookup strategy.
/// Cache-line aligned to avoid split loads.
#[rustfmt::skip]
static DECODE_LUT: AlignedLut = AlignedLut([
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
]);

// ---------------------------------------------------------------------------
// Decode engine — all functions require `target_feature(enable = "neon")`
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod decode_engine {
    use super::*;

    // -----------------------------------------------------------------------
    // Four-lane SIMD block type
    // -----------------------------------------------------------------------

    /// Four NEON 16-byte lanes — the canonical unit of a 64-byte Base64
    /// block after de-interleaving or de-lookup.
    #[derive(Clone, Copy)]
    struct Quad(uint8x16_t, uint8x16_t, uint8x16_t, uint8x16_t);

    impl From<uint8x16x4_t> for Quad {
        #[inline]
        fn from(v: uint8x16x4_t) -> Self {
            Self(v.0, v.1, v.2, v.3)
        }
    }

    // -----------------------------------------------------------------------
    // Decode LUT bundle
    // -----------------------------------------------------------------------

    /// Bundled decode LUT registers: the two 64-byte table halves and the
    /// XOR constant for the upper-half lookup pass.
    struct DecodeLuts {
        vlut0: uint8x16x4_t,
        vlut1: uint8x16x4_t,
        cv40: uint8x16_t,
    }

    impl DecodeLuts {
        /// Load the 128-byte decode LUT into NEON registers.
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn load() -> Self {
            Self {
                vlut0: vld1q_u8_x4(DECODE_LUT.0.as_ptr()),
                vlut1: vld1q_u8_x4(DECODE_LUT.0.as_ptr().add(64)),
                cv40: vdupq_n_u8(0x40),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pure SIMD primitives (stateless)
    // -----------------------------------------------------------------------

    /// Issue a software prefetch for the cache line at `addr`.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn prefetch(addr: *const u8) {
        core::arch::asm!(
            "prfm pldl1keep, [{addr}]",
            addr = in(reg) addr,
            options(nostack, preserves_flags),
        );
    }

    /// Two-pass de-lookup for a single 16-byte lane using the loaded LUTs.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn delookup(v: uint8x16_t, luts: &DecodeLuts) -> uint8x16_t {
        let upper = vqtbl4q_u8(luts.vlut1, veorq_u8(v, luts.cv40));
        vqtbx4q_u8(upper, luts.vlut0, v)
    }

    /// De-lookup all four lanes of a 64-byte block and return a `Quad` of results.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn decode_lanes(iv: uint8x16x4_t, luts: &DecodeLuts) -> Quad {
        Quad(
            delookup(iv.0, luts),
            delookup(iv.1, luts),
            delookup(iv.2, luts),
            delookup(iv.3, luts),
        )
    }

    /// Pack four 6-bit lanes into three 8-bit vectors and return `uint8x16x3_t`
    /// suitable for `vst3q_u8`.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn bitpack(q: &Quad) -> uint8x16x3_t {
        uint8x16x3_t(
            vorrq_u8(vshlq_n_u8::<2>(q.0), vshrq_n_u8::<4>(q.1)),
            vorrq_u8(vshlq_n_u8::<4>(q.1), vshrq_n_u8::<2>(q.2)),
            vorrq_u8(vshlq_n_u8::<6>(q.2), q.3),
        )
    }

    /// Fold four lanes into the error accumulator (OR tree) and return the
    /// updated accumulator.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn chk_fold(q: &Quad, xv: uint8x16_t) -> uint8x16_t {
        vorrq_u8(xv, vorrq_u8(vorrq_u8(q.0, q.1), vorrq_u8(q.2, q.3)))
    }

    /// Strict combined check: OR decoded values with raw input bytes and
    /// fold into the accumulator.
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn b64chk_strict(d: &Quad, raw: &Quad, xv: uint8x16_t) -> uint8x16_t {
        vorrq_u8(
            xv,
            vorrq_u8(
                vorrq_u8(vorrq_u8(d.0, raw.0), vorrq_u8(d.1, raw.1)),
                vorrq_u8(vorrq_u8(d.2, raw.2), vorrq_u8(d.3, raw.3)),
            ),
        )
    }

    /// Return `true` if any accumulator byte has bit 7 set (indicates
    /// invalid input).
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn has_error(xv: uint8x16_t) -> bool {
        vaddvq_u8(vshrq_n_u8::<7>(xv)) != 0
    }

    // -----------------------------------------------------------------------
    // Decode context — bundles input/output cursors, LUTs and error accumulator
    // -----------------------------------------------------------------------

    /// Mutable decode state used by the SIMD pipeline.
    ///
    /// Methods assume the NEON CPU feature has been enabled for the caller.
    struct DecodeCtx {
        ip: *const u8,
        op: *mut u8,
        in_end: *const u8,
        ip_start: *const u8,
        op_start: *mut u8,
        luts: DecodeLuts,
        xv: uint8x16_t,
    }

    impl DecodeCtx {
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn new(in_data: &[u8], out_data: &mut [u8]) -> Self {
            let ip_start = in_data.as_ptr();
            let op_start = out_data.as_mut_ptr();
            Self {
                ip: ip_start,
                op: op_start,
                in_end: ip_start.add(in_data.len()),
                ip_start,
                op_start,
                luts: DecodeLuts::load(),
                xv: vdupq_n_u8(0),
            }
        }

        #[inline]
        fn remaining(&self) -> usize {
            remaining(self.ip, self.in_end)
        }

        #[inline]
        unsafe fn advance(&mut self, in_bytes: usize, out_bytes: usize) {
            self.ip = self.ip.add(in_bytes);
            self.op = self.op.add(out_bytes);
        }

        /// Decode one 64-byte block with full strict validation.
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn decode_block_checked(&mut self) {
            let iv = vld4q_u8(self.ip);
            let d = decode_lanes(iv, &self.luts);
            self.xv = b64chk_strict(&d, &Quad::from(iv), self.xv);
            vst3q_u8(self.op, bitpack(&d));
        }

        /// Process remaining 64-byte blocks, check for errors and return offsets.
        ///
        /// Returns `None` on invalid input.
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn decode_tail(mut self) -> Option<(usize, usize)> {
            while self.remaining() > verify_model::DECODE_BLOCK_INPUT_BYTES {
                self.decode_block_checked();
                self.advance(64, 48);
            }

            if has_error(self.xv) {
                return None;
            }

            Some(crate::engine::offsets(
                self.ip,
                self.op,
                self.ip_start,
                self.op_start,
            ))
        }
    }

    // -----------------------------------------------------------------------
    // Main decode functions
    // -----------------------------------------------------------------------

    /// NEON Base64 decode — **non-strict** mode (DN=256, CHECK0-only).
    ///
    /// # Safety
    /// The `neon` CPU feature must be available on the executing core.
    #[inline]
    #[target_feature(enable = "neon")]
    pub unsafe fn decode_neon(in_data: &[u8], out_data: &mut [u8]) -> Option<(usize, usize)> {
        let mut ctx = DecodeCtx::new(in_data, out_data);

        const DN: usize = 256;

        while ctx.remaining() > DN {
            // Blocks 0 & 1: load, decode, pack (no validation)
            let iv0 = vld4q_u8(ctx.ip);
            let d0 = decode_lanes(iv0, &ctx.luts);
            let ov0 = bitpack(&d0);

            let iv1 = vld4q_u8(ctx.ip.add(64));
            let d1 = decode_lanes(iv1, &ctx.luts);
            let ov1 = bitpack(&d1);

            // Early load blocks 2 & 3 (software pipelining)
            let iv2 = vld4q_u8(ctx.ip.add(128));
            let iv3 = vld4q_u8(ctx.ip.add(192));

            // Store blocks 0 & 1
            vst3q_u8(ctx.op, ov0);
            vst3q_u8(ctx.op.add(48), ov1);

            // Block 2: decode, pack (no validation)
            let d2 = decode_lanes(iv2, &ctx.luts);
            let ov2 = bitpack(&d2);

            // Block 3: decode + CHECK0
            ctx.xv = chk_fold(&Quad::from(iv3), ctx.xv);
            let d3 = decode_lanes(iv3, &ctx.luts);
            ctx.xv = chk_fold(&d3, ctx.xv);
            let ov3 = bitpack(&d3);

            // Store blocks 2 & 3
            vst3q_u8(ctx.op.add(96), ov2);
            vst3q_u8(ctx.op.add(144), ov3);

            ctx.advance(DN, (DN / 4) * 3);
        }

        ctx.decode_tail()
    }

    /// NEON Base64 decode — **strict** mode (DN=128, dual-accumulator).
    ///
    /// # Safety
    /// The `neon` CPU feature must be available on the executing core.
    #[inline]
    #[target_feature(enable = "neon")]
    pub unsafe fn decode_neon_strict(
        in_data: &[u8],
        out_data: &mut [u8],
    ) -> Option<(usize, usize)> {
        let mut ctx = DecodeCtx::new(in_data, out_data);
        let mut rv = vdupq_n_u8(0);

        const DN: usize = 128;

        let simd_limit = if in_data.len() > DN {
            ctx.in_end as usize - DN
        } else {
            0
        };

        while (ctx.ip as usize) < simd_limit {
            prefetch(ctx.ip.add(DN));

            // Early load both blocks
            let iv0 = vld4q_u8(ctx.ip);
            let iv1 = vld4q_u8(ctx.ip.add(64));

            // Decode both blocks
            let d0 = decode_lanes(iv0, &ctx.luts);
            let ov0 = bitpack(&d0);

            let d1 = decode_lanes(iv1, &ctx.luts);
            let ov1 = bitpack(&d1);

            // Error checks (dual-acc: xv for decoded, rv for raw)
            ctx.xv = chk_fold(&d0, ctx.xv);
            ctx.xv = chk_fold(&d1, ctx.xv);
            rv = chk_fold(&Quad::from(iv0), rv);
            rv = chk_fold(&Quad::from(iv1), rv);

            // Store blocks 0 & 1
            vst3q_u8(ctx.op, ov0);
            vst3q_u8(ctx.op.add(48), ov1);

            ctx.advance(DN, 96);
        }

        // Merge raw-input accumulator before tail check.
        ctx.xv = vorrq_u8(ctx.xv, rv);

        ctx.decode_tail()
    }
}

/// Thin wrapper that calls the non-strict NEON decode kernel.
///
/// # Safety
/// The `neon` CPU feature must be available on the executing core.
#[target_feature(enable = "neon")]
pub unsafe fn decode_neon_kernel_partial(input: &[u8], out: &mut [u8]) -> Option<(usize, usize)> {
    unsafe { decode_engine::decode_neon(input, out) }
}

/// NEON strict-mode decode kernel.  Returns `(consumed_input, written_output)`
/// or `None` on invalid input.
///
/// # Safety
/// The `neon` CPU feature must be available on the executing core.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn decode_neon_kernel_strict(input: &[u8], out: &mut [u8]) -> Option<(usize, usize)> {
    unsafe { decode_engine::decode_neon_strict(input, out) }
}

/// NEON encode kernel.  Returns `(consumed_input, written_output)`.
///
/// # Safety
/// The `neon` CPU feature must be available on the executing core.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn encode_neon_kernel(input: &[u8], out: &mut [u8]) -> (usize, usize) {
    unsafe { encode_engine::encode_base64_neon(input, out) }
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

    /// Mutable encode state: input/output cursors and loaded LUT registers.
    struct EncodeCtx {
        ip: *const u8,
        op: *mut u8,
        in_end: *const u8,
        ip_start: *const u8,
        op_start: *mut u8,
        vlut: uint8x16x4_t,
        cv3f: uint8x16_t,
    }

    impl EncodeCtx {
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn new(in_data: &[u8], out_data: &mut [u8]) -> Self {
            let ip_start = in_data.as_ptr();
            let op_start = out_data.as_mut_ptr();
            Self {
                ip: ip_start,
                op: op_start,
                in_end: ip_start.add(in_data.len()),
                ip_start,
                op_start,
                vlut: vld1q_u8_x4(ENCODE_LUT.as_ptr()),
                cv3f: vdupq_n_u8(0x3F),
            }
        }

        #[inline]
        fn remaining(&self) -> usize {
            remaining(self.ip, self.in_end)
        }

        /// Encode one 48→64 byte block at the current position and advance
        /// the internal input/output cursors.
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn encode_block(&mut self) {
            let iv = vld3q_u8(self.ip);

            // Bit-unpack: 3 bytes → 4 six-bit values
            let a = vshrq_n_u8::<2>(iv.0);
            let b = vandq_u8(
                vorrq_u8(vshlq_n_u8::<4>(iv.0), vshrq_n_u8::<4>(iv.1)),
                self.cv3f,
            );
            let c = vandq_u8(
                vorrq_u8(vshlq_n_u8::<2>(iv.1), vshrq_n_u8::<6>(iv.2)),
                self.cv3f,
            );
            let d = vandq_u8(iv.2, self.cv3f);

            // Forward lookup: 6-bit → ASCII
            let ov = uint8x16x4_t(
                vqtbl4q_u8(self.vlut, a),
                vqtbl4q_u8(self.vlut, b),
                vqtbl4q_u8(self.vlut, c),
                vqtbl4q_u8(self.vlut, d),
            );

            vst4q_u8(self.op, ov);
            self.ip = self.ip.add(verify_model::ENCODE_BLOCK_INPUT_BYTES);
            self.op = self.op.add(verify_model::ENCODE_BLOCK_OUTPUT_BYTES);
        }
    }

    /// NEON Base64 encode.
    ///
    /// Processes input in pairs of 48-byte blocks (96→128), then single
    /// blocks (48→64). Returns `(consumed_input, written_output)`.
    ///
    /// # Safety
    ///
    /// Caller must ensure the NEON CPU feature is available.
    #[inline]
    #[target_feature(enable = "neon")]
    pub unsafe fn encode_base64_neon(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let mut ctx = EncodeCtx::new(in_data, out_data);

        while verify_model::can_run_encode_pair(ctx.remaining()) {
            ctx.encode_block();
            ctx.encode_block();
        }

        while verify_model::can_run_encode_block(ctx.remaining()) {
            ctx.encode_block();
        }

        crate::engine::offsets(ctx.ip, ctx.op, ctx.ip_start, ctx.op_start)
    }
}
