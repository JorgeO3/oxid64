use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::ssse3::DecodeOpts;
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

fn decode_avx2_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Avx2Decoder::new();
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_avx2_non_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Avx2Decoder::with_opts(DecodeOpts { strict: false });
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

proptest! {
    #[test]
    fn test_avx2_decode_matches_scalar(ref input in any::<Vec<u8>>()) {
        if is_x86_feature_detected!("avx2") {
            let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
            let _enc_len = encode_base64_fast(input, &mut encoded);

            let expected = decode_scalar_reference(&encoded);
            let actual_strict = decode_avx2_strict(&encoded);
            prop_assert_eq!(expected.clone(), actual_strict);
            let actual_non_strict = decode_avx2_non_strict(&encoded);
            prop_assert_eq!(expected, actual_non_strict);
        }
    }
}

#[test]
fn test_avx2_decode_specific_lengths() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }
    for len in 0usize..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_avx2_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_avx2_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_avx2_decode_invalid_chars() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }
    // Invalid chars at various positions to exercise scalar tail, SSE tail,
    // and AVX2 main-loop paths.
    let invalid_inputs = [
        "AAA*".as_bytes(),
        "AAAA*AAA".as_bytes(),
        "AAAAAAAAAAAA*AAA".as_bytes(),
        "AAAAAAAAAAAAAAAAAAAA*AAA".as_bytes(),
    ];

    for input in invalid_inputs {
        let actual_strict = decode_avx2_strict(input);
        assert!(
            actual_strict.is_none(),
            "Strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_non_strict = decode_avx2_non_strict(input);
        assert!(
            actual_non_strict.is_none(),
            "Non-strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
    }
}

/// Test with inputs large enough to exercise the double-DS128 (256-byte) and
/// single-DS128 (128-byte) unrolled loops, plus the SSE tail.
#[test]
fn test_avx2_decode_large_inputs() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }
    // Sizes chosen to hit different loop-stage boundaries:
    // 192 = exactly one DS128 worth of output (128 input = 96 output)
    // 384 = exactly one double-DS128 (256 input = 192 output)
    // 768 = multiple double-DS128 blocks
    // 4096 = large general case
    for raw_len in [192usize, 384, 768, 4096, 65536] {
        let input: Vec<u8> = (0..raw_len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; raw_len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_avx2_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at raw_len {}",
            raw_len
        );
        let actual_non_strict = decode_avx2_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at raw_len {}",
            raw_len
        );
    }
}

/// Invalid character placed deep inside input, at a position that would be
/// processed by the AVX2 main loop (not just the scalar/SSE tail).
#[test]
fn test_avx2_decode_invalid_in_main_loop() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }
    // Build a valid 1024-byte encoded string, then corrupt a byte in the
    // AVX2 hot loop region.
    let raw: Vec<u8> = (0..768).map(|i| (i % 256) as u8).collect();
    let mut encoded = vec![0u8; 768_usize.div_ceil(3) * 4];
    let enc_len = encode_base64_fast(&raw, &mut encoded);
    encoded.truncate(enc_len);

    // Corrupt byte 100 — well inside the first DS128 block
    let mut bad = encoded.clone();
    bad[100] = b'*';
    assert!(
        decode_avx2_strict(&bad).is_none(),
        "Strict should reject invalid char at position 100"
    );

    // Non-strict may or may not catch it depending on CHECK0 coverage,
    // but if it does produce output, it must still match scalar or be None.
    // (We just verify it doesn't panic.)
    let _ = decode_avx2_non_strict(&bad);
}
