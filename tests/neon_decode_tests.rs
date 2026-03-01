#![cfg(target_arch = "aarch64")]

use oxid64::engine::DecodeOpts;
use oxid64::engine::neon::NeonDecoder;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use proptest::prelude::*;

fn decode_scalar_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len];
    decode_base64_fast(input, &mut out)?;
    Some(out)
}

fn decode_neon_strict(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let decoder = NeonDecoder::new();
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_neon_non_strict(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let decoder = NeonDecoder::with_opts(DecodeOpts { strict: false });
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

proptest! {
    #[test]
    fn test_neon_decode_matches_scalar(ref input in any::<Vec<u8>>()) {
        let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(input, &mut encoded);

        if std::arch::is_aarch64_feature_detected!("neon") {
            let expected = decode_scalar_reference(&encoded);
            let actual_strict = decode_neon_strict(&encoded);
            prop_assert_eq!(expected.clone(), actual_strict);
            let actual_non_strict = decode_neon_non_strict(&encoded);
            prop_assert_eq!(expected, actual_non_strict);
        }
    }
}

#[test]
fn test_neon_decode_specific_lengths() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }
    for len in 0usize..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_neon_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_neon_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_neon_decode_large_inputs() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }
    // Test sizes that exercise the DN=256 main loop and cleanup loop.
    for len in [192usize, 384, 768, 4096, 65536] {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_neon_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_neon_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_neon_decode_invalid_chars() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }
    let invalid_inputs = [
        "AAA*".as_bytes(),
        "AAAA*AAA".as_bytes(),
        "AAAAAAAAAAAA*AAA".as_bytes(),
        "AAAAAAAAAAAAAAAAAAAA*AAA".as_bytes(),
    ];

    for input in invalid_inputs {
        let actual_strict = decode_neon_strict(input);
        assert!(
            actual_strict.is_none(),
            "Strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_non_strict = decode_neon_non_strict(input);
        assert!(
            actual_non_strict.is_none(),
            "Non-strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
    }
}

#[test]
fn test_neon_decode_invalid_in_main_loop() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }
    // Create a valid 512-byte Base64 input (exercises DN=256 main loop),
    // then inject an invalid character in the middle.
    let raw: Vec<u8> = (0..384).map(|i| (i % 256) as u8).collect();
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let _enc_len = encode_base64_fast(&raw, &mut encoded);
    assert!(encoded.len() >= 512);

    // Place invalid char at position 300 (inside the main loop).
    encoded[300] = b'*';

    let actual_strict = decode_neon_strict(&encoded);
    assert!(
        actual_strict.is_none(),
        "Strict should detect invalid char in main loop"
    );
}
