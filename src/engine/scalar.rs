//! Scalar Base64 encoder and decoder — Turbo-Base64 port.
//!
//! This module provides a pure-scalar (no SIMD) Base64 codec ported from
//! [Turbo-Base64](https://github.com/powturbo/Turbo-Base64). It is used as:
//!
//! - The **standalone scalar decoder** ([`ScalarDecoder`]) for platforms
//!   without SIMD support.
//! - The **tail fallback** for all SIMD decoders/encoders: after the
//!   vectorised hot loop processes full blocks, this module finishes the
//!   remaining bytes.
//!
//! # Encoder
//!
//! [`encode_base64_fast`] encodes raw bytes to standard Base64 (RFC 4648,
//! alphabet `A-Za-z0-9+/`, `=` padding). It uses a 4 096-entry encode
//! lookup table (`TB64LUT_LE`) that maps every pair of 6-bit indices to
//! two ASCII characters packed into a `u16`, enabling two characters per
//! table hit. The main loop processes 48-byte blocks (→ 64 Base64 chars)
//! with a 16× unrolled macro; a scalar tail handles remainders and padding.
//!
//! # Decoder
//!
//! [`decode_base64_fast`] decodes standard Base64 back to raw bytes. It uses
//! a 4×256-entry decode table (`DEC_LUTS`) where each of the four
//! character positions within a quad has its own 256-entry sub-table,
//! pre-shifted so that ORing the four lookups directly yields the three
//! output bytes packed in a `u32`. Invalid characters map to
//! `INVALID_U32` (`u32::MAX`); this sentinel propagates through the OR
//! chain and is checked once after all blocks are processed.
//!
//! The main loop processes 64-char blocks (→ 48 bytes) with a 16× unrolled
//! macro, 4-block prefetching, and overlapping 4-byte stores. A quad-by-quad
//! tail handles the remainder, and a final 4-char group handles padding.

use super::Base64Decoder;

// ===========================================================================
// Shared constants
// ===========================================================================

/// The standard Base64 alphabet (RFC 4648, Table 1).
const BASE64_ALPHABET: [u8; 64] =
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Sentinel value for invalid Base64 characters in decode tables.
///
/// Any byte whose reverse-lookup yields `0xFF` maps to this value in every
/// `D0`..`D3` sub-table. Because output words are formed by ORing all four
/// sub-table entries, a single invalid character sets all bits, which is
/// detected by `cu == INVALID_U32` after the block loop.
const INVALID_U32: u32 = u32::MAX;

// ===========================================================================
// Encode lookup table
// ===========================================================================

/// Wrapper to force 64-byte (cache-line) alignment on the encode LUT.
#[repr(align(64))]
struct AlignedLut([u32; 4096]);

/// Encode lookup table: 4 096 entries, indexed by a 12-bit value formed from
/// two adjacent 6-bit indices.
///
/// Each entry packs two Base64 ASCII characters into the low 16 bits of a
/// `u32` in little-endian order: `entry = char0 | (char1 << 8)`. A single
/// table hit thus produces two output characters.
///
/// ## Index encoding
///
/// Given three input bytes `(a, b, c)` that produce four 6-bit values
/// `(s0, s1, s2, s3)`:
///
/// - `idx1 = (a << 4) | (b >> 4)` = `(s0 << 6) | s1`
/// - `idx2 = ((b & 0x0F) << 8) | c` = `(s2 << 6) | s3`
///
/// Then `lut[idx1]` gives chars 0–1 and `lut[idx2]` gives chars 2–3.
const TB64LUT_INIT: [u32; 4096] = {
    let mut lut = [0u32; 4096];
    let mut i = 0;
    while i < 4096 {
        let c0 = BASE64_ALPHABET[(i >> 6) & 0x3F] as u32;
        let c1 = BASE64_ALPHABET[i & 0x3F] as u32;
        lut[i] = c0 | (c1 << 8);
        i += 1;
    }
    lut
};

/// The global encode LUT instance, 64-byte aligned for cache-friendly access.
static TB64LUT_LE: AlignedLut = AlignedLut(TB64LUT_INIT);

