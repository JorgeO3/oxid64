#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

use oxid64::engine::DecodeOpts;
use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use proptest::prelude::*;

fn has_avx512vbmi_backend() -> bool {
    is_x86_feature_detected!("avx512f")
        && is_x86_feature_detected!("avx512bw")
        && is_x86_feature_detected!("avx512vbmi")
}

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

fn decode_avx512_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Avx512VbmiDecoder::new();
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_avx512_non_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

/// Malformed padding must be rejected through the AVX-512 VBMI frontend
/// regardless of input size. We test with inputs large enough to exercise
/// the SIMD regimes before hitting the scalar tail that handles padding.
#[test]
fn test_avx512_rejects_malformed_padding() {
    if !has_avx512vbmi_backend() {
        return;
    }

    let decoder_strict = Avx512VbmiDecoder::new();
    let decoder_non_strict = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });

    // Prefix sizes chosen to exercise different AVX-512 VBMI regimes.
    // All multiples of 3 so encoding has no padding.
    let prefix_raw_sizes: &[usize] = &[
        0,   // empty prefix -> only tail
        3,   // 4 bytes encoded, scalar only
        63,  // 84 bytes encoded, just below tail64 threshold
        66,  // 88 bytes encoded, tail64 threshold
        291, // 388 bytes encoded, just below single-DS256
        294, // 392 bytes encoded, single-DS256 threshold
        486, // 648 bytes encoded, double-DS256 threshold
        720, // 960 bytes encoded, deep in double-DS256
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
                "strict AVX-512 should reject {:?} tail at prefix_raw_len={prefix_raw_len}",
                std::str::from_utf8(bad_tail).unwrap_or("<invalid>")
            );

            let non_strict_result =
                decoder_non_strict.decode_to_slice(&full_input, &mut out_non_strict);
            assert!(
                non_strict_result.is_none(),
                "non-strict AVX-512 should reject {:?} tail at prefix_raw_len={prefix_raw_len}",
                std::str::from_utf8(bad_tail).unwrap_or("<invalid>")
            );
        }
    }
}

fn decode_with_canary<F>(out_len: usize, mut f: F) -> Option<usize>
where
    F: FnMut(&mut [u8]) -> Option<usize>,
{
    let pre = 11usize;
    let post = 23usize;
    let canary = 0xA5u8;
    let mut backing = vec![canary; pre + out_len + post];
    let written = {
        let out = &mut backing[pre..pre + out_len];
        f(out)
    }?;
    assert!(backing[..pre].iter().all(|&b| b == canary));
    assert!(backing[pre + out_len..].iter().all(|&b| b == canary));
    Some(written)
}

/// Decode into an exact-fit canary buffer, returning decoded bytes on success.
fn decode_avx512_exact_canary(
    decoder: &Avx512VbmiDecoder,
    encoded: &[u8],
    expected_out_len: usize,
) -> Option<Vec<u8>> {
    let pre = 13usize;
    let post = 19usize;
    let canary = 0xE5u8;
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

/// Exact-window canary test at every AVX-512 VBMI regime transition boundary.
///
/// AVX-512 regime thresholds (from avx512vbmi model constants):
///   - Double-DS256: `remaining > 644` → enters at encoded_len >= 648
///   - Single-DS256: `remaining > 388` → enters at encoded_len >= 392
///   - Tail64:       `remaining > 84`  → enters at encoded_len >= 88
///   - Entry gate:   `inlen >= 388`    → SIMD entry (preload + single DS256)
///
/// We test encoded lengths at `threshold - 4`, `threshold`, `threshold + 4`
/// for each boundary, plus drain points, using exact-fit canary buffers.
#[test]
fn test_avx512_exact_window_regime_boundaries() {
    if !has_avx512vbmi_backend() {
        return;
    }

    let decoder_strict = Avx512VbmiDecoder::new();
    let decoder_non_strict = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });

    let critical_encoded_lens: Vec<usize> = {
        let mut lens = Vec::new();

        // Tail64 threshold: remaining > 84 → enters at encoded >= 88
        for &t in &[80, 84, 88, 92] {
            lens.push(t);
        }

        // Below SIMD entry but above tail64: scalar only + some tail64
        for &t in &[128, 132, 136, 252, 256, 260] {
            lens.push(t);
        }

        // Single-DS256 entry: inlen >= 388 → enters at 388; remaining > 388 → 392
        for &t in &[384, 388, 392, 396] {
            lens.push(t);
        }

        // Double-DS256 entry: remaining > 644 → enters at 648
        for &t in &[640, 644, 648, 652] {
            lens.push(t);
        }

        // After single-DS256 (consumed 256): remaining for tail64
        // tail64 needs remaining > 84 → in_len - 256 > 84 → in_len > 340 → 344
        // But single-DS256 requires in_len >= 392, so after it:
        // remaining = in_len - 256 - consumed_by_SIMD_header
        for &t in &[456, 460, 464] {
            lens.push(t);
        }

        // After double-DS256 (consumed 512): tail64 drainage
        for &t in &[712, 716, 720] {
            lens.push(t);
        }

        // Larger: multiple double-DS256 iterations
        for &t in &[900, 904, 1156, 1160, 2048] {
            lens.push(t);
        }

        lens.sort();
        lens.dedup();
        lens.retain(|&l| l % 4 == 0);
        lens
    };

    for &enc_len in &critical_encoded_lens {
        let raw_len = (enc_len / 4) * 3;

        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(59)).collect();
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
        let result_strict = decode_avx512_exact_canary(&decoder_strict, &encoded, raw_len)
            .unwrap_or_else(|| {
                panic!("strict canary decode failed at enc_len={enc_len} raw_len={raw_len}")
            });
        assert_eq!(
            result_strict, expected,
            "strict decode mismatch at enc_len={enc_len}"
        );

        // Non-strict
        let result_non_strict = decode_avx512_exact_canary(&decoder_non_strict, &encoded, raw_len)
            .unwrap_or_else(|| {
                panic!("non-strict canary decode failed at enc_len={enc_len} raw_len={raw_len}")
            });
        assert_eq!(
            result_non_strict, expected,
            "non-strict decode mismatch at enc_len={enc_len}"
        );
    }
}

