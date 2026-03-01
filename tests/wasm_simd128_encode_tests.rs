#![cfg(target_arch = "wasm32")]

use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::wasm_simd128::WasmSimd128Decoder;
use proptest::prelude::*;

fn encode_scalar_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = encode_base64_fast(input, &mut out);
    out.truncate(n);
    out
}

fn encode_wasm(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = WasmSimd128Decoder::new().encode_to_slice(input, &mut out);
    out.truncate(n);
    out
}

proptest! {
    #[test]
    fn test_wasm_encode_matches_scalar(ref input in any::<Vec<u8>>()) {
        let expected = encode_scalar_reference(input);
        let actual = encode_wasm(input);
        prop_assert_eq!(expected, actual);
    }
}

#[test]
fn test_wasm_encode_specific_lengths() {
    for len in 0..2048 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_wasm(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_wasm_encode_large_lengths() {
    // Exercise the main 96-byte unrolled loop
    for len in [512, 768, 1024, 2048, 4096, 8192, 65536, 1048576] {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_wasm(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_wasm_encode_roundtrip() {
    for len in [0, 1, 2, 3, 48, 96, 192, 384, 768, 1024] {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let encoded = encode_wasm(&input);

        // Decode back with scalar
        if encoded.is_empty() {
            assert!(input.is_empty());
            continue;
        }
        let pad = if *encoded.last().unwrap() == b'=' {
            if encoded[encoded.len() - 2] == b'=' {
                2
            } else {
                1
            }
        } else {
            0
        };
        let dec_len = (encoded.len() / 4) * 3 - pad;
        let mut decoded = vec![0u8; dec_len];
        let written =
            decode_base64_fast(&encoded, &mut decoded).expect("roundtrip decode should not fail");
        decoded.truncate(written);
        assert_eq!(input, decoded, "Roundtrip failed at length {}", len);
    }
}
