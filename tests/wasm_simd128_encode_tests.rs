#![cfg(target_arch = "wasm32")]

use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::engine::wasm_simd128::WasmSimd128Decoder;

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

fn encode_with_canary<F>(out_len: usize, mut f: F) -> Vec<u8>
where
    F: FnMut(&mut [u8]) -> usize,
{
    let pre = 5usize;
    let post = 17usize;
    let canary = 0x5Cu8;
    let mut backing = vec![canary; pre + out_len + post];
    let written = {
        let out = &mut backing[pre..pre + out_len];
        f(out)
    };
    assert_eq!(written, out_len);
    assert!(backing[..pre].iter().all(|&b| b == canary));
    assert!(backing[pre + out_len..].iter().all(|&b| b == canary));
    backing[pre..pre + out_len].to_vec()
}

#[test]
fn test_wasm_encode_matches_scalar() {
    for len in [0usize, 1, 2, 3, 11, 12, 13, 47, 48, 49, 95, 96, 97, 511] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(13)).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_wasm(&input);
        assert_eq!(expected, actual, "encode mismatch at len={len}");
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

#[test]
fn test_wasm_encode_misaligned_and_canary_safe() {
    let dec = WasmSimd128Decoder::new();
    for len in [
        0usize, 1, 2, 3, 47, 48, 49, 51, 52, 59, 60, 95, 96, 97, 107, 108, 512,
    ] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(31)).collect();
        let expected = encode_scalar_reference(&input);
        let out_len = expected.len();

        for offset in 1usize..4 {
            let canary = 0xD2u8;
            let mut backing = vec![canary; out_len + offset + 13];
            let written = dec.encode_to_slice(&input, &mut backing[offset..offset + out_len]);
            assert_eq!(written, out_len);
            assert_eq!(&backing[offset..offset + out_len], expected.as_slice());
            assert!(backing[..offset].iter().all(|&b| b == canary));
            assert!(backing[offset + out_len..].iter().all(|&b| b == canary));
        }

        let got = encode_with_canary(out_len, |out| dec.encode_to_slice(&input, out));
        assert_eq!(got, expected);
    }
}

#[test]
fn test_wasm_encode_boundary_lengths_match_scalar() {
    for len in [
        11usize, 12, 13, 47, 48, 49, 51, 52, 59, 60, 95, 96, 97, 107, 108,
    ] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(17)).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_wasm(&input);
        assert_eq!(expected, actual, "Failed at boundary length {len}");
    }
}
