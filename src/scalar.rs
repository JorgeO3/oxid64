const BASE64_ALPHABET: [u8; 64] =
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[repr(align(64))]
struct AlignedLut([u32; 4096]);

#[repr(align(64))]
struct DecLuts {
    table: [u32; 1024],
}

const TB64LUT_INIT: [u32; 4096] = {
    let mut lut = [0u32; 4096];
    let mut i = 0;
    while i < 4096 {
        let c0 = BASE64_ALPHABET[(i >> 6) & 0x3F] as u32;
        let c1 = BASE64_ALPHABET[i & 0x3F] as u32;
        // little-endian packing: in memory this is [first, second]
        lut[i] = c0 | (c1 << 8);
        i += 1;
    }
    lut
};

static TB64LUT_LE: AlignedLut = AlignedLut(TB64LUT_INIT);

const INVALID_U32: u32 = u32::MAX;

// 0..63 for valid chars, 0xFF for invalid.
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

const D0_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            // v0 contributes to byte0: (v0 << 2)
            t[x] = v << 2;
        }
        x += 1;
    }
    t
};

const D1_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            // v1 contributes to byte0 and byte1.
            t[x] = (v >> 4) | ((v & 0x0F) << 12);
        }
        x += 1;
    }
    t
};

const D2_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            // v2 contributes to byte1 and byte2.
            t[x] = ((v >> 2) << 8) | ((v & 0x03) << 22);
        }
        x += 1;
    }
    t
};

const D3_INIT: [u32; 256] = {
    let mut t = [INVALID_U32; 256];
    let mut x = 0usize;
    while x < 256 {
        let v = REV64_INIT[x] as u32;
        if v != 0xFF {
            // v3 contributes to byte2.
            t[x] = v << 16;
        }
        x += 1;
    }
    t
};

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

static DEC_LUTS: DecLuts = DecLuts {
    table: DEC_TABLE_INIT,
};

#[inline(always)]
const fn encoded_len(n: usize) -> usize {
    ((n + 2) / 3) * 4
}

#[inline(always)]
const fn lut_pack(idx1: usize, idx2: usize) -> u32 {
    let v1 = TB64LUT_LE.0[idx1];
    let v2 = TB64LUT_LE.0[idx2];
    v1 | (v2 << 16) // listo para store LE (4 bytes)
}

#[inline(always)]
const fn idx1_from_abc(a: u8, b: u8) -> usize {
    ((a as usize) << 4) | ((b as usize) >> 4)
}

#[inline(always)]
const fn idx2_from_abc(b: u8, c: u8) -> usize {
    (((b as usize) & 0x0F) << 8) | (c as usize)
}

#[inline(always)]
const fn enc3_lut(a: u8, b: u8, c: u8) -> u32 {
    lut_pack(idx1_from_abc(a, b), idx2_from_abc(b, c))
}

#[inline(always)]
const fn enc3_lut_u32_be(u: u32) -> u32 {
    let idx1 = (u >> 20) as usize;
    let idx2 = ((u >> 8) & 0x0FFF) as usize;
    lut_pack(idx1, idx2)
}

#[inline(always)]
const fn load_u32_be_unaligned(inp: &[u8; 48], i: usize) -> u32 {
    u32::from_be_bytes([inp[i], inp[i + 1], inp[i + 2], inp[i + 3]])
}

#[inline(always)]
#[rustfmt::skip]
const fn encode_block_48_to_64(inp: &[u8; 48], out: &mut [u8; 64]) {
    // out as 16 fixed 4-byte chunks; the backend lowers this to u32 stores.
    let (out4, rem) = out.as_chunks_mut::<4>();
    debug_assert!(rem.is_empty());

    macro_rules! e32 {
        ($k:expr) => {{
            let i = $k * 3;
            let x = enc3_lut_u32_be(load_u32_be_unaligned(inp, i));
            out4[$k] = x.to_le_bytes();
        }};
    }

    // Unroll 16x: 48 bytes -> 64 bytes.
    e32!(0);  e32!(1);  e32!(2);  e32!(3);
    e32!(4);  e32!(5);  e32!(6);  e32!(7);
    e32!(8);  e32!(9);  e32!(10); e32!(11);
    e32!(12); e32!(13); e32!(14);
    out4[15] = enc3_lut_u32_be(load_u32_be_unaligned(inp, 44) << 8).to_le_bytes();
}