// ===========================================================================
// Decode lookup tables
// ===========================================================================

/// Reverse mapping: ASCII byte → 6-bit value (0..63), or `0xFF` if invalid.
///
/// Built at compile time from [`BASE64_ALPHABET`]. Used directly by the
/// padding tail decoder ([`decode_tail_3`]) and as the basis for the
/// pre-shifted decode sub-tables `D0`..`D3`.
const REV64_INIT: [u8; 256] = {
    let mut t = [0xFFu8; 256];
    let mut i = 0usize;
    while i < 64 {
        let c = BASE64_ALPHABET[i] as usize;
        t[c] = i as u8;
        i += 1;
    }
    t
};

/// Decode sub-table for position 0 (first char of a Base64 quad).
///
/// `D0[byte] = (6-bit value) << 2`, placing the 6-bit value into the high
/// 6 bits of output byte 0. Invalid bytes map to [`INVALID_U32`].
const D0_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            t[x] = v << 2;
        }
        x += 1;
    }
    t
};

/// Decode sub-table for position 1 (second char of a Base64 quad).
///
/// `D1[byte]` contributes to both byte 0 (low 2 bits) and byte 1 (high
/// 4 bits): `(v >> 4) | ((v & 0x0F) << 12)`. Invalid bytes map to
/// [`INVALID_U32`].
const D1_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            t[x] = (v >> 4) | ((v & 0x0F) << 12);
        }
        x += 1;
    }
    t
};

/// Decode sub-table for position 2 (third char of a Base64 quad).
///
/// `D2[byte]` contributes to byte 1 (low 2 bits) and byte 2 (high 4 bits):
/// `((v >> 2) << 8) | ((v & 0x03) << 22)`. Invalid bytes map to
/// [`INVALID_U32`].
const D2_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            t[x] = ((v >> 2) << 8) | ((v & 0x03) << 22);
        }
        x += 1;
    }
    t
};

/// Decode sub-table for position 3 (fourth char of a Base64 quad).
///
/// `D3[byte] = v << 16`, placing the 6-bit value into the low 6 bits of
/// output byte 2. Invalid bytes map to [`INVALID_U32`].
const D3_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            t[x] = v << 16;
        }
        x += 1;
    }
    t
};

/// Wrapper to force 64-byte (cache-line) alignment on the decode LUT.
#[repr(align(64))]
struct DecLuts {
    table: [u32; 1024],
}

/// Combined decode table: four 256-entry sub-tables concatenated.
///
/// Layout: `table[0..256]` = D0, `table[256..512]` = D1,
/// `table[512..768]` = D2, `table[768..1024]` = D3.
///
/// To decode a quad `(a, b, c, d)`:
/// ```text
/// word = table[a] | table[b + 256] | table[c + 512] | table[d + 768]
/// ```
/// The three output bytes are packed little-endian in `word`.
const DEC_TABLE_INIT: [u32; 1024] = {
    let mut t = [0u32; 1024];
    let mut i = 0;
    while i < 256 {
        t[i] = D0_INIT[i];
        t[i + 256] = D1_INIT[i];
        t[i + 512] = D2_INIT[i];
        t[i + 768] = D3_INIT[i];
        i += 1;
    }
    t
};

/// The global decode LUT instance, 64-byte aligned for cache-friendly access.
static DEC_LUTS: DecLuts = DecLuts {
    table: DEC_TABLE_INIT,
};

// ===========================================================================
// Encode helpers
// ===========================================================================

