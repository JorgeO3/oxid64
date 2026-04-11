#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::ssse3::Ssse3Decoder;

fuzz_target!(|data: &[u8]| {
    if !cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        || !std::is_x86_feature_detected!("ssse3")
    {
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

    let mut bad = encoded.clone();
    let idx = (data.len().wrapping_mul(17)) % valid_len;
    bad[idx] = b'*';

    let mut scalar_out = vec![0u8; raw.len() + 16];
    assert!(decode_base64_fast(&bad, &mut scalar_out).is_none());

    let mut simd_out = vec![0u8; raw.len() + 32];
    assert!(Ssse3Decoder::new()
        .decode_to_slice(&bad, &mut simd_out)
        .is_none());
});
