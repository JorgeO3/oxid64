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

fn decode_with_canary<F>(out_len: usize, mut f: F) -> Option<usize>
where
    F: FnMut(&mut [u8]) -> Option<usize>,
{
    let pre = 7usize;
    let post = 19usize;
    let canary = 0xA6u8;
    let mut backing = vec![canary; pre + out_len + post];
    let written = {
        let out = &mut backing[pre..pre + out_len];
        f(out)
    }?;
    assert!(backing[..pre].iter().all(|&b| b == canary));
    assert!(backing[pre + out_len..].iter().all(|&b| b == canary));
    Some(written)
}

fn expected_non_strict_checked(encoded_len: usize, pos: usize) -> bool {
    let mut ip = 0usize;

    while encoded_len.saturating_sub(ip) > 256 {
        if pos >= ip && pos < ip + 256 {
            return (pos - ip) / 64 == 3;
        }
        ip += 256;
    }

    while encoded_len.saturating_sub(ip) > 64 {
        if pos >= ip && pos < ip + 64 {
            return true;
        }
        ip += 64;
    }

    true
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

#[test]
fn test_neon_decode_rejects_high_bit_bytes_in_strict_mode() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }

    let raw: Vec<u8> = (0..240).map(|i| (i % 256) as u8).collect();
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let _ = encode_base64_fast(&raw, &mut encoded);
    assert_eq!(encoded.len(), 320);

    for &byte in &[0x80u8, 0xFFu8] {
        for &pos in &[0usize, 63, 64, 127, 128, 191, 192, 255, 256, 315] {
            let mut bad = encoded.clone();
            bad[pos] = byte;
            let actual = decode_neon_strict(&bad);
            assert!(
                actual.is_none(),
                "strict should reject high-bit byte {byte:#04x} at pos={pos}"
            );
        }
    }
}

#[test]
fn test_neon_decode_non_strict_matches_documented_schedule() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }

    for raw_len in [240usize, 432] {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(27)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        for pos in 0..encoded.len() {
            if encoded[pos] == b'=' {
                continue;
            }

            let mut bad = encoded.clone();
            bad[pos] = b'*';
            let actual = decode_neon_non_strict(&bad);
            assert_eq!(
                actual.is_none(),
                expected_non_strict_checked(encoded.len(), pos),
                "non-strict schedule mismatch at encoded_len={}, pos={pos}",
                encoded.len()
            );
        }
    }
}

#[test]
fn test_neon_decode_valid_padding_at_simd_block_boundaries() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }

    let strict = NeonDecoder::new();
    let non_strict = NeonDecoder::with_opts(DecodeOpts { strict: false });

    for len in [46usize, 47, 94, 95, 142, 143, 190, 191] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(39)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);
        assert_eq!(encoded.len() % 64, 0, "expected 64-byte boundary case");
        assert!(encoded.ends_with(b"=") || encoded.ends_with(b"=="));

        let strict_written =
            decode_with_canary(raw.len(), |out| strict.decode_to_slice(&encoded, out))
                .expect("strict decode should accept valid padded boundary case");
        assert_eq!(
            strict_written,
            raw.len(),
            "strict decoded length mismatch at len={len}"
        );

        let non_strict_written =
            decode_with_canary(raw.len(), |out| non_strict.decode_to_slice(&encoded, out))
                .expect("non-strict decode should accept valid padded boundary case");
        assert_eq!(
            non_strict_written,
            raw.len(),
            "non-strict decoded length mismatch at len={len}"
        );
    }
}

#[test]
fn test_neon_decode_preserves_canaries_with_exact_output_window() {
    if !std::arch::is_aarch64_feature_detected!("neon") {
        return;
    }

    let decoder = NeonDecoder::new();
    for len in [0usize, 1, 2, 3, 63, 64, 65, 191, 192, 193, 1024] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(41)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        let written = decode_with_canary(raw.len(), |out| decoder.decode_to_slice(&encoded, out))
            .expect("strict decode should succeed on valid input");
        assert_eq!(written, raw.len(), "decoded length mismatch at len={len}");
    }
}
