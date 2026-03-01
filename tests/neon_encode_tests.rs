#![cfg(target_arch = "aarch64")]

use oxid64::engine::neon::NeonDecoder;
use oxid64::engine::scalar::encode_base64_fast;
use proptest::prelude::*;

fn encode_scalar_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = encode_base64_fast(input, &mut out);
    out.truncate(n);
    out
}

fn encode_neon_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = NeonDecoder::new().encode_to_slice(input, &mut out);
    out.truncate(n);
    out
}

proptest! {
    #[test]
    fn test_neon_encode_matches_scalar(ref input in any::<Vec<u8>>()) {
        if std::arch::is_aarch64_feature_detected!("neon") {
            let expected = encode_scalar_reference(input);
            let actual = encode_neon_reference(input);
            prop_assert_eq!(expected, actual);
        }
    }
}

#[test]
fn test_neon_encode_specific_lengths() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }
    for len in 0..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_neon_reference(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_neon_encode_large_inputs() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }
    // Test sizes that exercise the EN=128 main loop and cleanup loop.
    for len in [96, 192, 384, 768, 4096, 65536] {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_neon_reference(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}
