#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::DecodeOpts;

#[repr(align(32))]
struct Aligned([u8; 400]);

fn expected_checked(encoded_len: usize, pos: usize) -> bool {
    let mut ip = 0usize;

    while encoded_len.saturating_sub(ip) > 64 + 2 * 128 + 4 {
        for base in [0usize, 128] {
            if pos >= ip + base && pos < ip + base + 128 {
                let lane = (pos - (ip + base)) / 32;
                return lane != 1;
            }
        }
        ip += 256;
    }

    if encoded_len.saturating_sub(ip) > 64 + 128 + 4 {
        if pos >= ip && pos < ip + 128 {
            let lane = (pos - ip) / 32;
            return lane != 1;
        }
        ip += 128;
    }

    while encoded_len.saturating_sub(ip) > 16 + 4 {
        if pos >= ip && pos < ip + 16 {
            return true;
        }
        ip += 16;
    }

    true
}

fuzz_target!(|data: &[u8]| {
    if !cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    let raw_len = match data.first().copied().unwrap_or(0) % 3 {
        0 => 150usize,
        1 => 246usize,
        _ => 342usize,
    };
    let mut raw = vec![0u8; raw_len];
    for (i, slot) in raw.iter_mut().enumerate() {
        *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(9);
    }

    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let written = encode_base64_fast(&raw, &mut encoded);
    encoded.truncate(written);
    if encoded.is_empty() {
        return;
    }

    let pos = data.get(1).copied().unwrap_or(0) as usize % encoded.len();
    if encoded[pos] == b'=' {
        return;
    }

    let mut bad = encoded.clone();
    bad[pos] = b'*';

    let mut out = Aligned([0u8; 400]);
    let got = Avx2Decoder::with_opts(DecodeOpts { strict: false })
        .decode_to_slice(&bad, &mut out.0[..raw_len]);
    assert_eq!(got.is_none(), expected_checked(encoded.len(), pos));
});
