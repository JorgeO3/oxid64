use oxid64::engine::DecodeOpts;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::ssse3::Ssse3Decoder;
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

fn decode_ssse3_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Ssse3Decoder::new();
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

/// Decode into an exact-fit canary buffer and assert no OOB writes.
/// Returns the decoded bytes on success.
fn decode_ssse3_exact_canary(
    decoder: &Ssse3Decoder,
    encoded: &[u8],
    expected_out_len: usize,
) -> Option<Vec<u8>> {
    let pre = 13usize;
    let post = 19usize;
    let canary = 0xE3u8;
    let mut backing = vec![canary; pre + expected_out_len + post];
    let written = {
        let out = &mut backing[pre..pre + expected_out_len];
        decoder.decode_to_slice(encoded, out)
    }?;
    assert_eq!(written, expected_out_len, "written length mismatch");
    assert!(
        backing[..pre].iter().all(|&b| b == canary),
        "leading canaries corrupted (pre={pre})"
    );
    assert!(
        backing[pre + expected_out_len..]
            .iter()
            .all(|&b| b == canary),
        "trailing canaries corrupted (post={post})"
    );
    Some(backing[pre..pre + written].to_vec())
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
        if input[n - 2] == b'=' { 2 } else { 1 }
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
        let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
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
    let max_len = if cfg!(miri) { 64usize } else { 1024usize };
    for len in 0..max_len {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
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

#[test]
fn test_sse_non_strict_may_accept_unchecked_invalid_lane() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let raw = vec![0x5Au8; 96];
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let _ = encode_base64_fast(&raw, &mut encoded);

    // Place an invalid byte in the second 16-byte lane of the first DS64 block.
    encoded[20] = b'*';

    let strict = decode_ssse3_strict(&encoded);
    assert!(
        strict.is_none(),
        "strict decoder must reject invalid character"
    );

    let non_strict = decode_ssse3_non_strict(&encoded);
    assert!(
        non_strict.is_some(),
        "non-strict currently validates only one lane per DS64 block"
    );
}

#[test]
fn test_sse_align_output_short_padded_inputs() {
    // Regression: align_output previously formed `&mut [u8; 3]` via raw
    // pointer cast even when only 1 or 2 output bytes were valid, which
    // was UB when the allocation had exactly the decoded length.
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let decoder_strict = Ssse3Decoder::new();
    let decoder_non_strict = Ssse3Decoder::with_opts(DecodeOpts { strict: false });

    // (base64_input, expected_decoded_bytes)
    let cases: &[(&[u8], &[u8])] = &[
        (b"TQ==", &[0x4D]),             // 2-pad → 1 byte
        (b"TUE=", &[0x4D, 0x41]),       // 1-pad → 2 bytes
        (b"TVFH", &[0x4D, 0x51, 0x47]), // no pad → 3 bytes
        (b"AAAA", &[0x00, 0x00, 0x00]), // all zeros
        (b"////", &[0xFF, 0xFF, 0xFF]), // all ones
        (b"/w==", &[0xFF]),             // 1 byte, high bits
        (b"/8E=", &[0xFF, 0xC1]),       // 2 bytes, high bits
    ];

    let canary = 0xD1u8;

    for (b64_input, expected) in cases {
        let out_len = expected.len();

        // Test with several misalignments to ensure align_output runs.
        for offset in 1usize..16 {
            // Exact-fit output buffer surrounded by canaries.
            let total = offset + out_len + 8;
            let mut buf = vec![canary; total];
            let out_slice = &mut buf[offset..offset + out_len];

            let written = decoder_strict
                .decode_to_slice(b64_input, out_slice)
                .unwrap_or_else(|| {
                    panic!(
                        "strict decode failed for {:?} at offset {}",
                        std::str::from_utf8(b64_input).unwrap(),
                        offset
                    )
                });
            assert_eq!(
                written,
                out_len,
                "strict: wrong length for {:?} at offset {}",
                std::str::from_utf8(b64_input).unwrap(),
                offset
            );
            assert_eq!(
                &buf[offset..offset + out_len],
                *expected,
                "strict: wrong content for {:?} at offset {}",
                std::str::from_utf8(b64_input).unwrap(),
                offset
            );
            // Canary integrity — no OOB writes.
            assert!(
                buf[..offset].iter().all(|&b| b == canary),
                "strict: leading canaries corrupted for {:?} at offset {}",
                std::str::from_utf8(b64_input).unwrap(),
                offset
            );
            assert!(
                buf[offset + out_len..].iter().all(|&b| b == canary),
                "strict: trailing canaries corrupted for {:?} at offset {}",
                std::str::from_utf8(b64_input).unwrap(),
                offset
            );

            // Same test for non-strict decoder.
            let mut buf2 = vec![canary; total];
            let out_slice2 = &mut buf2[offset..offset + out_len];
            let written2 = decoder_non_strict
                .decode_to_slice(b64_input, out_slice2)
                .unwrap_or_else(|| {
                    panic!(
                        "non-strict decode failed for {:?} at offset {}",
                        std::str::from_utf8(b64_input).unwrap(),
                        offset
                    )
                });
            assert_eq!(written2, out_len);
            assert_eq!(&buf2[offset..offset + out_len], *expected);
            assert!(buf2[..offset].iter().all(|&b| b == canary));
            assert!(buf2[offset + out_len..].iter().all(|&b| b == canary));
        }
    }
}

/// Exact-window canary test at every SSSE3 regime transition boundary.
///
/// SSSE3 regime thresholds (with `safe_end = in_len - 4`):
///   - DS64 double: `remaining >= 160` → enters at `encoded_len >= 164`
///   - DS64 single: `remaining >= 96`  → enters at `encoded_len >= 100`
///   - Tail16:      `remaining >= 16`  → enters at `encoded_len >= 20`
///   - Pre-SIMD:    `remaining >= 32 (preload) && out_remaining >= 16`
///
/// We test encoded lengths at `threshold - 4`, `threshold`, `threshold + 4`
/// for each boundary, plus additional regime drain points, using an exact-fit
/// output buffer with canary guards. Both strict and non-strict modes.
#[test]
fn test_ssse3_exact_window_regime_boundaries() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let decoder_strict = Ssse3Decoder::new();
    let decoder_non_strict = Ssse3Decoder::with_opts(DecodeOpts { strict: false });

    // Critical encoded lengths (must be multiples of 4) near regime transitions.
    // Each transition is tested at -4/0/+4 from the threshold.
    let critical_encoded_lens: Vec<usize> = {
        let mut lens = Vec::new();

        // Tail16 threshold: encoded >= 20
        for &t in &[16, 20, 24] {
            lens.push(t);
        }

        // Pre-SIMD gate / DS64 entry: encoded >= 36 (32 preload + 4 safe_end)
        // The pre-SIMD gate needs remaining(ip, safe_end) >= 32. safe_end = in_len - 4.
        // After align_output (0 iterations if aligned), remaining = in_len - 4.
        // So need in_len - 4 >= 32 → in_len >= 36.
        for &t in &[32, 36, 40] {
            lens.push(t);
        }

        // DS64 single threshold: remaining >= 96 → in_len >= 100
        for &t in &[96, 100, 104] {
            lens.push(t);
        }

        // DS64 double threshold: remaining >= 160 → in_len >= 164
        for &t in &[160, 164, 168] {
            lens.push(t);
        }

        // At 2×DS64 drain points (where double loop runs 1 iteration then falls through)
        for &t in &[288, 292, 296] {
            lens.push(t);
        }

        // Larger: 3× double iterations
        for &t in &[416, 420, 424] {
            lens.push(t);
        }

        // Some odd boundary sizes that stress tail16 after DS64
        // DS64 processes 64→48, tail16 processes 16→12.
        // After 1 DS64 (consumed 64), if remaining(in safe_end) >= 16 → tail16
        // encoded=84: safe_end=80, after DS64: ip at 64, remaining=16 → exactly enters tail16
        for &t in &[80, 84, 88] {
            lens.push(t);
        }

        // After 2 DS64 (consumed 128), safe_end = in_len-4
        // For tail16: need remaining >= 16 → in_len - 4 - 128 >= 16 → in_len >= 148
        for &t in &[144, 148, 152] {
            lens.push(t);
        }

        lens.sort();
        lens.dedup();
        // Filter to valid Base64 lengths (multiples of 4)
        lens.retain(|&l| l % 4 == 0);
        lens
    };

    for &enc_len in &critical_encoded_lens {
        // Compute raw length from encoded length (no padding)
        let raw_len = (enc_len / 4) * 3;

        // Generate deterministic raw bytes and encode
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(37)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let enc_written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(enc_written);
        assert_eq!(
            encoded.len(),
            enc_len,
            "encoded length mismatch for raw_len={raw_len}"
        );

        let expected = decode_scalar_reference(&encoded).expect("scalar decode should work");

        // Strict: exact-fit canary buffer
        let result_strict = decode_ssse3_exact_canary(&decoder_strict, &encoded, raw_len)
            .unwrap_or_else(|| {
                panic!("strict canary decode failed at enc_len={enc_len} raw_len={raw_len}")
            });
        assert_eq!(
            result_strict, expected,
            "strict decode mismatch at enc_len={enc_len}"
        );

        // Non-strict: exact-fit canary buffer
        let result_non_strict = decode_ssse3_exact_canary(&decoder_non_strict, &encoded, raw_len)
            .unwrap_or_else(|| {
                panic!("non-strict canary decode failed at enc_len={enc_len} raw_len={raw_len}")
            });
        assert_eq!(
            result_non_strict, expected,
            "non-strict decode mismatch at enc_len={enc_len}"
        );
    }
}

