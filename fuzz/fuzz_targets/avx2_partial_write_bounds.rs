#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::DecodeOpts;

fn partial_written_prefix(encoded_len: usize) -> usize {
    let mut ip = 0usize;
    let mut op = 0usize;

    while encoded_len.saturating_sub(ip) > 64 + 2 * 128 + 4 {
        ip += 256;
        op += 192;
    }
    if encoded_len.saturating_sub(ip) > 64 + 128 + 4 {
        ip += 128;
        op += 96;
    }
    while encoded_len.saturating_sub(ip) > 16 + 4 {
        ip += 16;
        op += 12;
    }
    if op == 0 {
        0
    } else {
        op + 4
    }
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
        *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(21);
    }

    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let written = encode_base64_fast(&raw, &mut encoded);
    encoded.truncate(written);

    let mut bad = encoded.clone();
    bad[0] = b'*';
    let bound = partial_written_prefix(encoded.len());

    for strict in [true, false] {
        let canary = if strict { 0xA5u8 } else { 0x5Au8 };
        let mut out = vec![canary; raw_len + 17];
        let got = Avx2Decoder::with_opts(DecodeOpts { strict })
            .decode_to_slice(&bad, &mut out[..raw_len]);
        assert!(got.is_none());
        assert!(out[..bound].iter().any(|&b| b != canary));
        assert!(out[bound..raw_len].iter().all(|&b| b == canary));
    }
});
