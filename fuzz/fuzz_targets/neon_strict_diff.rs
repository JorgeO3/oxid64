#![no_main]
#![cfg_attr(not(target_arch = "aarch64"), allow(unused))]

use libfuzzer_sys::fuzz_target;

#[cfg(target_arch = "aarch64")]
mod native {
    use oxid64::engine::neon::NeonDecoder;
    use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};

    pub(super) fn run(data: &[u8]) {
        if !std::arch::is_aarch64_feature_detected!("neon") {
            return;
        }

        let raw = &data[..data.len().min(4096)];
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let written = encode_base64_fast(raw, &mut encoded);
        encoded.truncate(written);
        if encoded.is_empty() {
            return;
        }

        let pad = encoded.iter().rev().take_while(|&&b| b == b'=').count();
        let valid_len = encoded.len().saturating_sub(pad);
        if valid_len == 0 {
            return;
        }

        let idx = data.len().wrapping_mul(31) % valid_len;
        let mut bad = encoded.clone();
        bad[idx] = if idx & 1 == 0 { b'*' } else { 0xFF };

        let mut scalar_out = vec![0u8; raw.len() + 32];
        assert!(decode_base64_fast(&bad, &mut scalar_out).is_none());

        let mut simd_out = vec![0u8; raw.len() + 32];
        assert!(NeonDecoder::new()
            .decode_to_slice(&bad, &mut simd_out)
            .is_none());
    }
}

fuzz_target!(|data: &[u8]| {
    #[cfg(target_arch = "aarch64")]
    native::run(data);
});
