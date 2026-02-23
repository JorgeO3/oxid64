const BASE64_ALPHABET: [u8; 64] =
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[repr(align(64))]
struct AlignedLut([u16; 4096]);

const TB64LUT_INIT: [u16; 4096] = {
    let mut lut = [0u16; 4096];
    let mut i = 0;
    while i < 4096 {
        let c0 = BASE64_ALPHABET[(i >> 6) & 0x3F] as u16;
        let c1 = BASE64_ALPHABET[i & 0x3F] as u16;
        // little-endian packing: in memory this is [first, second]
        lut[i] = c0 | (c1 << 8);
        i += 1;
    }
    lut
};

static TB64LUT_LE: AlignedLut = AlignedLut(TB64LUT_INIT);

#[inline(always)]
const fn encoded_len(n: usize) -> usize {
    ((n + 2) / 3) * 4
}

#[inline(always)]
const fn lut_pack(idx1: usize, idx2: usize) -> u32 {
    let v1 = TB64LUT_LE.0[idx1] as u32;
    let v2 = TB64LUT_LE.0[idx2] as u32;
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

#[cfg(test)]
mod tests {
    use super::encode_base64_fast;
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
}
