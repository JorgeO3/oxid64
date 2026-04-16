#![no_main]
#![cfg_attr(not(any(target_arch = "x86", target_arch = "x86_64")), allow(unused))]

use libfuzzer_sys::fuzz_target;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod native {
    use oxid64::engine::DecodeOpts;
    use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
    use oxid64::engine::scalar::encode_base64_fast;

    fn has_backend() -> bool {
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512vbmi")
    }

    fn partial_written_prefix(encoded_len: usize) -> usize {
        let mut ip = 0usize;
        let mut op = 0usize;

        while encoded_len.saturating_sub(ip) > 128 + 2 * 256 + 4 {
            ip += 512;
            op += 384;
        }
        if encoded_len.saturating_sub(ip) > 128 + 256 + 4 {
            ip += 256;
            op += 192;
        }
        while encoded_len.saturating_sub(ip) > 64 + 16 + 4 {
            ip += 64;
            op += 48;
        }
        if op == 0 {
            0
        } else {
            op + 16
        }
    }

    pub(super) fn run(data: &[u8]) {
        if !has_backend() {
            return;
        }

        let raw_len = if data.first().copied().unwrap_or(0) & 1 == 0 {
            294usize
        } else {
            486usize
        };
        let mut raw = vec![0u8; raw_len];
        for (i, slot) in raw.iter_mut().enumerate() {
            *slot = data.get(i).copied().unwrap_or(i as u8).wrapping_mul(23);
        }

        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(written);

        let mut bad = encoded.clone();
        bad[0] = b'*';
        let bound = partial_written_prefix(encoded.len());

        for strict in [true, false] {
            let canary = if strict { 0xA5u8 } else { 0x5Au8 };
            let mut out = vec![canary; raw_len + 33];
            let got = Avx512VbmiDecoder::with_opts(DecodeOpts { strict })
                .decode_to_slice(&bad, &mut out[..raw_len]);
            assert!(got.is_none());
            assert!(out[..bound].iter().any(|&b| b != canary));
            assert!(out[bound..raw_len].iter().all(|&b| b == canary));
        }
    }
}

fuzz_target!(|data: &[u8]| {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    native::run(data);
});
