use oxid64::simd::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::simd::ssse3::Ssse3Decoder;
use oxid64::simd::ssse3_cstyle::Ssse3CStyleStrictDecoder;
use proptest::prelude::*;

fn decode_scalar_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' {
            2
        } else {
            1
        }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len];
    decode_base64_fast(input, &mut out)?;
    Some(out)
}

fn decode_sse_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' {
            2
        } else {
            1
        }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3Decoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' {
            2
        } else {
            1
        }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

proptest! {
    #[test]
    fn test_sse_decode_matches_scalar(ref input in any::<Vec<u8>>()) {
        let mut encoded = vec![0u8; ((input.len() + 2) / 3) * 4];
        let _enc_len = encode_base64_fast(input, &mut encoded);

        if is_x86_feature_detected!("ssse3") {
            let expected = decode_scalar_reference(&encoded);
            let actual = decode_sse_reference(&encoded);
            prop_assert_eq!(expected.clone(), actual.clone());
            let actual_cstyle_strict = decode_sse_cstyle_strict_reference(&encoded);
            prop_assert_eq!(expected, actual_cstyle_strict);
        }
    }
}

#[test]
fn test_sse_decode_specific_lengths() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }
    for len in 0..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; ((len + 2) / 3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual = decode_sse_reference(&encoded);
        assert_eq!(expected.clone(), actual, "Failed at length {}", len);
        let actual_cstyle_strict = decode_sse_cstyle_strict_reference(&encoded);
        assert_eq!(
            expected, actual_cstyle_strict,
            "C-style strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_sse_decode_invalid_chars() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }
    let invalid_inputs = [
        "AAA*".as_bytes(),
        "AAAA*AAA".as_bytes(),
        "AAAAAAAAAAAA*AAA".as_bytes(),
        "AAAAAAAAAAAAAAAAAAAA*AAA".as_bytes(),
    ];

    for input in invalid_inputs {
        let actual = decode_sse_reference(input);
        assert!(
            actual.is_none(),
            "Should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_cstyle_strict = decode_sse_cstyle_strict_reference(input);
        assert!(
            actual_cstyle_strict.is_none(),
            "C-style strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
    }
}
