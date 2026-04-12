use oxid64::engine::DecodeOpts;
use oxid64::engine::avx2::Avx2Decoder;
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

fn decode_avx2_unchecked(input: &[u8], out: &mut [u8]) -> Option<usize> {
    Avx2Decoder::new().decode_to_slice_unchecked(input, out)
}

/// Decode into an exact-fit canary buffer and assert no OOB writes.
fn decode_avx2_exact_canary(
    decoder: &Avx2Decoder,
    encoded: &[u8],
    expected_out_len: usize,
) -> Option<Vec<u8>> {
    let pre = 13usize;
    let post = 19usize;
    let canary = 0xE2u8;
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
    let max_len = if cfg!(miri) { 64usize } else { 1024usize };
    for len in 0..max_len {
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
    let sizes: &[usize] = if cfg!(miri) {
        &[192, 384, 768]
    } else {
        &[192, 384, 768, 4096, 65536]
    };
    for &raw_len in sizes {
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

#[test]
fn test_avx2_decode_misaligned_output_and_input_subslice() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    let decoder = Avx2Decoder::new();
    for len in [
        0usize, 1, 2, 3, 31, 32, 33, 127, 128, 129, 383, 384, 385, 2048,
    ] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(19)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        let mut in_backing = vec![b'#'; encoded.len() + 9];
        in_backing[5..5 + encoded.len()].copy_from_slice(&encoded);
        let input = &in_backing[5..5 + encoded.len()];

        for out_offset in 1usize..4 {
            let out_len = raw.len();
            let canary = 0xC7u8;
            let mut out_backing = vec![canary; out_len + out_offset + 13];
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

#[test]
fn test_avx2_unchecked_valid_input_matches_scalar() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    for len in [
        0usize, 1, 2, 3, 31, 32, 33, 127, 128, 129, 383, 384, 385, 2048,
    ] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(11)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        let expected = decode_scalar_reference(&encoded).expect("scalar decode should succeed");
        let mut out = vec![0u8; expected.len() + 16];
        let written =
            decode_avx2_unchecked(&encoded, &mut out).expect("unchecked decode should succeed");
        assert_eq!(
            &out[..written],
            expected.as_slice(),
            "unchecked mismatch at raw len={len}"
        );
    }
}

#[test]
fn test_avx2_unchecked_output_too_small_returns_none() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    let raw = b"output-small";
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let _ = encode_base64_fast(raw, &mut encoded);
    let mut out = vec![0u8; raw.len().saturating_sub(1)];
    assert!(decode_avx2_unchecked(&encoded, &mut out).is_none());
}

#[test]
fn test_avx2_unchecked_malformed_input_preserves_canaries() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    let raw = vec![0x42u8; 246];
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let _ = encode_base64_fast(&raw, &mut encoded);

    // Invalidate both a DS128 body byte and a scalar-tail byte.
    let mut bad = encoded.clone();
    bad[40] = b'*';
    let last = bad.len() - 1;
    bad[last] = b'*';

    let out_len = (bad.len() / 4) * 3;
    for offset in 1usize..4 {
        let canary = 0x9Du8;
        let mut backing = vec![canary; out_len + offset + 17];
        let out = &mut backing[offset..offset + out_len];
        let _ = decode_avx2_unchecked(&bad, out);
        assert!(backing[..offset].iter().all(|&b| b == canary));
        assert!(backing[offset + out_len..].iter().all(|&b| b == canary));
    }
}