/// Error-path canary test: invalid input must not corrupt canary bytes
/// even when SIMD stores have already been written (deferred error check).
#[test]
fn test_ssse3_error_path_preserves_canaries() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let decoder = Ssse3Decoder::new();

    // Test with inputs large enough to exercise each SIMD regime,
    // placing the invalid byte at different positions within each regime.
    let test_cases: &[(usize, usize)] = &[
        // (raw_len to generate valid base64, position to corrupt)
        (12, 3),    // scalar tail only
        (24, 15),   // tail16 regime
        (96, 60),   // inside first DS64 block
        (192, 100), // inside second DS64 block (double loop)
        (384, 200), // deep in double-DS64 loop
        (768, 500), // well within hot loop
    ];

    for &(raw_len, corrupt_pos) in test_cases {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(41)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let enc_written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(enc_written);

        if corrupt_pos >= encoded.len() {
            continue;
        }

        let mut bad = encoded.clone();
        bad[corrupt_pos] = b'*';

        let out_len = (bad.len() / 4) * 3;
        for offset in 1usize..4 {
            let canary = 0xD7u8;
            let pre = offset;
            let post = 17;
            let mut backing = vec![canary; pre + out_len + post];
            let out = &mut backing[pre..pre + out_len];
            let result = decoder.decode_to_slice(&bad, out);
            assert!(
                result.is_none(),
                "strict should reject invalid at pos {corrupt_pos} in enc_len={}",
                bad.len()
            );
            assert!(
                backing[..pre].iter().all(|&b| b == canary),
                "leading canaries corrupted at raw_len={raw_len} corrupt_pos={corrupt_pos}"
            );
            assert!(
                backing[pre + out_len..].iter().all(|&b| b == canary),
                "trailing canaries corrupted at raw_len={raw_len} corrupt_pos={corrupt_pos}"
            );
        }
    }
}

