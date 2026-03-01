#![cfg(target_arch = "wasm32")]

use oxid64::engine::DecodeOpts;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::wasm_simd128::WasmSimd128Decoder;
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

proptest! {
    #[test]
    fn test_wasm_decode_matches_scalar(ref input in any::<Vec<u8>>()) {
        let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
        let _enc_len = encode_base64_fast(input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual_strict = decode_wasm_strict(&encoded);
        prop_assert_eq!(expected.clone(), actual_strict);
        let actual_non_strict = decode_wasm_non_strict(&encoded);
        prop_assert_eq!(expected, actual_non_strict);
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
        let actual_non_strict = decode_wasm_non_strict(input);
        assert!(
            actual_non_strict.is_none(),
            "Non-strict should have failed for input of len {} with invalid char",
            input.len()
        );
    }
}
