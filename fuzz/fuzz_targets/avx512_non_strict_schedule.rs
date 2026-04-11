#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::DecodeOpts;

#[repr(align(64))]
struct Aligned([u8; 800]);

fn has_backend() -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && std::is_x86_feature_detected!("avx512vbmi")
}

fn expected_checked(encoded_len: usize, pos: usize) -> bool {
    let mut ip = 0usize;

    while encoded_len.saturating_sub(ip) > 128 + 2 * 256 + 4 {
        for base in [0usize, 256] {
            if pos >= ip + base && pos < ip + base + 256 {
                let lane = (pos - (ip + base)) / 64;
                return lane != 1;
            }
        }
        ip += 512;
    }

    if encoded_len.saturating_sub(ip) > 128 + 256 + 4 {
        if pos >= ip && pos < ip + 256 {
            let lane = (pos - ip) / 64;
            return lane != 1;
        }
        ip += 256;
    }

    while encoded_len.saturating_sub(ip) > 64 + 16 + 4 {
        if pos >= ip && pos < ip + 64 {
            return true;
        }
        ip += 64;
    }

    true
}

fuzz_target!(|data: &[u8]| {
    if !cfg!(any(target_arch = "x86", target_arch = "x86_64")) || !has_backend() {
        return;
    }

    let raw_len = if data.first().copied().unwrap_or(0) & 1 == 0 {
        294usize
    } else {
        486usize
    };
    let mut raw = vec![0u8; raw_len];
    for (i, slot) in raw.iter_mut().enumerate() {
        *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(7);
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

    let mut out = Aligned([0u8; 800]);
    let got = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false })
        .decode_to_slice(&bad, &mut out.0[..raw_len]);
    assert_eq!(got.is_none(), expected_checked(encoded.len(), pos));
});
