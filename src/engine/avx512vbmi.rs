//! AVX-512 VBMI Base64 codec — Turbo-Base64 port.
//!
//! Uses the AVX-512 VBMI byte-granularity permute instructions
//! (`vpermb`, `vpermi2b`, `vpmultishiftqb`) for a 3-instruction decode
//! core and a 3-instruction encode core operating on 512-bit vectors.
//! Requires Ice Lake or later.

use super::scalar::{decode_base64_fast, encode_base64_fast};
use super::{d2i, w2i, Base64Decoder, DecodeOpts};
use crate::engine::common::{assert_encode_capacity, prepare_decode_output, remaining};
use crate::engine::models::avx512vbmi as verify_model;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// AVX-512 VBMI Base64 encoder and decoder.
///
/// Requires Ice Lake or later (AVX-512F + AVX-512BW + AVX-512VBMI).
///
/// The decoder validation mode is controlled by [`DecodeOpts`]:
///
/// - `Avx512VbmiDecoder::new()` — strict mode (default, all vectors validated).
/// - `Avx512VbmiDecoder::with_opts(opts)` — custom configuration.
pub struct Avx512VbmiDecoder {
    opts: DecodeOpts,
}

#[inline]
pub(crate) fn has_avx512vbmi() -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && std::is_x86_feature_detected!("avx512vbmi")
}

impl Avx512VbmiDecoder {
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
    /// Dispatches to the strict or non-strict AVX-512 VBMI engine based on
    /// `self.opts.strict`, falling back to scalar for the tail.
    ///
    /// Returns `None` if the input contains invalid Base64 characters.
    #[inline]
    pub fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        let _out_len = prepare_decode_output(input, out)?;

        if has_avx512vbmi() {
            let engine_fn = if self.opts.strict {
                decode_engine::decode_avx512_strict
                    as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            } else {
                decode_engine::decode_avx512
                    as unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>
            };
            return super::dispatch_decode(input, out, engine_fn);
        }

        decode_base64_fast(input, out)
    }

    /// Encode raw bytes to Base64 ASCII, returning the number of bytes written.
    ///
    /// Uses AVX-512 VBMI vectorised encoding when available, falling back to
    /// scalar for the tail.
    #[inline]
    pub fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        assert_encode_capacity(input.len(), out.len());

        let mut consumed = 0usize;
        let mut written = 0usize;

        if has_avx512vbmi() {
            // SAFETY: feature gate checked above.
            let (c, w) = unsafe { encode_engine::encode_base64_avx512(input, out) };
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

impl Default for Avx512VbmiDecoder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Decoder for Avx512VbmiDecoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        Avx512VbmiDecoder::decode_to_slice(self, input, out)
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        Avx512VbmiDecoder::encode_to_slice(self, input, out)
    }
}

// ---------------------------------------------------------------------------
// Shared decode dispatch (mirrors avx2::dispatch_decode)
// ---------------------------------------------------------------------------