/// Exact-window canary test at every AVX2 regime transition boundary.
///
/// AVX2 regime thresholds (from avx2 model constants):
///   - Triple-DS128 (strict): `in_len > 452` → enters at encoded_len >= 456
///   - Double-DS128:          `in_len > 324` → enters at encoded_len >= 328
///   - Single-DS128:          `in_len >= 196` → enters at encoded_len >= 196
///   - Tail16:                `remaining > 20` → enters when leftover > 20
///
/// We test encoded lengths at `threshold - 4`, `threshold`, `threshold + 4`
/// for each boundary, plus drain points, using exact-fit canary buffers.
#[test]
fn test_avx2_exact_window_regime_boundaries() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    let decoder_strict = Avx2Decoder::new();
    let decoder_non_strict = Avx2Decoder::with_opts(DecodeOpts { strict: false });

    // Critical encoded lengths near AVX2 regime transitions (multiples of 4).
    let critical_encoded_lens: Vec<usize> = {
        let mut lens = Vec::new();

        // Tail16 threshold: remaining > 20 → enters when leftover > 20
        // After all DS128 loops, if SIMD was skipped entirely (in_len < 196),
        // the tail16 loop runs when in_len > 20+consumed.
        // For no-SIMD path: enters at in_len > 20 → in_len >= 24 (mult of 4)
        for &t in &[16, 20, 24, 28] {
            lens.push(t);
        }

        // Single-DS128 threshold: in_len >= 196
        for &t in &[192, 196, 200] {
            lens.push(t);
        }

        // Double-DS128 threshold: in_len > 324 → enters at 328
        for &t in &[320, 324, 328, 332] {
            lens.push(t);
        }

        // Triple-DS128 threshold (strict only): in_len > 452 → enters at 456
        for &t in &[448, 452, 456, 460] {
            lens.push(t);
        }

        // Drain points after DS128 loops where tail16 kicks in.
        // After 1 single DS128 (consumed 128 input), remaining for tail16.
        // encoded=196: safe_end=192. After preload+DS128, ip at 128, remaining=64.
        // tail16 enters when remaining > 20, so it runs.
        // Test where tail16 just barely enters/doesn't enter after DS128.
        // After DS128 consumed 128: remaining = in_len - 128.
        // Tail16 needs remaining > 20 → in_len > 148 → enters at 152.
        // But we already have single DS128 threshold at 196, so after 1 DS128
        // remaining = in_len - 128 - 4 (safe_end offset already accounted for)

        // After 2×DS128 (consumed 256): remaining = in_len - 256
        // Tail16: remaining > 20 → in_len > 276 → 280
        for &t in &[276, 280, 284] {
            lens.push(t);
        }

        // After triple-DS128 (consumed 384): remaining = in_len - 384
        // Tail16: remaining > 20 → in_len > 404 → 408
        for &t in &[404, 408, 412] {
            lens.push(t);
        }

        // Larger: multiple triple iterations (strict)
        for &t in &[580, 584, 588, 840, 844] {
            lens.push(t);
        }

        lens.sort();
        lens.dedup();
        lens.retain(|&l| l % 4 == 0);
        lens
    };

    for &enc_len in &critical_encoded_lens {
        let raw_len = (enc_len / 4) * 3;

        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(43)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let enc_written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(enc_written);
        assert_eq!(
            encoded.len(),
            enc_len,
            "encoded length mismatch for raw_len={raw_len}"
        );

        let expected = decode_scalar_reference(&encoded).expect("scalar decode should work");

        // Strict
        let result_strict = decode_avx2_exact_canary(&decoder_strict, &encoded, raw_len)
            .unwrap_or_else(|| {
                panic!("strict canary decode failed at enc_len={enc_len} raw_len={raw_len}")
            });
        assert_eq!(
            result_strict, expected,
            "strict decode mismatch at enc_len={enc_len}"
        );

        // Non-strict
        let result_non_strict = decode_avx2_exact_canary(&decoder_non_strict, &encoded, raw_len)
            .unwrap_or_else(|| {
                panic!("non-strict canary decode failed at enc_len={enc_len} raw_len={raw_len}")
            });
        assert_eq!(
            result_non_strict, expected,
            "non-strict decode mismatch at enc_len={enc_len}"
        );
    }
}

