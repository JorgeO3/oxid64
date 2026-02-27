use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::ssse3::{DecodeOpts, Ssse3Decoder};
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

fn decode_ssse3_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Ssse3Decoder::new();
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_ssse3_non_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
    let written = decoder.decode_to_slice(input, &mut out)?;
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
            let actual_strict = decode_ssse3_strict(&encoded);
            prop_assert_eq!(expected.clone(), actual_strict);
            let actual_non_strict = decode_ssse3_non_strict(&encoded);
            prop_assert_eq!(expected, actual_non_strict);
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
        let actual_strict = decode_ssse3_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_ssse3_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
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
        let actual_strict = decode_ssse3_strict(input);
        assert!(
            actual_strict.is_none(),
            "Strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_non_strict = decode_ssse3_non_strict(input);
        assert!(
            actual_non_strict.is_none(),
            "Non-strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
    }
}
