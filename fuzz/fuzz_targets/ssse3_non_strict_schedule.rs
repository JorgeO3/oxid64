#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::ssse3::Ssse3Decoder;
use oxid64::engine::DecodeOpts;

#[repr(align(16))]
struct Aligned([u8; 160]);

fn expected_checked(encoded_len: usize, pos: usize) -> bool {
    match encoded_len {
        128 => {
            if pos < 64 {
                pos < 16
            } else {
                true
            }
        }
        192 => {
            if pos < 64 {
                pos < 16
            } else if pos < 128 {
                pos < 80
            } else {
                true
            }
        }
        _ => true,
    }
}

fuzz_target!(|data: &[u8]| {
    if !cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    let raw_len = if data.first().copied().unwrap_or(0) & 1 == 0 {
        96
    } else {
        144
    };
    let mut raw = vec![0u8; raw_len];
    for (i, slot) in raw.iter_mut().enumerate() {
        *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(13);
    }

    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let written = encode_base64_fast(&raw, &mut encoded);
    encoded.truncate(written);

    let pad = encoded.iter().rev().take_while(|&&b| b == b'=').count();
    let valid_len = encoded.len().saturating_sub(pad);
    if valid_len == 0 {
        return;
    }

    let pos = data.get(1).copied().unwrap_or(0) as usize % valid_len;
    let mut bad = encoded.clone();
    bad[pos] = b'*';

    let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
    let mut out = Aligned([0u8; 160]);
    let got = dec.decode_to_slice(&bad, &mut out.0[..raw.len()]);
    assert_eq!(got.is_none(), expected_checked(encoded.len(), pos));
});