#[inline(always)]
fn encode_tail_rem1(rem0: u8, out4: &mut [u8; 4]) {
    let idx = idx1_from_abc(rem0, 0);
    let p = TB64LUT_LE.0[idx].to_le_bytes();
    out4[0] = p[0];
    out4[1] = p[1];
    out4[2] = b'=';
    out4[3] = b'=';
}

#[inline(always)]
fn encode_tail_rem2(rem0: u8, rem1: u8, out4: &mut [u8; 4]) {
    let idx1 = idx1_from_abc(rem0, rem1);
    let idx2 = idx2_from_abc(rem1, 0);
    let p1 = TB64LUT_LE.0[idx1].to_le_bytes();
    let p2 = TB64LUT_LE.0[idx2].to_le_bytes();

    out4[0] = p1[0];
    out4[1] = p1[1];
    out4[2] = p2[0];
    out4[3] = b'=';
}

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

pub fn encode_base64_fast(in_data: &[u8], out_data: &mut [u8]) -> usize {
    let needed = encoded_len(in_data.len());
    debug_assert!(out_data.len() >= needed);

    let out = &mut out_data[..needed];

    // Main blocks: 48 -> 64.
    let (blocks48, rem) = in_data.as_chunks::<48>();
    let blocks_out_len = blocks48.len() * 64;

    let (out_blocks, out_tail) = out.split_at_mut(blocks_out_len);
    let (out64, rem_out64) = out_blocks.as_chunks_mut::<64>();
    debug_assert!(rem_out64.is_empty());

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

    // Reject x=== style tails.
    if pad == 2 && b64[n - 3] == b'=' {
        return None;
    }

    Some((n / 4) * 3 - pad)
}

#[inline(always)]
const fn du32(a: u8, b: u8, c: u8, d: u8) -> u32 {
    let x0 = DEC_LUTS.table[a as usize] | DEC_LUTS.table[(b as usize) | 256];
    let x1 = DEC_LUTS.table[(c as usize) | 512] | DEC_LUTS.table[(d as usize) | 768];
    x0 | x1
}

#[inline(always)]
const fn du32_u32_le(word: u32) -> u32 {
    let a = (word & 0xFF) as usize;
    let b = ((word >> 8) & 0xFF) as usize;
    let c = ((word >> 16) & 0xFF) as usize;
    let d = ((word >> 24) as u8) as usize;
    // Forzando el offset con OR le dice a LLVM que los limites están en 0..1023
    let x0 = DEC_LUTS.table[a] | DEC_LUTS.table[b | 256];
    let x1 = DEC_LUTS.table[c | 512] | DEC_LUTS.table[d | 768];
    x0 | x1
}

#[inline(always)]
unsafe fn write_u32_le_unaligned(p: *mut u8, v: u32) {
    unsafe { core::ptr::write_unaligned(p as *mut u32, v.to_le()) };
}

#[inline(always)]
fn prefetch_t0_384(p: *const u8) {
    #[cfg(target_arch = "x86")]
    unsafe {
        core::arch::x86::_mm_prefetch(
            p.wrapping_add(384) as *const i8,
            core::arch::x86::_MM_HINT_T0,
        );
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::x86_64::_mm_prefetch(
            p.wrapping_add(384) as *const i8,
            core::arch::x86_64::_MM_HINT_T0,
        );
    }
}

#[inline(always)]
#[rustfmt::skip]
fn decode_block_64_to_48(inp: &[u8; 64], out: &mut [u8; 48], cu: &mut u32) {
    let (in4, in4_rem) = inp.as_chunks::<4>();
    debug_assert!(in4_rem.is_empty());
    let op = out.as_mut_ptr();

    macro_rules! q4 {
        ($k:expr) => {{
            let u = du32_u32_le(u32::from_le_bytes(in4[$k]));
            *cu |= u;
            let o = $k * 3;
            // SAFETY: for k in 0..=14, o+4 <= 46 and this overlapping write pattern
            // is intentional (next group overwrites the extra byte).
            unsafe { write_u32_le_unaligned(op.add(o), u) };
        }};
    }

    q4!(0);  q4!(1);  q4!(2);  q4!(3);
    q4!(4);  q4!(5);  q4!(6);  q4!(7);
    q4!(8);  q4!(9);  q4!(10); q4!(11);
    q4!(12); q4!(13); q4!(14);

    let u = du32_u32_le(u32::from_le_bytes(in4[15]));
    *cu |= u;
    let bb = u.to_le_bytes();
    out[45] = bb[0];
    out[46] = bb[1];
    out[47] = bb[2];
}

