#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};

fn has_backend() -> bool {
    std::is_x86_feature_detected!("avx512f")
        && std::is_x86_feature_detected!("avx512bw")
        && std::is_x86_feature_detected!("avx512vbmi")
}

fuzz_target!(|data: &[u8]| {
    if !cfg!(any(target_arch = "x86", target_arch = "x86_64")) || !has_backend() {
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

    let idx = (data.len().wrapping_mul(29)) % valid_len;
    let mut bad = encoded.clone();
    bad[idx] = b'*';

    let mut scalar_out = vec![0u8; raw.len() + 32];
    assert!(decode_base64_fast(&bad, &mut scalar_out).is_none());

    let mut simd_out = vec![0u8; raw.len() + 64];
    assert!(Avx512VbmiDecoder::new()
        .decode_to_slice(&bad, &mut simd_out)
        .is_none());
});
