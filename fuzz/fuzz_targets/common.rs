#![allow(dead_code)]

use oxid64::engine::scalar::{ScalarDecoder, decode_base64_fast, encode_base64_fast};
use oxid64::{Base64Decoder, Decoder, decoded_len, encoded_len};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86_native {
    use super::{decode_with_window, encode_with_window};
    use oxid64::Base64Decoder;
    use oxid64::engine::avx2::Avx2Decoder;
    use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
    use oxid64::engine::ssse3::Ssse3Decoder;

    pub(super) fn assert_strict_decode_matches_scalar(input: &[u8], expected: &Option<Vec<u8>>) {
        if std::is_x86_feature_detected!("ssse3") {
            let dec = Ssse3Decoder::new();
            assert_eq!(
                decode_with_window(&dec, input, 1),
                *expected,
                "ssse3 strict mismatch"
            );
        }
        if std::is_x86_feature_detected!("avx2") {
            let dec = Avx2Decoder::new();
            assert_eq!(
                decode_with_window(&dec, input, 2),
                *expected,
                "avx2 strict mismatch"
            );
        }
        if std::is_x86_feature_detected!("avx512vbmi") {
            let dec = Avx512VbmiDecoder::new();
            assert_eq!(
                decode_with_window(&dec, input, 3),
                *expected,
                "avx512 strict mismatch"
            );
        }
    }

    pub(super) fn assert_encode_matches_scalar(input: &[u8], expected: &[u8]) {
        if std::is_x86_feature_detected!("ssse3") {
            let dec = Ssse3Decoder::new();
            assert_eq!(
                encode_with_window(&dec, input, 1),
                expected,
                "ssse3 encode mismatch"
            );
        }
        if std::is_x86_feature_detected!("avx2") {
            let dec = Avx2Decoder::new();
            assert_eq!(
                encode_with_window(&dec, input, 2),
                expected,
                "avx2 encode mismatch"
            );
        }
        if std::is_x86_feature_detected!("avx512vbmi") {
            let dec = Avx512VbmiDecoder::new();
            assert_eq!(
                encode_with_window(&dec, input, 3),
                expected,
                "avx512 encode mismatch"
            );
        }
    }

    pub(super) fn assert_roundtrip_all(input: &[u8], encoded: &[u8]) {
        if std::is_x86_feature_detected!("avx2") {
            let dec = Avx2Decoder::new();
            assert_eq!(dec.decode(encoded).expect("avx2 decode"), input);
        }
        if std::is_x86_feature_detected!("ssse3") {
            let dec = Ssse3Decoder::new();
            assert_eq!(dec.decode(encoded).expect("ssse3 decode"), input);
        }
        if std::is_x86_feature_detected!("avx512vbmi") {
            let dec = Avx512VbmiDecoder::new();
            assert_eq!(dec.decode(encoded).expect("avx512 decode"), input);
        }
    }
}

pub(crate) const MAX_FUZZ_LEN: usize = 4096;

pub(crate) fn clamp_input(data: &[u8]) -> &[u8] {
    &data[..data.len().min(MAX_FUZZ_LEN)]
}

pub(crate) fn scalar_encode(input: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; encoded_len(input.len())];
    let written = encode_base64_fast(input, &mut out);
    out.truncate(written);
    out
}

pub(crate) fn scalar_decode(input: &[u8]) -> Option<Vec<u8>> {
    let out_len = decoded_len(input)?;
    let mut out = vec![0u8; out_len];
    let written = decode_base64_fast(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

pub(crate) fn encode_with_window<E: Base64Decoder>(
    engine: &E,
    input: &[u8],
    offset: usize,
) -> Vec<u8> {
    let out_len = encoded_len(input.len());
    let canary = 0xA5u8;
    let mut backing = vec![canary; out_len + offset + 16];
    let out = &mut backing[offset..offset + out_len];
    let written = engine.encode_to_slice(input, out);
    assert_eq!(written, out_len);
    assert!(backing[..offset].iter().all(|&b| b == canary));
    assert!(backing[offset + out_len..].iter().all(|&b| b == canary));
    backing[offset..offset + out_len].to_vec()
}

pub(crate) fn decode_with_window<E: Base64Decoder>(
    engine: &E,
    input: &[u8],
    offset: usize,
) -> Option<Vec<u8>> {
    let out_len = decoded_len(input)?;
    let canary = 0x5Au8;
    let mut backing = vec![canary; out_len + offset + 16];
    let out = &mut backing[offset..offset + out_len];
    let written = engine.decode_to_slice(input, out)?;
    assert_eq!(written, out_len);
    assert!(backing[..offset].iter().all(|&b| b == canary));
    assert!(backing[offset + out_len..].iter().all(|&b| b == canary));
    Some(backing[offset..offset + out_len].to_vec())
}

pub(crate) fn subslice_with_prefix(input: &[u8], prefix_len: usize, fill: u8) -> Vec<u8> {
    let mut backing = vec![fill; prefix_len + input.len() + 4];
    backing[prefix_len..prefix_len + input.len()].copy_from_slice(input);
    backing
}

pub(crate) fn assert_strict_decode_matches_scalar(input: &[u8]) {
    let expected = scalar_decode(input);

    let scalar = ScalarDecoder;
    assert_eq!(decode_with_window(&scalar, input, 0), expected);
    assert_eq!(decode_with_window(&scalar, input, 1), expected);

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    x86_native::assert_strict_decode_matches_scalar(input, &expected);
}
pub(crate) fn assert_encode_matches_scalar(input: &[u8]) {
    let expected = scalar_encode(input);

    let scalar = ScalarDecoder;
    assert_eq!(encode_with_window(&scalar, input, 0), expected);
    assert_eq!(encode_with_window(&scalar, input, 1), expected);

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    x86_native::assert_encode_matches_scalar(input, &expected);
}

pub(crate) fn assert_roundtrip_all(input: &[u8]) {
    let encoded = scalar_encode(input);
    let detected = Decoder::detect();
    let decoded = detected
        .decode(&encoded)
        .expect("Decoder::detect must decode scalar encoding");
    assert_eq!(decoded, input);

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    x86_native::assert_roundtrip_all(input, &encoded);
}