/// Dispatches an AVX-512 decode function, falling back to scalar for the tail.
// ---------------------------------------------------------------------------
// Decode engine — all functions require AVX-512 VBMI
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod decode_engine {
    #[allow(unused_imports)]
    use super::*;

    /// 128-byte decode lookup table (two 64-byte halves for `permutex2var`).
    ///
    /// Maps ASCII Base64 characters to their 6-bit values. Invalid characters
    /// map to `0x80` (high bit set) for error detection.
    struct DecodeTables {
        /// Lower 64 bytes of the 128-byte decode LUT.
        vlut0: __m512i,
        /// Upper 64 bytes of the 128-byte decode LUT.
        vlut1: __m512i,
        /// Cross-pack permutation: compacts 48 decoded bytes from a 64-byte
        /// register after maddubs+madd. The last 16 bytes are zeroed.
        vp: __m512i,
    }

    impl DecodeTables {
        /// Initialise all SIMD constants.
        ///
        /// # Safety
        ///
        /// Caller must ensure AVX-512 VBMI is available.
        #[inline]
        #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
        unsafe fn new() -> Self {
            Self {
                // ASCII 0x00..0x3F → mostly invalid (0x80), except:
                //   '+' (0x2B) → 62 (0x3E)
                //   '/' (0x2F) → 63 (0x3F)
                //   '0'-'9' (0x30..0x39) → 52..61 (0x34..0x3D)
                //   '=' (0x3D) → 0x3C? No, '=' is padding handled by scalar.
                vlut0: _mm512_setr_epi32(
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x80808080),
                    w2i(0x3e808080), // 0x28..0x2B: ..., ..., ..., '+'=0x3E
                    w2i(0x3f808080), // 0x2C..0x2F: ..., ..., ..., '/'=0x3F
                    w2i(0x37363534), // 0x30..0x33: '0'=52, '1'=53, '2'=54, '3'=55
                    w2i(0x3b3a3938), // 0x34..0x37: '4'=56, '5'=57, '6'=58, '7'=59
                    w2i(0x80803d3c), // 0x38..0x3B: '8'=60, '9'=61, ..., ...
                    w2i(0x80808080), // 0x3C..0x3F: all invalid
                ),
                // ASCII 0x40..0x7F:
                //   'A'-'Z' (0x41..0x5A) → 0..25
                //   'a'-'z' (0x61..0x7A) → 26..51
                vlut1: _mm512_setr_epi32(
                    w2i(0x02010080), // 0x40..0x43: '@'=inv, 'A'=0, 'B'=1, 'C'=2
                    w2i(0x06050403), // 0x44..0x47: 'D'=3, 'E'=4, 'F'=5, 'G'=6
                    w2i(0x0a090807), // 0x48..0x4B: 'H'=7, 'I'=8, 'J'=9, 'K'=10
                    w2i(0x0e0d0c0b), // 0x4C..0x4F: 'L'=11, 'M'=12, 'N'=13, 'O'=14
                    w2i(0x1211100f), // 0x50..0x53: 'P'=15, 'Q'=16, 'R'=17, 'S'=18
                    w2i(0x16151413), // 0x54..0x57: 'T'=19, 'U'=20, 'V'=21, 'W'=22
                    w2i(0x80191817), // 0x58..0x5B: 'X'=23, 'Y'=24, 'Z'=25, '['=inv
                    w2i(0x80808080), // 0x5C..0x5F: all invalid
                    w2i(0x1c1b1a80), // 0x60..0x63: '`'=inv, 'a'=26, 'b'=27, 'c'=28
                    w2i(0x201f1e1d), // 0x64..0x67: 'd'=29, 'e'=30, 'f'=31, 'g'=32
                    w2i(0x24232221), // 0x68..0x6B: 'h'=33, 'i'=34, 'j'=35, 'k'=36
                    w2i(0x28272625), // 0x6C..0x6F: 'l'=37, 'm'=38, 'n'=39, 'o'=40
                    w2i(0x2c2b2a29), // 0x70..0x73: 'p'=41, 'q'=42, 'r'=43, 's'=44
                    w2i(0x302f2e2d), // 0x74..0x77: 't'=45, 'u'=46, 'v'=47, 'w'=48
                    w2i(0x80333231), // 0x78..0x7B: 'x'=49, 'y'=50, 'z'=51, '{'=inv
                    w2i(0x80808080), // 0x7C..0x7F: all invalid
                ),
                // Cross-pack permutation: extracts 48 decoded bytes from a
                // 512-bit register after maddubs + madd bit-packing.
                // Each 128-bit lane has 12 valid bytes in positions determined
                // by the madd layout; this permutation gathers them into the
                // low 48 bytes. The last 16 bytes (4 dwords) are zero-padded.
                vp: _mm512_setr_epi32(
                    w2i(0x06000102),
                    w2i(0x090a0405),
                    w2i(0x0c0d0e08),
                    w2i(0x16101112),
                    w2i(0x191a1415),
                    w2i(0x1c1d1e18),
                    w2i(0x26202122),
                    w2i(0x292a2425),
                    w2i(0x2c2d2e28),
                    w2i(0x36303132),
                    w2i(0x393a3435),
                    w2i(0x3c3d3e38),
                    w2i(0x00000000),
                    w2i(0x00000000),
                    w2i(0x00000000),
                    w2i(0x00000000),
                ),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Core SIMD helpers — AVX-512 VBMI (512-bit)
    // -----------------------------------------------------------------------

    /// Map a 64-byte Base64 input vector to 6-bit decoded values using the
    /// VBMI `permutex2var` instruction (single 128-byte LUT lookup).
    ///
    /// Port of C macro `BITMAP256V8_6(iv, ov)`.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn bitmap512v8_6(iv: __m512i, t: &DecodeTables) -> __m512i {
        _mm512_permutex2var_epi8(t.vlut0, iv, t.vlut1)
    }

    /// Bit-pack a 512-bit vector of 6-bit values into 48 contiguous decoded
    /// bytes in the low portion of the result.
    ///
    /// Port of C macro `BITPACK512V8_6(v)`.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn bitpack512v8_6(v: __m512i, vp: __m512i) -> __m512i {
        let mul1 = _mm512_set1_epi32(w2i(0x01400140));
        let mul2 = _mm512_set1_epi32(w2i(0x00011000));
        let merge = _mm512_maddubs_epi16(v, mul1);
        let packed = _mm512_madd_epi16(merge, mul2);
        _mm512_permutexvar_epi8(vp, packed)
    }

    /// Accumulate validity check into `vx` using ternary logic OR.
    ///
    /// Port of C macro `B64CHK(iv, ov, vx)`.
    /// `ternarylogic(vx, ov, iv, 0xfe)` computes `vx | ov | iv`.
    /// Since invalid characters in `ov` have bit 7 set (0x80), OR-ing with
    /// `iv` (which has bit 7 set for any ASCII >= 0x80) and accumulating
    /// into `vx` means `movepi8_mask(vx) != 0` detects any invalids.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn b64chk512(iv: __m512i, ov: __m512i, vx: __m512i) -> __m512i {
        _mm512_ternarylogic_epi32(vx, ov, iv, 0xfe)
    }

    // -----------------------------------------------------------------------
    // DS256 block processing
    // -----------------------------------------------------------------------

    /// Process one DS256 block in non-strict (CHECK0) mode.
    ///
    /// 256 input bytes -> 192 output bytes. Direct port of the C `DS256(_i_)`
    /// macro with CHECK0 semantics: only the first vector of the first pair
    /// is validated; the second pair is always validated.
    ///
    /// `iu0`/`iu1` are forwarded: on entry they hold the current block's first
    /// pair; on exit they hold the next block's first pair.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn process_ds256_partial(
        ip: *const u8,
        op: *mut u8,
        base: usize,
        iu0: &mut __m512i,
        iu1: &mut __m512i,
        t: &DecodeTables,
        vx: &mut __m512i,
    ) {
        // Load second pair of this block
        let iv0 = _mm512_loadu_si512(ip.add(128 + base) as *const __m512i);
        let iv1 = _mm512_loadu_si512(ip.add(128 + base + 64) as *const __m512i);

        // Map + pack first pair (iu0, iu1)
        let ou0 = bitmap512v8_6(*iu0, t);
        // CHECK0: validate only first vector
        *vx = b64chk512(*iu0, ou0, *vx);
        let ou0 = bitpack512v8_6(ou0, t.vp);

        let ou1 = bitmap512v8_6(*iu1, t);
        // CHECK0: skip iu1 validation in non-strict
        let ou1 = bitpack512v8_6(ou1, t.vp);

        // Forward-load next iteration's first pair
        *iu0 = _mm512_loadu_si512(ip.add(128 + base + 128) as *const __m512i);
        *iu1 = _mm512_loadu_si512(ip.add(128 + base + 192) as *const __m512i);

        // Store first pair: each 512-bit register has 48 valid bytes in the
        // low portion after bitpack. Use a single 512-bit store (only low 48
        // bytes are meaningful, but we write 64 — the next store will overlap).
        let ob = base / 4 * 3;
        _mm512_storeu_si512(op.add(ob) as *mut __m512i, ou0);
        _mm512_storeu_si512(op.add(ob + 48) as *mut __m512i, ou1);

        // Map + pack second pair (iv0, iv1)
        let ov0 = bitmap512v8_6(iv0, t);
        // CHECK1: second pair is always validated
        *vx = b64chk512(iv0, ov0, *vx);
        let ov0 = bitpack512v8_6(ov0, t.vp);

        let ov1 = bitmap512v8_6(iv1, t);
        *vx = b64chk512(iv1, ov1, *vx);
        let ov1 = bitpack512v8_6(ov1, t.vp);

        // Store second pair
        _mm512_storeu_si512(op.add(ob + 96) as *mut __m512i, ov0);
        _mm512_storeu_si512(op.add(ob + 144) as *mut __m512i, ov1);
    }

    /// Process one DS256 block in strict (B64CHECK) mode.
    ///
    /// Same as [`process_ds256_partial`] but validates ALL four vectors
    /// (both pairs), matching Turbo-Base64's B64CHECK behaviour.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn process_ds256_strict(
        ip: *const u8,
        op: *mut u8,
        base: usize,
        iu0: &mut __m512i,
        iu1: &mut __m512i,
        t: &DecodeTables,
        vx: &mut __m512i,
    ) {
        // Load second pair of this block
        let iv0 = _mm512_loadu_si512(ip.add(128 + base) as *const __m512i);
        let iv1 = _mm512_loadu_si512(ip.add(128 + base + 64) as *const __m512i);

        // Map + pack first pair (iu0, iu1)
        let ou0 = bitmap512v8_6(*iu0, t);
        // CHECK0: validate first vector
        *vx = b64chk512(*iu0, ou0, *vx);
        let ou0 = bitpack512v8_6(ou0, t.vp);

        let ou1 = bitmap512v8_6(*iu1, t);
        // CHECK1: validate second vector (strict mode)
        *vx = b64chk512(*iu1, ou1, *vx);
        let ou1 = bitpack512v8_6(ou1, t.vp);

        // Forward-load next iteration's first pair
        *iu0 = _mm512_loadu_si512(ip.add(128 + base + 128) as *const __m512i);
        *iu1 = _mm512_loadu_si512(ip.add(128 + base + 192) as *const __m512i);

        // Store first pair (48 valid bytes per 512-bit register)
        let ob = base / 4 * 3;
        _mm512_storeu_si512(op.add(ob) as *mut __m512i, ou0);
        _mm512_storeu_si512(op.add(ob + 48) as *mut __m512i, ou1);

        // Map + pack second pair (iv0, iv1)
        let ov0 = bitmap512v8_6(iv0, t);
        *vx = b64chk512(iv0, ov0, *vx);
        let ov0 = bitpack512v8_6(ov0, t.vp);

        let ov1 = bitmap512v8_6(iv1, t);
        *vx = b64chk512(iv1, ov1, *vx);
        let ov1 = bitpack512v8_6(ov1, t.vp);

        // Store second pair
        _mm512_storeu_si512(op.add(ob + 96) as *mut __m512i, ov0);
        _mm512_storeu_si512(op.add(ob + 144) as *mut __m512i, ov1);
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Public entry points
    // -----------------------------------------------------------------------

    /// Non-strict AVX-512 VBMI decode (CHECK0 mode).
    ///
    /// Direct port of `tb64v512dec` from `turbob64v512.c` with default
    /// CHECK0/CHECK1 semantics.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX-512 VBMI is available.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub unsafe fn decode_avx512(in_data: &[u8], out_data: &mut [u8]) -> Option<(usize, usize)> {
        let inlen = in_data.len();
        if inlen & 3 != 0 {
            return None;
        }

        let in_base = in_data.as_ptr();
        let out_base = out_data.as_mut_ptr();
        let in_ = in_base.add(inlen);
        let mut ip = in_base;
        let mut op = out_base;
        let mut vx = _mm512_setzero_si512();

        let t = DecodeTables::new();

        // Main unrolled loop: DS256 x2 = 512 input -> 384 output
        // Guard: need 128 (pre-load) + 2*256 (two DS256 blocks) + 4 (tail slack)
        if inlen >= 128 + 256 + 4 {
            let mut iu0 = _mm512_loadu_si512(ip as *const __m512i);
            let mut iu1 = _mm512_loadu_si512(ip.add(64) as *const __m512i);

            // Double-DS256 unrolled loop: 512 input -> 384 output
            while remaining(ip, in_) > 128 + 2 * 256 + 4 {
                process_ds256_partial(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                process_ds256_partial(ip, op, 256, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(512);
                op = op.add(384);
            }

            // Single-DS256 drain: 256 input -> 192 output
            if remaining(ip, in_) > 128 + 256 + 4 {
                process_ds256_partial(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(256);
                op = op.add(192);
            }
        } else if inlen == 0 {
            return Some((0, 0));
        }

        // Single-vector 64-byte tail loop
        while remaining(ip, in_) > 64 + 16 + 4 {
            let iv = _mm512_loadu_si512(ip as *const __m512i);
            let ov = bitmap512v8_6(iv, &t);
            // CHECK0: validate in tail
            vx = b64chk512(iv, ov, vx);
            let ov = bitpack512v8_6(ov, t.vp);
            _mm512_storeu_si512(op as *mut __m512i, ov);
            ip = ip.add(64);
            op = op.add(48);
        }

        // Check SIMD error accumulator
        if _mm512_movepi8_mask(vx) != 0 {
            return None;
        }

        Some(crate::engine::offsets(ip, op, in_base, out_base))
    }

    /// Strict AVX-512 VBMI decode (B64CHECK mode — all vectors validated).
    ///
    /// Same structure as [`decode_avx512`] but uses the strict
    /// `process_ds256_strict` which validates all 4 vectors per block.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX-512 VBMI is available.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub unsafe fn decode_avx512_strict(
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
        let mut vx = _mm512_setzero_si512();

        let t = DecodeTables::new();

        // Main unrolled loop with strict checking
        if inlen >= 128 + 256 + 4 {
            let mut iu0 = _mm512_loadu_si512(ip as *const __m512i);
            let mut iu1 = _mm512_loadu_si512(ip.add(64) as *const __m512i);

            // Double-DS256 unrolled loop: 512 input -> 384 output
            while remaining(ip, in_) > 128 + 2 * 256 + 4 {
                process_ds256_strict(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                process_ds256_strict(ip, op, 256, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(512);
                op = op.add(384);
            }

            // Single-DS256 drain
            if remaining(ip, in_) > 128 + 256 + 4 {
                process_ds256_strict(ip, op, 0, &mut iu0, &mut iu1, &t, &mut vx);
                ip = ip.add(256);
                op = op.add(192);
            }
        } else if inlen == 0 {
            return Some((0, 0));
        }

        // Single-vector 64-byte tail loop (all validated in strict)
        while remaining(ip, in_) > 64 + 16 + 4 {
            let iv = _mm512_loadu_si512(ip as *const __m512i);
            let ov = bitmap512v8_6(iv, &t);
            vx = b64chk512(iv, ov, vx);
            let ov = bitpack512v8_6(ov, t.vp);
            _mm512_storeu_si512(op as *mut __m512i, ov);
            ip = ip.add(64);
            op = op.add(48);
        }

        // Check SIMD error accumulator
        if _mm512_movepi8_mask(vx) != 0 {
            return None;
        }

        Some(crate::engine::offsets(ip, op, in_base, out_base))
    }
}

// ---------------------------------------------------------------------------
// Encode engine — all functions require AVX-512 VBMI
// ---------------------------------------------------------------------------
#[allow(unsafe_op_in_unsafe_fn)]
mod encode_engine {
    use super::verify_model::{
        can_run_double_es256, can_run_single_es256, BLOCK_IN_BYTES, BLOCK_OUT_BYTES,
        DOUBLE_ES256_BLOCK_STARTS, DOUBLE_ES256_INPUT_BYTES, DOUBLE_ES256_OUTPUT_BYTES,
        DOUBLE_ES256_PRELOAD_STARTS, ES256_BLOCK_STARTS, ES256_INPUT_BYTES, ES256_OUTPUT_BYTES,
        SINGLE_ES256_REQUIRED_INPUT,
    };
    use super::*;

    /// Encode a single 512-bit block: 48 input bytes -> 64 output bytes.
    ///
    /// The 3-instruction VBMI pipeline:
    /// 1. `permutexvar_epi8(vf, input)` — reshuffle 48 bytes into 64 positions
    /// 2. `multishift_epi64_epi8(vs, reshuffled)` — extract 6-bit fields
    /// 3. `permutexvar_epi8(shifted, vlut)` — map 6-bit indices to Base64 ASCII
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    unsafe fn encode_block_512(input: __m512i, vf: __m512i, vs: __m512i, vlut: __m512i) -> __m512i {
        let reshuffled = _mm512_permutexvar_epi8(vf, input);
        let shifted = _mm512_multishift_epi64_epi8(vs, reshuffled);
        _mm512_permutexvar_epi8(shifted, vlut)
    }

    /// AVX-512 VBMI Base64 encoder.
    ///
    /// Direct port of `tb64v512enc` from `turbob64v512.c`.
    /// Returns `(consumed, written)` byte counts.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX-512 VBMI is available.
    #[inline]
    #[target_feature(enable = "avx512f,avx512bw,avx512vbmi")]
    pub unsafe fn encode_base64_avx512(in_data: &[u8], out_data: &mut [u8]) -> (usize, usize) {
        let inlen = in_data.len();
        let outlen = inlen.div_ceil(3) * 4;
        let mut in_idx = 0usize;
        let mut out_idx = 0usize;

        debug_assert!(out_data.len() >= outlen);

        // Base64 alphabet LUT: maps 6-bit index -> ASCII character
        let vlut = _mm512_setr_epi64(
            d2i(0x4847464544434241), // A B C D E F G H
            d2i(0x504F4E4D4C4B4A49), // I J K L M N O P
            d2i(0x5857565554535251), // Q R S T U V W X
            d2i(0x6665646362615A59), // Y Z a b c d e f
            d2i(0x6E6D6C6B6A696867), // g h i j k l m n
            d2i(0x767574737271706F), // o p q r s t u v
            d2i(0x333231307A797877), // w x y z 0 1 2 3
            d2i(0x2F2B393837363534), // 4 5 6 7 8 9 + /
        );

        // Reshuffle permutation: maps 48 input bytes into 64 positions for
        // the multishift step. Each 8-byte group in the output gets 6 source
        // bytes arranged so that multishift extracts the right 6-bit fields.
        let vf = _mm512_setr_epi32(
            w2i(0x01020001),
            w2i(0x04050304),
            w2i(0x07080607),
            w2i(0x0a0b090a),
            w2i(0x0d0e0c0d),
            w2i(0x10110f10),
            w2i(0x13141213),
            w2i(0x16171516),
            w2i(0x191a1819),
            w2i(0x1c1d1b1c),
            w2i(0x1f201e1f),
            w2i(0x22232122),
            w2i(0x25262425),
            w2i(0x28292728),
            w2i(0x2b2c2a2b),
            w2i(0x2e2f2d2e),
        );

        // Multishift control: extracts 6-bit fields at bit positions
        // 48, 54, 36, 42, 16, 22, 4, 10 within each 64-bit lane.
        let vs = _mm512_set1_epi64(d2i(0x3036242a1016040a));

        // ES256 processes four contiguous 48-byte blocks (192 input -> 256 output).
        // The single drain needs loads starting at 0, 48, 96 and 144, so the
        // farthest 64-byte load touches byte 207.
        if inlen >= SINGLE_ES256_REQUIRED_INPUT {
            let ip = in_data.as_ptr();
            let op = out_data.as_mut_ptr();
            let in_end = ip.add(inlen);

            let mut u0 = _mm512_loadu_si512(ip as *const __m512i);
            let mut u1 = _mm512_loadu_si512(ip.add(BLOCK_IN_BYTES) as *const __m512i);
            let mut op_cur = op;
            let mut ip_cur = ip;

            // Double-ES256 loop: 384 input -> 512 output. The planner keeps
            // block starts contiguous (0, 48, 96, 144, 192, 240, 288, 336) and
            // preloads the next iteration at 384 and 432.
            while can_run_double_es256(remaining(ip_cur, in_end)) {
                let v0 = _mm512_loadu_si512(ip_cur.add(ES256_BLOCK_STARTS[2]) as *const __m512i);
                let v1 = _mm512_loadu_si512(ip_cur.add(ES256_BLOCK_STARTS[3]) as *const __m512i);

                u0 = encode_block_512(u0, vf, vs, vlut);
                u1 = encode_block_512(u1, vf, vs, vlut);
                _mm512_storeu_si512(op_cur as *mut __m512i, u0);
                _mm512_storeu_si512(op_cur.add(BLOCK_OUT_BYTES) as *mut __m512i, u1);

                u0 = _mm512_loadu_si512(ip_cur.add(DOUBLE_ES256_BLOCK_STARTS[4]) as *const __m512i);
                u1 = _mm512_loadu_si512(ip_cur.add(DOUBLE_ES256_BLOCK_STARTS[5]) as *const __m512i);

                let v0 = encode_block_512(v0, vf, vs, vlut);
                let v1 = encode_block_512(v1, vf, vs, vlut);
                _mm512_storeu_si512(op_cur.add(2 * BLOCK_OUT_BYTES) as *mut __m512i, v0);
                _mm512_storeu_si512(op_cur.add(3 * BLOCK_OUT_BYTES) as *mut __m512i, v1);

                let v0 =
                    _mm512_loadu_si512(ip_cur.add(DOUBLE_ES256_BLOCK_STARTS[6]) as *const __m512i);
                let v1 =
                    _mm512_loadu_si512(ip_cur.add(DOUBLE_ES256_BLOCK_STARTS[7]) as *const __m512i);

                u0 = encode_block_512(u0, vf, vs, vlut);
                u1 = encode_block_512(u1, vf, vs, vlut);
                _mm512_storeu_si512(op_cur.add(4 * BLOCK_OUT_BYTES) as *mut __m512i, u0);
                _mm512_storeu_si512(op_cur.add(5 * BLOCK_OUT_BYTES) as *mut __m512i, u1);

                u0 = _mm512_loadu_si512(
                    ip_cur.add(DOUBLE_ES256_PRELOAD_STARTS[0]) as *const __m512i
                );
                u1 = _mm512_loadu_si512(
                    ip_cur.add(DOUBLE_ES256_PRELOAD_STARTS[1]) as *const __m512i
                );

                let v0 = encode_block_512(v0, vf, vs, vlut);
                let v1 = encode_block_512(v1, vf, vs, vlut);
                _mm512_storeu_si512(op_cur.add(6 * BLOCK_OUT_BYTES) as *mut __m512i, v0);
                _mm512_storeu_si512(op_cur.add(7 * BLOCK_OUT_BYTES) as *mut __m512i, v1);

                op_cur = op_cur.add(DOUBLE_ES256_OUTPUT_BYTES);
                ip_cur = ip_cur.add(DOUBLE_ES256_INPUT_BYTES);
            }

            if can_run_single_es256(remaining(ip_cur, in_end)) {
                let v0 = _mm512_loadu_si512(ip_cur.add(ES256_BLOCK_STARTS[2]) as *const __m512i);
                let v1 = _mm512_loadu_si512(ip_cur.add(ES256_BLOCK_STARTS[3]) as *const __m512i);

                u0 = encode_block_512(u0, vf, vs, vlut);
                u1 = encode_block_512(u1, vf, vs, vlut);
                _mm512_storeu_si512(op_cur as *mut __m512i, u0);
                _mm512_storeu_si512(op_cur.add(BLOCK_OUT_BYTES) as *mut __m512i, u1);

                let v0 = encode_block_512(v0, vf, vs, vlut);
                let v1 = encode_block_512(v1, vf, vs, vlut);
                _mm512_storeu_si512(op_cur.add(2 * BLOCK_OUT_BYTES) as *mut __m512i, v0);
                _mm512_storeu_si512(op_cur.add(3 * BLOCK_OUT_BYTES) as *mut __m512i, v1);

                op_cur = op_cur.add(ES256_OUTPUT_BYTES);
                ip_cur = ip_cur.add(ES256_INPUT_BYTES);
            }

            // Compute consumed/written from pointer offsets
            in_idx = ip_cur as usize - ip as usize;
            out_idx = op_cur as usize - op as usize;
        }

        (in_idx, out_idx)
    }
}
