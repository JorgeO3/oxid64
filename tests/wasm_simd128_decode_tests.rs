#![cfg(target_arch = "wasm32")]

use oxid64::engine::DecodeOpts;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::wasm_simd128::WasmSimd128Decoder;

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

fn decode_wasm_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = WasmSimd128Decoder::new();
    let written = decoder.decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_wasm_non_strict(input: &[u8]) -> Option<Vec<u8>> {
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
    let decoder = WasmSimd128Decoder::with_opts(DecodeOpts { strict: false });
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
    let safe_end = encoded_len.saturating_sub(4);
    let mut ip = 0usize;

    if !(ip + 32 <= safe_end) {
        return true;
    }

    while ip + 32 + 128 <= safe_end {
        if pos >= ip && pos < ip + 64 {
            return (pos - ip) < 16;
        }
        if pos >= ip + 64 && pos < ip + 128 {
            return (pos - (ip + 64)) < 16;
        }
        ip += 128;
    }

    while ip + 32 + 64 <= safe_end {
        if pos >= ip && pos < ip + 64 {
            return (pos - ip) < 16;
        }
        ip += 64;
    }

    while ip + 16 <= safe_end {
        if pos >= ip && pos < ip + 16 {
            return true;
        }
        ip += 16;
    }

    true
}

#[test]
fn test_wasm_decode_matches_scalar() {
    for len in [
        0usize, 1, 2, 3, 15, 16, 17, 63, 64, 65, 95, 96, 97, 143, 144, 145, 511,
    ] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(17)).collect();
        let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_wasm_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "strict mismatch at len={len}"
        );
        let actual_non_strict = decode_wasm_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "non-strict mismatch at len={len}"
        );
    }
}

#[test]
fn test_wasm_decode_specific_lengths() {
    for len in 0usize..2048 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_wasm_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_wasm_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_wasm_decode_large_lengths() {
    // Exercise the double-DS64 unrolled loop (128+ input bytes)
    for len in [512usize, 768, 1024, 2048, 4096, 8192, 65536, 1048576] {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_wasm_strict(&encoded);
        assert_eq!(
            expected.clone(),
            actual_strict,
            "Strict failed at length {}",
            len
        );
        let actual_non_strict = decode_wasm_non_strict(&encoded);
        assert_eq!(
            expected, actual_non_strict,
            "Non-strict failed at length {}",
            len
        );
    }
}

#[test]
fn test_wasm_decode_invalid_chars() {
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
        let actual_strict = decode_wasm_strict(input);
        assert!(
            actual_strict.is_none(),
            "Strict should have failed for input of len {} with invalid char",
            input.len()
        );
    }
}

#[test]
fn test_wasm_decode_valid_alphabet_regression_for_simd() {
    let encoded = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let expected = decode_scalar_reference(encoded)
        .expect("scalar decode should accept valid base64 alphabet blocks");

    let strict = decode_wasm_strict(encoded).expect("strict wasm decode should succeed");
    assert_eq!(strict, expected);

    let non_strict = decode_wasm_non_strict(encoded)
        .expect("non-strict wasm decode should succeed on valid input");
    assert_eq!(non_strict, expected);
}

#[test]
fn test_wasm_non_strict_matches_documented_schedule() {
    for raw_len in [96usize, 144] {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(27)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        for pos in 0..encoded.len() {
            if encoded[pos] == b'=' {
                continue;
            }

            let mut bad = encoded.clone();
            bad[pos] = b'*';
            let actual = decode_wasm_non_strict(&bad);
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
fn test_wasm_decode_rejects_high_bit_bytes_in_strict_mode() {
    let raw: Vec<u8> = (0..96).map(|i| (i % 256) as u8).collect();
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let _ = encode_base64_fast(&raw, &mut encoded);
    assert_eq!(encoded.len(), 128);

    for &byte in &[0x80u8, 0xFFu8] {
        for &pos in &[0usize, 15, 16, 31, 32, 47, 48, 63, 64, 111] {
            let mut bad = encoded.clone();
            bad[pos] = byte;
            let actual = decode_wasm_strict(&bad);
            assert!(
                actual.is_none(),
                "strict should reject high-bit byte {byte:#04x} at pos={pos}"
            );
        }
    }
}

#[test]
fn test_wasm_decode_strict_rejects_invalid_every_position_in_ds64_regimes() {
    for raw_len in [96usize, 144] {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(13)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        for pos in 0..encoded.len() {
            if encoded[pos] == b'=' {
                continue;
            }

            let mut bad = encoded.clone();
            bad[pos] = b'*';
            assert!(
                decode_wasm_strict(&bad).is_none(),
                "strict wasm decode accepted invalid byte at encoded_len={}, pos={pos}",
                encoded.len()
            );
        }
    }
}

#[test]
fn test_wasm_decode_valid_padding_at_simd_boundaries() {
    let strict = WasmSimd128Decoder::new();
    let non_strict = WasmSimd128Decoder::with_opts(DecodeOpts { strict: false });

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
fn test_wasm_decode_misaligned_output_and_input_subslice() {
    let decoder = WasmSimd128Decoder::new();
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

#[test]
fn test_wasm_decode_preserves_canaries_with_exact_output_window() {
    let decoder = WasmSimd128Decoder::new();
    for len in [0usize, 1, 2, 3, 63, 64, 65, 191, 192, 193, 1024] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(41)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let _ = encode_base64_fast(&raw, &mut encoded);

        let written = decode_with_canary(raw.len(), |out| decoder.decode_to_slice(&encoded, out))
            .expect("strict decode should succeed on valid input");
        assert_eq!(written, raw.len(), "decoded length mismatch at len={len}");
    }
}