/// Error-path canary test for AVX2: invalid input must not corrupt canary bytes.
/// Complements `test_avx2_unchecked_malformed_input_preserves_canaries` by
/// testing the strict (checked) path with corrupt positions in each regime.
#[test]
fn test_avx2_strict_error_path_preserves_canaries() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    let decoder = Avx2Decoder::new();

    let test_cases: &[(usize, usize)] = &[
        (12, 3),    // scalar tail only
        (24, 15),   // tail16 regime
        (192, 100), // inside first DS128 block
        (384, 200), // inside double-DS128 loop
        (768, 500), // deep in hot loop
    ];

    for &(raw_len, corrupt_pos) in test_cases {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(47)).collect();
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
            let canary = 0xD6u8;
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

/// Malformed padding must be rejected through the AVX2 frontend regardless
/// of input size. We test with inputs large enough to exercise the SIMD
/// regimes before hitting the scalar tail that handles padding.
#[test]
fn test_avx2_rejects_malformed_padding() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    let decoder_strict = Avx2Decoder::new();
    let decoder_non_strict = Avx2Decoder::with_opts(DecodeOpts { strict: false });

    // Prefix sizes chosen to exercise different AVX2 regimes.
    // All multiples of 3 so encoding has no padding.
    let prefix_raw_sizes: &[usize] = &[
        0,   // empty prefix -> only tail
        3,   // 4 bytes encoded, scalar only
        15,  // 20 bytes encoded, just enters tail16
        72,  // 96 bytes encoded, below single-DS128
        147, // 196 bytes encoded, single-DS128 threshold
        246, // 328 bytes encoded, double-DS128 threshold
        342, // 456 bytes encoded, triple-DS128 threshold
        480, // 640 bytes encoded, deep in triple-DS128
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
                "strict AVX2 should reject {:?} tail at prefix_raw_len={prefix_raw_len}",
                std::str::from_utf8(bad_tail).unwrap_or("<invalid>")
            );

            let non_strict_result =
                decoder_non_strict.decode_to_slice(&full_input, &mut out_non_strict);
            assert!(
                non_strict_result.is_none(),
                "non-strict AVX2 should reject {:?} tail at prefix_raw_len={prefix_raw_len}",
                std::str::from_utf8(bad_tail).unwrap_or("<invalid>")
            );
        }
    }
}

/// Unchecked path exact-window canary test at AVX2 regime boundaries.
#[test]
fn test_avx2_unchecked_exact_window_regime_boundaries() {
    if !is_x86_feature_detected!("avx2") {
        return;
    }

    for &enc_len in &[
        16usize, 20, 24, 96, 100, 192, 196, 200, 320, 324, 328, 456, 460, 584, 840,
    ] {
        if enc_len % 4 != 0 {
            continue;
        }
        let raw_len = (enc_len / 4) * 3;

        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(53)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let enc_written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(enc_written);

        let expected = decode_scalar_reference(&encoded).expect("scalar should work");

        let pre = 11usize;
        let post = 23usize;
        let canary = 0xA9u8;
        let mut backing = vec![canary; pre + raw_len + post];
        let written = {
            let out = &mut backing[pre..pre + raw_len];
            decode_avx2_unchecked(&encoded, out)
        }
        .unwrap_or_else(|| panic!("unchecked canary decode failed at enc_len={enc_len}"));

        assert_eq!(
            written, raw_len,
            "unchecked length mismatch at enc_len={enc_len}"
        );
        assert_eq!(
            &backing[pre..pre + written],
            expected.as_slice(),
            "unchecked content mismatch at enc_len={enc_len}"
        );
        assert!(
            backing[..pre].iter().all(|&b| b == canary),
            "unchecked leading canaries corrupted at enc_len={enc_len}"
        );
        assert!(
            backing[pre + raw_len..].iter().all(|&b| b == canary),
            "unchecked trailing canaries corrupted at enc_len={enc_len}"
        );
    }
}
