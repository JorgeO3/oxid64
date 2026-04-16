#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::scalar::encode_base64_fast;
use proptest::prelude::*;

fn encode_scalar_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = encode_base64_fast(input, &mut out);
    out.truncate(n);
    out
}

fn encode_avx2_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = Avx2Decoder::new().encode_to_slice(input, &mut out);
    out.truncate(n);
    out
}

proptest! {
    #[test]
    fn test_avx2_encode_matches_scalar(ref input in any::<Vec<u8>>()) {
        if is_x86_feature_detected!("avx2") {
            let expected = encode_scalar_reference(input);
            let actual = encode_avx2_reference(input);
            prop_assert_eq!(expected, actual);
        }
    }
}

#[test]
fn test_avx2_encode_specific_lengths() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }
    let max_len = if cfg!(miri) { 64usize } else { 1024usize };
    for len in 0..max_len {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_avx2_reference(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}
