const BASE64_ALPHABET: [u8; 64] =
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const TB64LUT_LE: [u16; 4096] = {
    let mut lut = [0u16; 4096];
    let mut i = 0;
    while i < 4096 {
        let c0 = BASE64_ALPHABET[i & 0x3F] as u16;
        let c1 = BASE64_ALPHABET[(i >> 6) & 0x3F] as u16;
        // little-endian packing: en memoria queda [c0, c1]
        lut[i] = c0 | (c1 << 8);
        i += 1;
    }
    lut
};

#[inline(always)]
const fn enc3_lut(a: u8, b: u8, c: u8) -> u32 {
    let u = ((a as u32) << 16) | ((b as u32) << 8) | (c as u32);
    let idx1 = ((u >> 12) & 0x0FFF) as usize;
    let idx2 = (u & 0x0FFF) as usize;

    let v1 = TB64LUT_LE[idx1] as u32;
    let v2 = TB64LUT_LE[idx2] as u32;

    v1 | (v2 << 16) // listo para store LE (4 bytes)
}

#[inline(always)]
#[rustfmt::skip]
const fn encode_block_48_to_64(inp: &[u8; 48], out: &mut [u8; 64]) {
    // out como 16 chunks fijos de 4: el backend suele bajarlo a stores de u32
    let (out4, rem) = out.as_chunks_mut::<4>();
    debug_assert!(rem.is_empty());

    macro_rules! e {
        ($k:expr) => {{
            let i = $k * 3;
            let x = enc3_lut(inp[i], inp[i + 1], inp[i + 2]);
            out4[$k] = x.to_le_bytes();
        }};
    }

    // Unroll 16×: 48 bytes -> 64 bytes
    e!(0);  e!(1);  e!(2);  e!(3);
    e!(4);  e!(5);  e!(6);  e!(7);
    e!(8);  e!(9);  e!(10); e!(11);
    e!(12); e!(13); e!(14); e!(15);
}

#[inline(always)]
fn encode_scalar_tail(in_data: &[u8], out: &mut [u8]) -> usize {
    let needed = ((in_data.len() + 2) / 3) * 4;
    debug_assert!(out.len() >= needed);

    let out = &mut out[..needed];
    let (in3, rem) = in_data.as_chunks::<3>();
    let (out4, out_rem) = out.as_chunks_mut::<4>();
    debug_assert!(out_rem.is_empty());

    for i in 0..in3.len() {
        let [a, b, c] = in3[i];
        let x = enc3_lut(a, b, c);
        out4[i] = x.to_le_bytes();
    }

    if !rem.is_empty() {
        let o = &mut out4[in3.len()];
        let b0 = rem[0] as u32;

        if rem.len() == 1 {
            let idx = ((b0 << 4) & 0x0FFF) as usize;
            let p = TB64LUT_LE[idx].to_le_bytes();
            o[0] = p[0];
            o[1] = p[1];
            o[2] = b'=';
            o[3] = b'=';
        } else {
            let b1 = rem[1] as u32;
            let idx1 = (((b0 << 4) | (b1 >> 4)) & 0x0FFF) as usize;
            let idx2 = (((b1 & 0x0F) << 8) & 0x0FFF) as usize;

            let p1 = TB64LUT_LE[idx1].to_le_bytes();
            let p2 = TB64LUT_LE[idx2].to_le_bytes();

            o[0] = p1[0];
            o[1] = p1[1];
            o[2] = p2[0];
            o[3] = b'=';
        }
    }

    needed
}

pub fn encode_base64_fast(in_data: &[u8], out_data: &mut [u8]) -> usize {
    let needed = ((in_data.len() + 2) / 3) * 4;
    debug_assert!(out_data.len() >= needed);

    let out = &mut out_data[..needed];

    // Bloques grandes: 48 -> 64
    let (blocks48, rem) = in_data.as_chunks::<48>();
    let blocks_out_len = blocks48.len() * 64;

    // out_blocks como [[u8;64]]
    let (out_blocks, out_tail) = out.split_at_mut(blocks_out_len);
    let (out64, rem_out64) = out_blocks.as_chunks_mut::<64>();
    debug_assert!(rem_out64.is_empty());

    for i in 0..blocks48.len() {
        encode_block_48_to_64(&blocks48[i], &mut out64[i]);
    }

    // Cola (0..47 bytes): scalar, pero ya sobre out_tail exacto
    let tail_written = encode_scalar_tail(rem, out_tail);
    debug_assert_eq!(blocks_out_len + tail_written, needed);

    needed
}