#[inline(always)]
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

#[inline(always)]
const fn decode_tail(last4: &[u8; 4], out: &mut [u8], out_off: usize) -> Option<(usize, u32)> {
    let a = last4[0];
    let b = last4[1];
    let c = last4[2];
    let d = last4[3];

    // No padding (4 -> 3).
    if d != b'=' {
        let u = du32(a, b, c, d);
        if u == INVALID_U32 {
            return None;
        }
        let bb = u.to_le_bytes();
        out[out_off] = bb[0];
        out[out_off + 1] = bb[1];
        out[out_off + 2] = bb[2];
        return Some((3, u));
    }

    // One padding (3 -> 2): xxx=
    if c != b'=' {
        let v0 = REV64_INIT[a as usize];
        let v1 = REV64_INIT[b as usize];
        let v2 = REV64_INIT[c as usize];
        if (v0 | v1 | v2) == 0xFF {
            return None;
        }

        let o0 = ((v0 as u32) << 2) | ((v1 as u32) >> 4);
        let o1 = (((v1 as u32) & 0x0F) << 4) | ((v2 as u32) >> 2);

        out[out_off] = o0 as u8;
        out[out_off + 1] = o1 as u8;
        return Some((2, o0 | (o1 << 8)));
    }

    // Two padding (2 -> 1): xx==
    let v0 = REV64_INIT[a as usize];
    let v1 = REV64_INIT[b as usize];
    if (v0 | v1) == 0xFF {
        return None;
    }

    let o0 = ((v0 as u32) << 2) | ((v1 as u32) >> 4);
    out[out_off] = o0 as u8;
    Some((1, o0))
}

pub fn decode_base64_fast(in_data: &[u8], out_data: &mut [u8]) -> Option<usize> {
    let out_len = decoded_len_strict(in_data)?;
    if out_data.len() < out_len {
        return None;
    }
    if in_data.is_empty() {
        return Some(0);
    }

    // Process everything except final 4 chars with no-padding path.
    let n = in_data.len();
    let body = &in_data[..n - 4];
    let last4: &[u8; 4] = in_data[n - 4..].try_into().ok()?;
    let out = &mut out_data[..out_len];

    let mut cu = 0u32;

    // Main blocks: 64 chars -> 48 bytes.
    let (chunks64, rem64) = body.as_chunks::<64>();
    let body_out_len = chunks64.len() * 48;

    let (out_body, out_tail) = out.split_at_mut(body_out_len);
    let (out48, rem_out48) = out_body.as_chunks_mut::<48>();
    debug_assert!(rem_out48.is_empty());

    decode_body_blocks_64_to_48(chunks64, out48, &mut cu);

    // Remaining body quads (<64 chars).
    let mut out_off = body_out_len;
    let (rem4, rem4_tail) = rem64.as_chunks::<4>();
    debug_assert!(rem4_tail.is_empty());
    for quad in rem4 {
        let u = du32_u32_le(u32::from_le_bytes(*quad));
        cu |= u;

        let bb = u.to_le_bytes();
        let o = out_off - body_out_len;
        out_tail[o] = bb[0];
        out_tail[o + 1] = bb[1];
        out_tail[o + 2] = bb[2];
        out_off += 3;
    }

    // Final quad with padding rules.
    let (tail_written, cu_tail) = decode_tail(last4, out, out_off)?;
    cu |= cu_tail;
    out_off += tail_written;

    if cu == INVALID_U32 {
        None
    } else {
        debug_assert_eq!(out_off, out_len);
        Some(out_len)
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
    use base64::{Engine as _, engine::general_purpose::STANDARD as b64_std};

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
    fn matches_standard_for_many_sizes() {
        for len in 0..=8192usize {
            let mut input = vec![0u8; len];
            fill_xorshift(&mut input);

            let mut out = vec![0u8; ((len + 2) / 3) * 4 + 8];
            let written = encode_base64_fast(&input, &mut out);
            let expected = b64_std.encode(&input);

            assert_eq!(
                &out[..written],
                expected.as_bytes(),
                "mismatch at len={len}"
            );
        }
    }

    #[test]
    fn decode_roundtrip_many_sizes() {
        for len in 0..=8192usize {
            let mut input = vec![0u8; len];
            fill_xorshift(&mut input);

            let encoded = b64_std.encode(&input);
            let mut out = vec![0u8; len + 8];
            let written = decode_base64_fast(encoded.as_bytes(), &mut out).expect("decode failed");
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