#[test]
fn test_sse_decode_misaligned_output_and_input_subslice() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let decoder = Ssse3Decoder::new();
    for len in [0usize, 1, 2, 3, 15, 16, 17, 63, 64, 65, 191, 192, 193, 1024] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(23)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        let mut in_backing = vec![b'!'; encoded.len() + 7];
        in_backing[3..3 + encoded.len()].copy_from_slice(&encoded);
        let input = &in_backing[3..3 + encoded.len()];

        for out_offset in 1usize..4 {
            let out_len = raw.len();
            let canary = 0xD1u8;
            let mut out_backing = vec![canary; out_len + out_offset + 11];
            let out_slice = &mut out_backing[out_offset..out_offset + out_len];

            let written = decoder
                .decode_to_slice(input, out_slice)
                .expect("strict decode should succeed");
            assert_eq!(written, out_len, "len mismatch at raw len={len}");
            assert_eq!(
                out_slice,
                raw.as_slice(),
                "decode mismatch at raw len={len}"
            );
            assert!(out_backing[..out_offset].iter().all(|&b| b == canary));
            assert!(
                out_backing[out_offset + out_len..]
                    .iter()
                    .all(|&b| b == canary)
            );
        }
    }
}

/// Malformed padding must be rejected through the SSSE3 frontend regardless
/// of input size. We test with inputs large enough to exercise the SIMD
/// regimes before hitting the scalar tail that handles padding.
#[test]
fn test_ssse3_rejects_malformed_padding() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let decoder_strict = Ssse3Decoder::new();
    let decoder_non_strict = Ssse3Decoder::with_opts(DecodeOpts { strict: false });

    // For each prefix length, we'll generate valid Base64 data (no padding)
    // of that length, then append a 4-byte tail with various malformed padding.
    // The prefix sizes are chosen to exercise different SSSE3 regimes.
    let prefix_raw_sizes: &[usize] = &[
        0,   // empty prefix → only tail
        3,   // 4 bytes encoded, scalar only
        12,  // 16 bytes encoded, tail16
        48,  // 64 bytes encoded, DS64 single
        120, // 160 bytes encoded, DS64 double
        240, // 320 bytes encoded, deep in double-DS64
    ];

    let malformed_tails: &[&[u8]] = &[
        b"=AAA", // = in position 0
        b"A=AA", // = in position 1
        b"AA=A", // = in position 2 without position 3
        b"A===", // x=== pattern
        b"TR==", // non-canonical 2-pad (bottom 4 bits of v1 != 0)
        b"TWF=", // non-canonical 1-pad (bottom 2 bits of v2 != 0)
    ];

    for &prefix_raw_len in prefix_raw_sizes {
        let prefix_raw: Vec<u8> = (0..prefix_raw_len)
            .map(|i| (i as u8).wrapping_mul(31))
            .collect();
        let mut prefix_enc = vec![0u8; prefix_raw_len.div_ceil(3) * 4];
        let enc_written = encode_base64_fast(&prefix_raw, &mut prefix_enc);
        prefix_enc.truncate(enc_written);
        // prefix_enc should have no padding (prefix_raw_len is multiple of 3)
        assert_eq!(
            prefix_raw_len % 3,
            0,
            "prefix_raw_len must be multiple of 3"
        );

        for &bad_tail in malformed_tails {
            let mut full_input = prefix_enc.clone();
            full_input.extend_from_slice(bad_tail);

            let max_out = (full_input.len() / 4) * 3;
            let mut out_strict = vec![0u8; max_out + 16];
            let mut out_non_strict = vec![0u8; max_out + 16];

            let strict_result = decoder_strict.decode_to_slice(&full_input, &mut out_strict);
            assert!(
                strict_result.is_none(),
                "strict SSSE3 should reject {:?} tail at prefix_raw_len={prefix_raw_len}",
                std::str::from_utf8(bad_tail).unwrap_or("<invalid>")
            );

            let non_strict_result =
                decoder_non_strict.decode_to_slice(&full_input, &mut out_non_strict);
            assert!(
                non_strict_result.is_none(),
                "non-strict SSSE3 should reject {:?} tail at prefix_raw_len={prefix_raw_len}",
                std::str::from_utf8(bad_tail).unwrap_or("<invalid>")
            );
        }
    }
}
