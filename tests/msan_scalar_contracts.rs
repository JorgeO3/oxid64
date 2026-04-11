use oxid64::Base64Decoder;
use oxid64::engine::scalar::{ScalarDecoder, decode_base64_fast, encode_base64_fast};

fn encoded(input: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; input.len().div_ceil(3) * 4];
    let written = encode_base64_fast(input, &mut out);
    out.truncate(written);
    out
}

#[test]
fn test_scalar_decode_exact_subslice_preserves_canaries() {
    let dec = ScalarDecoder;

    for len in [0usize, 1, 2, 3, 4, 5, 15, 16, 17, 63, 64, 65] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(7)).collect();
        let encoded = encoded(&raw);

        for offset in 1usize..4 {
            let canary = 0xD4u8;
            let mut backing = vec![canary; raw.len() + offset + 9];
            let out = &mut backing[offset..offset + raw.len()];
            let written = dec
                .decode_to_slice(&encoded, out)
                .expect("scalar decode should succeed");

            assert_eq!(written, raw.len(), "decoded len mismatch at raw len={len}");
            assert_eq!(out, raw.as_slice(), "decode mismatch at raw len={len}");
            assert!(backing[..offset].iter().all(|&b| b == canary));
            assert!(backing[offset + raw.len()..].iter().all(|&b| b == canary));
        }
    }
}

#[test]
fn test_scalar_decode_tails_and_padding_boundaries() {
    let cases = [
        (b"".as_slice(), b"".as_slice()),
        (b"Zg==".as_slice(), b"f".as_slice()),
        (b"Zm8=".as_slice(), b"fo".as_slice()),
        (b"Zm9v".as_slice(), b"foo".as_slice()),
        (b"Zm9vYg==".as_slice(), b"foob".as_slice()),
        (b"Zm9vYmE=".as_slice(), b"fooba".as_slice()),
        (b"Zm9vYmFy".as_slice(), b"foobar".as_slice()),
    ];

    for (input, expected) in cases {
        let mut out = vec![0u8; expected.len()];
        let written = decode_base64_fast(input, &mut out).expect("valid tail decode must succeed");
        assert_eq!(written, expected.len());
        assert_eq!(&out[..written], expected);
    }
}

#[test]
fn test_scalar_decode_checked_wrapper_rejects_small_output() {
    let dec = ScalarDecoder;
    let input = b"TWFu";
    let mut out = [0u8; 2];
    assert_eq!(dec.decode_to_slice(input, &mut out), None);
}

#[test]
fn test_scalar_encode_exact_subslice_preserves_canaries() {
    let dec = ScalarDecoder;

    for len in [0usize, 1, 2, 3, 4, 5, 15, 16, 17, 63, 64, 65] {
        let raw: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(11)).collect();
        let expected = encoded(&raw);

        for offset in 1usize..4 {
            let canary = 0xA3u8;
            let mut backing = vec![canary; expected.len() + offset + 7];
            let out = &mut backing[offset..offset + expected.len()];
            let written = dec.encode_to_slice(&raw, out);

            assert_eq!(
                written,
                expected.len(),
                "encoded len mismatch at raw len={len}"
            );
            assert_eq!(out, expected.as_slice(), "encode mismatch at raw len={len}");
            assert!(backing[..offset].iter().all(|&b| b == canary));
            assert!(
                backing[offset + expected.len()..]
                    .iter()
                    .all(|&b| b == canary)
            );
        }
    }
}
