use oxid64::simd::sse42::Sse42Decoder;
use oxid64::simd::avx2::Avx2Decoder;
use oxid64::simd::scalar::encode_base64_fast;
use proptest::prelude::*;

fn encode_scalar_reference(input: &[u8]) -> Vec<u8> {
    let out_len = ((input.len() + 2) / 3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = encode_base64_fast(input, &mut out);
    out.truncate(n);
    out
}

fn encode_sse_reference(input: &[u8]) -> Vec<u8> {
    let out_len = ((input.len() + 2) / 3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = Sse42Decoder::encode_to_slice(input, &mut out);
    out.truncate(n);
    out
}

fn encode_avx2_reference(input: &[u8]) -> Vec<u8> {
    let out_len = ((input.len() + 2) / 3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = Avx2Decoder::encode_to_slice(input, &mut out);
    out.truncate(n);
    out
}

proptest! {
    #[test]
    fn test_sse_encode_matches_scalar(ref input in any::<Vec<u8>>()) {
        if is_x86_feature_detected!("ssse3") {
            let expected = encode_scalar_reference(input);
            let actual = encode_sse_reference(input);
            prop_assert_eq!(expected, actual);
        }
    }

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
fn test_sse_encode_specific_lengths() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }
    for len in 0..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_sse_reference(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_avx2_encode_specific_lengths() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }
    for len in 0..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_avx2_reference(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}