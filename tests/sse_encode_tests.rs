#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::ssse3::Ssse3Decoder;
use proptest::prelude::*;

fn encode_scalar_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = encode_base64_fast(input, &mut out);
    out.truncate(n);
    out
}

fn encode_sse_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = Ssse3Decoder::new().encode_to_slice(input, &mut out);
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
}

#[test]
fn test_sse_encode_specific_lengths() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }
    let max_len = if cfg!(miri) { 64usize } else { 1024usize };
    for len in 0..max_len {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_sse_reference(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_sse_encode_misaligned_output_slice() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let sse = Ssse3Decoder::new();
    for len in [
        0usize, 1, 2, 3, 4, 15, 16, 17, 31, 32, 47, 48, 63, 64, 127, 128, 255,
    ] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(37)).collect();
        let expected = encode_scalar_reference(&input);
        let out_len = expected.len();

        for offset in 1usize..4 {
            let mut backing = vec![0u8; out_len + offset + 16];
            let written = sse.encode_to_slice(&input, &mut backing[offset..offset + out_len]);
            assert_eq!(
                written, out_len,
                "unexpected encoded size at len={len}, offset={offset}"
            );
            assert_eq!(
                &backing[offset..offset + out_len],
                expected.as_slice(),
                "mismatch at len={len}, offset={offset}"
            );
        }
    }
}