/// Error-path canary test for AVX-512 VBMI: invalid input must not corrupt
/// canary bytes even after SIMD stores (deferred error check).
#[test]
fn test_avx512_error_path_preserves_canaries() {
    if !has_avx512vbmi_backend() {
        return;
    }

    let decoder = Avx512VbmiDecoder::new();

    let test_cases: &[(usize, usize)] = &[
        (12, 3),      // scalar tail only
        (96, 60),     // tail64 regime
        (384, 200),   // inside single-DS256 block
        (768, 500),   // inside double-DS256 loop
        (1536, 1000), // deep in hot loop
    ];

    for &(raw_len, corrupt_pos) in test_cases {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(61)).collect();
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
            let canary = 0xD5u8;
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

proptest! {
    #[test]
    fn test_avx512_decode_matches_scalar(ref input in any::<Vec<u8>>()) {
        let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(input, &mut encoded);

        if has_avx512vbmi_backend() {
            let expected = decode_scalar_reference(&encoded);
            let actual_strict = decode_avx512_strict(&encoded);
            prop_assert_eq!(expected.clone(), actual_strict);
            let actual_non_strict = decode_avx512_non_strict(&encoded);
            prop_assert_eq!(expected, actual_non_strict);
        }
    }
}

#[test]
fn test_avx512_decode_specific_lengths() {
    if !has_avx512vbmi_backend() {
        return;
    }
    let max_len = if cfg!(miri) { 64usize } else { 2048usize };
    for len in 0..max_len {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_avx512_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_avx512_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_avx512_decode_large_lengths() {
    if !has_avx512vbmi_backend() {
        return;
    }
    // Exercise the double-DS256 unrolled loop (512+ input bytes)
    let sizes: &[usize] = if cfg!(miri) {
        &[512, 768, 1024]
    } else {
        &[512, 768, 1024, 2048, 4096, 8192, 65536, 1048576]
    };
    for &len in sizes {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_avx512_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_avx512_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_avx512_decode_invalid_chars() {
    if !has_avx512vbmi_backend() {
        return;
    }
    let invalid_inputs = [
        "AAA*".as_bytes(),
        "AAAA*AAA".as_bytes(),
        "AAAAAAAAAAAA*AAA".as_bytes(),
        "AAAAAAAAAAAAAAAAAAAA*AAA".as_bytes(),
        // Longer invalid inputs to exercise SIMD loops
        &{
            let mut v = vec![b'A'; 256];
            v[200] = b'*';
            v
        }[..],
        &{
            let mut v = vec![b'A'; 1024];
            v[500] = b'\x80';
            v
        }[..],
    ];

    for input in invalid_inputs {
        let actual_strict = decode_avx512_strict(input);
        assert!(
            actual_strict.is_none(),
            "Strict should have failed for input of len {} with invalid char",
            input.len()
        );
        let actual_non_strict = decode_avx512_non_strict(input);
        assert!(
            actual_non_strict.is_none(),
            "Non-strict should have failed for input of len {} with invalid char",
            input.len()
        );
    }
}

#[test]
fn test_avx512_decode_preserves_canaries_with_exact_output_window() {
    if !has_avx512vbmi_backend() {
        return;
    }

    let decoder = Avx512VbmiDecoder::new();
    for len in [0usize, 1, 2, 3, 15, 16, 17, 63, 64, 65, 191, 192, 193, 1024] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(29)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);
        let out_len = raw.len();

        let written = decode_with_canary(out_len, |out| decoder.decode_to_slice(&encoded, out))
            .expect("strict decode should succeed on valid input");
        assert_eq!(written, out_len, "decoded length mismatch at len={len}");
    }
}

#[test]
fn test_avx512_decode_misaligned_output_and_input_subslice() {
    if !has_avx512vbmi_backend() {
        return;
    }

    let decoder = Avx512VbmiDecoder::new();
    for len in [0usize, 1, 2, 3, 63, 64, 65, 255, 256, 257, 1024] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(17)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        let mut in_backing = vec![b'@'; encoded.len() + 9];
        in_backing[4..4 + encoded.len()].copy_from_slice(&encoded);
        let input = &in_backing[4..4 + encoded.len()];

        for out_offset in 1usize..4 {
            let out_len = raw.len();
            let canary = 0xABu8;
            let mut out_backing = vec![canary; out_len + out_offset + 15];
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
