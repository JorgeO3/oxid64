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

fn encode_with_canary<F>(out_len: usize, mut f: F) -> Vec<u8>
where
    F: FnMut(&mut [u8]) -> usize,
{
    let pre = 5usize;
    let post = 17usize;
    let canary = 0x5Cu8;
    let mut backing = vec![canary; pre + out_len + post];
    let written = {
        let out = &mut backing[pre..pre + out_len];
        f(out)
    };
    assert_eq!(written, out_len);
    assert!(backing[..pre].iter().all(|&b| b == canary));
    assert!(backing[pre + out_len..].iter().all(|&b| b == canary));
    backing[pre..pre + out_len].to_vec()
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

#[test]
fn test_neon_encode_misaligned_and_canary_safe() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }

    let dec = NeonDecoder::new();
    for len in [
        0usize, 1, 2, 3, 46, 47, 48, 49, 94, 95, 96, 97, 142, 143, 144, 512,
    ] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(31)).collect();
        let expected = encode_scalar_reference(&input);
        let out_len = expected.len();

        for offset in 1usize..4 {
            let canary = 0xD2u8;
            let mut backing = vec![canary; out_len + offset + 13];
            let written = dec.encode_to_slice(&input, &mut backing[offset..offset + out_len]);
            assert_eq!(written, out_len);
            assert_eq!(&backing[offset..offset + out_len], expected.as_slice());
            assert!(backing[..offset].iter().all(|&b| b == canary));
            assert!(backing[offset + out_len..].iter().all(|&b| b == canary));
        }

        let got = encode_with_canary(out_len, |out| dec.encode_to_slice(&input, out));
        assert_eq!(got, expected);
    }
}

#[test]
fn test_neon_encode_boundary_lengths_match_scalar() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }

    for len in [46usize, 47, 94, 95, 142, 143, 190, 191] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(17)).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_neon_reference(&input);
        assert_eq!(expected, actual, "Failed at boundary length {len}");
    }
}