/// Compute the Base64-encoded length (with padding) for `n` raw bytes.
#[inline(always)]
pub const fn encoded_len(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// Pack two LUT entries (chars 0–1 and chars 2–3) into a single `u32`.
///
/// The result can be written as 4 little-endian bytes to produce the four
/// Base64 ASCII characters for one 3-byte input group.
#[inline(always)]
const fn lut_pack(idx1: usize, idx2: usize) -> u32 {
    let v1 = TB64LUT_LE.0[idx1];
    let v2 = TB64LUT_LE.0[idx2];
    v1 | (v2 << 16)
}

/// Compute LUT index 1 from input bytes `(a, b)`.
///
/// `idx1 = (a << 4) | (b >> 4)` encodes the first two 6-bit values.
#[inline(always)]
const fn idx1_from_abc(a: u8, b: u8) -> usize {
    ((a as usize) << 4) | ((b as usize) >> 4)
}

/// Compute LUT index 2 from input bytes `(b, c)`.
///
/// `idx2 = ((b & 0x0F) << 8) | c` encodes the last two 6-bit values.
#[inline(always)]
const fn idx2_from_abc(b: u8, c: u8) -> usize {
    (((b as usize) & 0x0F) << 8) | (c as usize)
}

/// Encode a 3-byte group `(a, b, c)` into a packed `u32` of 4 Base64 chars.
#[inline(always)]
const fn enc3_lut(a: u8, b: u8, c: u8) -> u32 {
    lut_pack(idx1_from_abc(a, b), idx2_from_abc(b, c))
}

/// Encode a 3-byte group loaded as a big-endian `u32` (top byte unused).
///
/// The input word has bytes `[_, a, b, c]` in big-endian order (bits 23..0).
/// Extracts the two 12-bit LUT indices directly from the 24-bit value.
#[inline(always)]
const fn enc3_lut_u32_be(u: u32) -> u32 {
    let idx1 = (u >> 20) as usize;
    let idx2 = ((u >> 8) & 0x0FFF) as usize;
    lut_pack(idx1, idx2)
}

/// Load a big-endian `u32` from a 48-byte array at offset `i`.
#[inline(always)]
const fn load_u32_be_unaligned(inp: &[u8; 48], i: usize) -> u32 {
    u32::from_be_bytes([inp[i], inp[i + 1], inp[i + 2], inp[i + 3]])
}

// ===========================================================================
// Encode — block and tail functions
// ===========================================================================

/// Encode a 48-byte block into a 64-byte Base64 output block.
///
/// Uses a 16× unrolled macro. Each iteration loads 3 bytes as a big-endian
/// `u32`, extracts two 12-bit LUT indices, and writes 4 ASCII bytes.
/// The last iteration shifts the load left by 8 to handle the final 2-byte
/// overlap at the block boundary.
#[rustfmt::skip]
#[inline(always)]
const fn encode_block_48_to_64(inp: &[u8; 48], out: &mut [u8; 64]) {
    let (out4, rem) = out.as_chunks_mut::<4>();
    debug_assert!(rem.is_empty());

    macro_rules! e32 {
        ($k:expr) => {{
            let i = $k * 3;
            let x = enc3_lut_u32_be(load_u32_be_unaligned(inp, i));
            out4[$k] = x.to_le_bytes();
        }};
    }

    e32!(0);  e32!(1);  e32!(2);  e32!(3);
    e32!(4);  e32!(5);  e32!(6);  e32!(7);
    e32!(8);  e32!(9);  e32!(10); e32!(11);
    e32!(12); e32!(13); e32!(14);
    out4[15] = enc3_lut_u32_be(load_u32_be_unaligned(inp, 44) << 8).to_le_bytes();
}

/// Encode a 1-byte remainder with `==` padding.
#[inline(always)]
const fn encode_tail_rem1(rem0: u8, out4: &mut [u8; 4]) {
    let idx = idx1_from_abc(rem0, 0);
    let p = TB64LUT_LE.0[idx].to_le_bytes();
    out4[0] = p[0];
    out4[1] = p[1];
    out4[2] = b'=';
    out4[3] = b'=';
}

/// Encode a 2-byte remainder with `=` padding.
#[inline(always)]
const fn encode_tail_rem2(rem0: u8, rem1: u8, out4: &mut [u8; 4]) {
    let idx1 = idx1_from_abc(rem0, rem1);
    let idx2 = idx2_from_abc(rem1, 0);
    let p1 = TB64LUT_LE.0[idx1].to_le_bytes();
    let p2 = TB64LUT_LE.0[idx2].to_le_bytes();

    out4[0] = p1[0];
    out4[1] = p1[1];
    out4[2] = p2[0];
    out4[3] = b'=';
}

/// Encode a 3-byte group `(a, b, c)` and write 4 Base64 bytes via raw pointer.
///
/// Used by the SSSE3 encoder's scalar alignment preamble to write output
/// before the output pointer is 16-byte aligned.
///
/// # Safety
///
/// `out` must point to at least 4 writable bytes. The write is unaligned.
///
/// Retained for parity with the C reference; the current SIMD encoders
/// use slice-based paths instead.
#[allow(dead_code)]
#[inline(always)]
pub(crate) unsafe fn encode_block_3_to_4_ptr(a: u8, b: u8, c: u8, out: *mut u8) {
    let x = enc3_lut(a, b, c);
    // SAFETY: caller guarantees `out` points to >= 4 writable bytes.
    unsafe { core::ptr::write_unaligned(out as *mut u32, x.to_le()) };
}

/// Encode a short tail (0..47 bytes) using the scalar 3-byte-at-a-time path.
///
/// Handles remainders after the 48-byte block loop, including final `=`
/// padding for 1-byte and 2-byte remainders.
#[inline(always)]
fn encode_scalar_tail(in_data: &[u8], out: &mut [u8]) -> usize {
    let needed = encoded_len(in_data.len());
    debug_assert!(out.len() >= needed);

    let out = &mut out[..needed];
    let (in3, rem) = in_data.as_chunks::<3>();
    let (out4, out_rem) = out.as_chunks_mut::<4>();
    debug_assert!(out_rem.is_empty());

    for (inp, outp) in in3.iter().zip(out4.iter_mut()) {
        let [a, b, c] = *inp;
        let x = enc3_lut(a, b, c);
        *outp = x.to_le_bytes();
    }

    if !rem.is_empty() {
        let o = &mut out4[in3.len()];
        if rem.len() == 1 {
            encode_tail_rem1(rem[0], o);
        } else {
            encode_tail_rem2(rem[0], rem[1], o);
        }
    }

    needed
}

// ===========================================================================
// Encode — public entry point
// ===========================================================================

/// Encode raw bytes to standard Base64 (RFC 4648).
///
/// Writes the encoded output to `out_data` and returns the number of bytes
/// written (always equal to `ceil(input.len() / 3) * 4`).
///
/// # Panics
///
/// Debug-asserts that `out_data` has sufficient capacity.
///
/// # Algorithm
///
/// The main loop processes 48-byte input blocks (→ 64 Base64 chars) using a
/// 16× unrolled encode block. Pairs of blocks are processed together to
/// improve instruction-level parallelism. A scalar tail handles the final
/// 0..47 bytes including `=` padding.
pub fn encode_base64_fast(in_data: &[u8], out_data: &mut [u8]) -> usize {
    let needed = encoded_len(in_data.len());
    debug_assert!(out_data.len() >= needed);

    let out = &mut out_data[..needed];

    // Main blocks: 48 input bytes -> 64 Base64 chars.
    let (blocks48, rem) = in_data.as_chunks::<48>();
    let blocks_out_len = blocks48.len() * 64;

    let (out_blocks, out_tail) = out.split_at_mut(blocks_out_len);
    let (out64, rem_out64) = out_blocks.as_chunks_mut::<64>();
    debug_assert!(rem_out64.is_empty());

    // Process pairs of 48-byte blocks for better ILP.
    let mut in2 = blocks48.chunks_exact(2);
    let mut out2 = out64.chunks_exact_mut(2);
    for (in_pair, out_pair) in in2.by_ref().zip(out2.by_ref()) {
        encode_block_48_to_64(&in_pair[0], &mut out_pair[0]);
        encode_block_48_to_64(&in_pair[1], &mut out_pair[1]);
    }
    if let (Some(last_in), Some(last_out)) =
        (in2.remainder().first(), out2.into_remainder().first_mut())
    {
        encode_block_48_to_64(last_in, last_out);
    }

    // Tail (0..47 bytes), including final padding.
    let tail_written = encode_scalar_tail(rem, out_tail);
    debug_assert_eq!(blocks_out_len + tail_written, needed);

    needed
}

// ===========================================================================
// Decode helpers
// ===========================================================================

/// Compute the decoded byte length from a Base64 input, accounting for padding.
///
/// Returns `None` if `b64` has an invalid length (not a multiple of 4) or
/// contains an illegal padding pattern (e.g. `x===`).
#[inline(always)]
pub const fn decoded_len_strict(b64: &[u8]) -> Option<usize> {
    let n = b64.len();
    if n == 0 {
        return Some(0);
    }
    if (n & 3) != 0 {
        return None;
    }

    let pad = if b64[n - 1] == b'=' {
        if b64[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };

    // Reject illegal `x===` padding pattern.
    if pad == 2 && b64[n - 3] == b'=' {
        return None;
    }

    Some((n / 4) * 3 - pad)
}

/// Decode a Base64 quad `(a, b, c, d)` into a packed little-endian `u32`.
///
/// The three output bytes are in bits 0..23. Returns [`INVALID_U32`] if any
/// input byte is not a valid Base64 character.
#[inline(always)]
const fn du32(a: u8, b: u8, c: u8, d: u8) -> u32 {
    let x0 = DEC_LUTS.table[a as usize] | DEC_LUTS.table[(b as usize) | 256];
    let x1 = DEC_LUTS.table[(c as usize) | 512] | DEC_LUTS.table[(d as usize) | 768];
    x0 | x1
}

/// Decode a Base64 quad packed as a little-endian `u32` word.
///
/// Splits the word into four bytes, looks up each in the appropriate
/// sub-table, and ORs the results. The four lookups are structured as
/// two independent pairs to allow superscalar execution:
/// `(x0 | x1) | (x2 | x3)` avoids a serial OR chain.
#[inline(always)]
const fn du32_u32_le(word: u32) -> u32 {
    let a = (word & 0xFF) as usize;
    let b = ((word >> 8) & 0xFF) as usize | 256;
    let c = ((word >> 16) & 0xFF) as usize | 512;
    let d = ((word >> 24) & 0xFF) as usize | 768;

    let t = &DEC_LUTS.table;

    let x0 = t[a];
    let x1 = t[b];
    let x2 = t[c];
    let x3 = t[d];

    (x0 | x1) | (x2 | x3)
}

/// Write a `u32` as 4 little-endian bytes via an unaligned pointer store.
///
/// # Safety
///
/// `p` must point to at least 4 writable bytes. The write is unaligned.
#[inline(always)]
unsafe fn write_u32_le_unaligned(p: *mut u8, v: u32) {
    // SAFETY: caller guarantees `p` points to >= 4 writable bytes.
    unsafe { core::ptr::write_unaligned(p as *mut u32, v.to_le()) };
}

/// Issue a software prefetch for data 384 bytes ahead of `p`.
///
/// Used in the 4-block decode loop to prefetch the next cache line of input
/// data. On non-x86 targets this is a no-op.
#[allow(unused_variables)]
#[inline(always)]
fn prefetch_t0_384(p: *const u8) {
    #[cfg(target_arch = "x86")]
    // SAFETY: _mm_prefetch is a hint; incorrect addresses are harmless.
    unsafe {
        core::arch::x86::_mm_prefetch(
            p.wrapping_add(384) as *const i8,
            core::arch::x86::_MM_HINT_T0,
        );
    }
    #[cfg(target_arch = "x86_64")]
    // SAFETY: _mm_prefetch is a hint; incorrect addresses are harmless.
    unsafe {
        core::arch::x86_64::_mm_prefetch(
            p.wrapping_add(384) as *const i8,
            core::arch::x86_64::_MM_HINT_T0,
        );
    }
}

// ===========================================================================
// Decode — block and body functions
// ===========================================================================

/// Decode a 64-byte Base64 block into 48 output bytes.
///
/// Uses a 16× unrolled macro. Each iteration decodes one 4-char quad into
/// 3 bytes via [`du32_u32_le`] and writes the result as an overlapping
/// 4-byte store (the extra byte is overwritten by the next iteration).
/// The last quad writes exactly 3 bytes to avoid overflowing the output.
///
/// Invalid-character detection is deferred: the OR of all decoded words is
/// accumulated into `cu`, and the caller checks `cu == INVALID_U32` after
/// all blocks are processed.
#[rustfmt::skip]
#[inline(always)]
fn decode_block_64_to_48(inp: &[u8; 64], out: &mut [u8; 48], cu: &mut u32) {
    let (in4, in4_rem) = inp.as_chunks::<4>();
    debug_assert!(in4_rem.is_empty());
    let op = out.as_mut_ptr();

    macro_rules! q4 {
        ($k:expr) => {{
            let u = du32_u32_le(u32::from_le_bytes(in4[$k]));
            *cu |= u;
            let o = $k * 3;
            // SAFETY: for k in 0..=14, o + 4 <= 46. The overlapping write
            // pattern is intentional — the next iteration overwrites the
            // extra byte. `op` points into `out` which is a &mut [u8; 48].
            unsafe { write_u32_le_unaligned(op.add(o), u) };
        }};
    }

    q4!(0);  q4!(1);  q4!(2);  q4!(3);
    q4!(4);  q4!(5);  q4!(6);  q4!(7);
    q4!(8);  q4!(9);  q4!(10); q4!(11);
    q4!(12); q4!(13); q4!(14);

    // Last quad: write exactly 3 bytes (no overlap beyond the buffer).
    let u = du32_u32_le(u32::from_le_bytes(in4[15]));
    *cu |= u;
    let bb = u.to_le_bytes();
    out[45] = bb[0];
    out[46] = bb[1];
    out[47] = bb[2];
}

/// Decode a sequence of 64-byte blocks into corresponding 48-byte outputs.
///
/// Processes blocks in groups of 4 with prefetching for the next cache line,
/// then handles any remaining blocks individually.
fn decode_body_blocks_64_to_48(chunks64: &[[u8; 64]], out48: &mut [[u8; 48]], cu: &mut u32) {
    debug_assert_eq!(chunks64.len(), out48.len());
    let mut in4 = chunks64.chunks_exact(4);
    let mut out4 = out48.chunks_exact_mut(4);
    for (in_pack, out_pack) in in4.by_ref().zip(out4.by_ref()) {
        prefetch_t0_384(in_pack.as_ptr() as *const u8);
        decode_block_64_to_48(&in_pack[0], &mut out_pack[0], cu);
        decode_block_64_to_48(&in_pack[1], &mut out_pack[1], cu);
        decode_block_64_to_48(&in_pack[2], &mut out_pack[2], cu);
        decode_block_64_to_48(&in_pack[3], &mut out_pack[3], cu);
    }

    for (inp, outp) in in4.remainder().iter().zip(out4.into_remainder().iter_mut()) {
        decode_block_64_to_48(inp, outp, cu);
    }
}

// ===========================================================================
// Decode — tail functions
// ===========================================================================

/// Decode the final 4-byte Base64 group, handling `=` padding.
///
/// Writes 1, 2, or 3 bytes into `out3` depending on padding:
/// - No padding (`d != '='`): 4 chars → 3 bytes.
/// - One `=` (`c != '='`): 3 chars → 2 bytes.
/// - Two `=`s: 2 chars → 1 byte.
///
/// Returns `Some((bytes_written, decoded_word))` on success, or `None` if
/// any non-padding character is invalid. The `decoded_word` is included so
/// callers can OR it into the error-detection accumulator `cu`.
#[inline(always)]
pub(crate) const fn decode_tail_3(last4: &[u8; 4], out3: &mut [u8; 3]) -> Option<(usize, u32)> {
    let a = last4[0];
    let b = last4[1];
    let c = last4[2];
    let d = last4[3];

    // No padding: 4 chars -> 3 bytes.
    if d != b'=' {
        let u = du32(a, b, c, d);
        if u == INVALID_U32 {
            return None;
        }
        out3[0] = (u & 0xFF) as u8;
        out3[1] = ((u >> 8) & 0xFF) as u8;
        out3[2] = ((u >> 16) & 0xFF) as u8;
        return Some((3, u));
    }

    // One '=' padding: 3 chars -> 2 bytes.
    if c != b'=' {
        let v0 = REV64_INIT[a as usize];
        let v1 = REV64_INIT[b as usize];
        let v2 = REV64_INIT[c as usize];
        if (v0 | v1 | v2) == 0xFF {
            return None;
        }

        let o0 = ((v0 as u32) << 2) | ((v1 as u32) >> 4);
        let o1 = (((v1 as u32) & 0x0F) << 4) | ((v2 as u32) >> 2);

        out3[0] = o0 as u8;
        out3[1] = o1 as u8;
        return Some((2, o0 | (o1 << 8)));
    }

    // Two '=' padding: 2 chars -> 1 byte.
    let v0 = REV64_INIT[a as usize];
    let v1 = REV64_INIT[b as usize];
    if (v0 | v1) == 0xFF {
        return None;
    }

    let o0 = ((v0 as u32) << 2) | ((v1 as u32) >> 4);
    out3[0] = o0 as u8;
    Some((1, o0))
}

/// Decode the final 4-byte group into an output slice at a given offset.
///
/// Thin wrapper around [`decode_tail_3`] that copies only the written bytes
/// into `out[out_off..]`. The intermediate 3-byte buffer avoids bounds-check
/// noise in the hot caller.
#[inline(always)]
fn decode_tail(last4: &[u8; 4], out: &mut [u8], out_off: usize) -> Option<(usize, u32)> {
    let mut tmp = [0u8; 3];
    let (written, cu) = decode_tail_3(last4, &mut tmp)?;

    out[out_off] = tmp[0];
    if written >= 2 {
        out[out_off + 1] = tmp[1];
    }
    if written >= 3 {
        out[out_off + 2] = tmp[2];
    }

    Some((written, cu))
}

// ===========================================================================
// Decode — public entry point
// ===========================================================================

/// Decode standard Base64 (RFC 4648) to raw bytes.
///
/// Writes decoded output to `out_data` and returns the number of bytes
/// written, or `None` if the input is invalid (bad length, illegal padding,
/// or non-Base64 characters).
///
/// # Panics
///
/// Debug-asserts that `out_data` has sufficient capacity.
///
/// # Algorithm
///
/// 1. **Block loop**: 64-char blocks (→ 48 bytes) via `decode_block_64_to_48`,
///    processed 4 at a time with prefetching.
/// 2. **Quad loop**: remaining 4-char groups (→ 3 bytes each).
/// 3. **Tail**: the final 4 chars, handling `=` padding.
///
/// Error detection is deferred: all decoded words are ORed into an
/// accumulator `cu`, and `cu == INVALID_U32` is checked once at the end.
pub fn decode_base64_fast(in_data: &[u8], out_data: &mut [u8]) -> Option<usize> {
    let out_len = decoded_len_strict(in_data)?;
    if out_data.len() < out_len {
        return None;
    }
    if in_data.is_empty() {
        return Some(0);
    }

    let n = in_data.len();
    let body = &in_data[..n - 4];
    let last4: &[u8; 4] = in_data[n - 4..].try_into().ok()?;
    let out = &mut out_data[..out_len];

    let mut cu = 0u32;

    // --- Phase 1: 64-char blocks -> 48-byte outputs ---
    let (chunks64, rem64) = body.as_chunks::<64>();
    let body_out_len = chunks64.len() * 48;

    let (out_body, out_tail) = out.split_at_mut(body_out_len);
    let (out48, rem_out48) = out_body.as_chunks_mut::<48>();
    debug_assert!(rem_out48.is_empty());

    decode_body_blocks_64_to_48(chunks64, out48, &mut cu);

    // --- Phase 2: remaining body quads (< 64 chars) ---
    let (rem4, rem4_tail) = rem64.as_chunks::<4>();
    debug_assert!(rem4_tail.is_empty());

    let used = rem4.len() * 3;
    debug_assert!(used <= out_tail.len());

    let (out_rem, out_tail2) = out_tail.split_at_mut(used);
    debug_assert_eq!(out_rem.len(), rem4.len() * 3);

    for (quad, dst3) in rem4.iter().zip(out_rem.chunks_exact_mut(3)) {
        let u = du32_u32_le(u32::from_le_bytes(*quad));
        cu |= u;

        dst3[0] = (u & 0xFF) as u8;
        dst3[1] = ((u >> 8) & 0xFF) as u8;
        dst3[2] = ((u >> 16) & 0xFF) as u8;
    }

    // --- Phase 3: final 4-char group (with possible padding) ---
    let (tail_written, cu_tail) = decode_tail(last4, out_tail2, 0)?;
    cu |= cu_tail;

    if cu == INVALID_U32 {
        None
    } else {
        debug_assert_eq!(body_out_len + used + tail_written, out_len);
        Some(out_len)
    }
}

// ===========================================================================
// ScalarDecoder — trait implementation
// ===========================================================================

/// Pure-scalar Base64 decoder/encoder.
///
/// Used as the fallback on platforms without SIMD support. Implements
/// [`Base64Decoder`] using [`decode_base64_fast`] and [`encode_base64_fast`].
pub struct ScalarDecoder;

impl Base64Decoder for ScalarDecoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        decode_base64_fast(input, out)
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        encode_base64_fast(input, out)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::{decode_base64_fast, decoded_len_strict, encode_base64_fast};

    /// Fill a buffer with deterministic pseudo-random bytes (xorshift64).
    fn fill_xorshift(buf: &mut [u8]) {
        let mut x = 0x1234_5678_9abc_def0_u64;
        for b in buf {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }

    #[test]
    fn matches_rfc4648_vectors() {
        const CASES: [(&[u8], &str); 7] = [
            (b"", ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];

        for (input, expected) in CASES {
            let mut out = vec![0u8; input.len().div_ceil(3) * 4 + 8];
            let written = encode_base64_fast(input, &mut out);
            assert_eq!(&out[..written], expected.as_bytes(), "encode mismatch");

            let mut decoded = vec![0u8; input.len() + 8];
            let decoded_len =
                decode_base64_fast(expected.as_bytes(), &mut decoded).expect("decode failed");
            assert_eq!(decoded_len, input.len(), "decode len mismatch");
            assert_eq!(&decoded[..decoded_len], input, "decode mismatch");
        }
    }

    #[test]
    fn decode_roundtrip_many_sizes() {
        let max_len = if cfg!(miri) { 1024usize } else { 8192usize };
        for len in 0..=max_len {
            let mut input = vec![0u8; len];
            fill_xorshift(&mut input);

            let mut encoded = vec![0u8; len.div_ceil(3) * 4 + 8];
            let enc_written = encode_base64_fast(&input, &mut encoded);
            let encoded = &encoded[..enc_written];

            let expected_len = decoded_len_strict(encoded).expect("decoded_len_strict failed");
            assert_eq!(expected_len, len, "decoded_len mismatch at len={len}");

            let mut out = vec![0u8; len + 8];
            let written = decode_base64_fast(encoded, &mut out).expect("decode failed");
            assert_eq!(written, len, "len mismatch at len={len}");
            assert_eq!(&out[..written], input.as_slice(), "mismatch at len={len}");
        }
    }

    #[test]
    fn decode_rejects_invalid() {
        let mut out = [0u8; 64];
        assert_eq!(decoded_len_strict(b""), Some(0));
        assert!(decode_base64_fast(b"A", &mut out).is_none());
        assert!(decode_base64_fast(b"AAA", &mut out).is_none());
        assert!(decode_base64_fast(b"AAAAA", &mut out).is_none());
        assert!(decode_base64_fast(b"A===", &mut out).is_none());
        assert!(decode_base64_fast(b"AA*A", &mut out).is_none());
        assert!(decode_base64_fast(b"AA=A", &mut out).is_none());
    }
}
