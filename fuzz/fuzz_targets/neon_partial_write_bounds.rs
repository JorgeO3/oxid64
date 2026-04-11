#![no_main]

use libfuzzer_sys::fuzz_target;

#[cfg(target_arch = "aarch64")]
use oxid64::engine::neon::NeonDecoder;
#[cfg(target_arch = "aarch64")]
use oxid64::engine::scalar::encode_base64_fast;
#[cfg(target_arch = "aarch64")]
use oxid64::engine::DecodeOpts;

#[cfg(target_arch = "aarch64")]
fn partial_written_prefix(encoded_len: usize) -> usize {
    let mut ip = 0usize;
    let mut op = 0usize;

    while encoded_len.saturating_sub(ip) > 256 {
        ip += 256;
        op += 192;
    }

    while encoded_len.saturating_sub(ip) > 64 {
        ip += 64;
        op += 48;
    }

    op
}

fuzz_target!(|data: &[u8]| {
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = data;
        return;
    }

    #[cfg(target_arch = "aarch64")]
    {
        if !std::arch::is_aarch64_feature_detected!("neon") {
            return;
        }

        let raw_len = if data.first().copied().unwrap_or(0) & 1 == 0 {
            240usize
        } else {
            432usize
        };
        let mut raw = vec![0u8; raw_len];
        for (i, slot) in raw.iter_mut().enumerate() {
            *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(23);
        }

        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(written);

        let mut bad = encoded.clone();
        bad[192] = if data.get(1).copied().unwrap_or(0) & 1 == 0 {
            b'*'
        } else {
            0xFF
        };
        let bound = partial_written_prefix(encoded.len());

        for strict in [true, false] {
            let canary = if strict { 0xA5u8 } else { 0x5Au8 };
            let mut out = vec![canary; raw_len + 17];
            let got = NeonDecoder::with_opts(DecodeOpts { strict })
                .decode_to_slice(&bad, &mut out[..raw_len]);
            assert!(got.is_none());
            assert!(out[..bound].iter().any(|&b| b != canary));
            assert!(out[bound..raw_len].iter().all(|&b| b == canary));
        }
    }
});
