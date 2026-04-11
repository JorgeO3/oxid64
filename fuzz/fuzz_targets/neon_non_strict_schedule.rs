#![no_main]

use libfuzzer_sys::fuzz_target;

#[cfg(target_arch = "aarch64")]
use oxid64::engine::neon::NeonDecoder;
#[cfg(target_arch = "aarch64")]
use oxid64::engine::scalar::encode_base64_fast;
#[cfg(target_arch = "aarch64")]
use oxid64::engine::DecodeOpts;

#[cfg(target_arch = "aarch64")]
fn expected_checked(encoded_len: usize, pos: usize) -> bool {
    let mut ip = 0usize;

    while encoded_len.saturating_sub(ip) > 256 {
        if pos >= ip && pos < ip + 256 {
            return (pos - ip) / 64 == 3;
        }
        ip += 256;
    }

    while encoded_len.saturating_sub(ip) > 64 {
        if pos >= ip && pos < ip + 64 {
            return true;
        }
        ip += 64;
    }

    true
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
            *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(11);
        }

        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(written);

        let pos = data.get(1).copied().unwrap_or(0) as usize % encoded.len();
        let mut bad = encoded.clone();
        bad[pos] = b'*';

        let mut out = vec![0u8; raw_len + 32];
        let got = NeonDecoder::with_opts(DecodeOpts { strict: false })
            .decode_to_slice(&bad, &mut out[..raw_len]);
        assert_eq!(got.is_none(), expected_checked(encoded.len(), pos));
    }
});
