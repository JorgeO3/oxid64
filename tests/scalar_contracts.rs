use oxid64::engine::scalar::{
    decode_base64_fast, decode_tail_3, decoded_len_strict, encode_base64_fast, ScalarDecoder,
};
use oxid64::Base64Decoder;

fn fill_xorshift(buf: &mut [u8]) {
    let mut x = 0x1234_5678_9abc_def0_u64;
    for b in buf {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = x as u8;
    }
}

#[test]
fn matches_rfc4648_vectors() {
    const CASES: [(&[u8], &str); 7] = [
        (b"", ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
    ];

    for (input, expected) in CASES {
        let mut out = vec![0u8; input.len().div_ceil(3) * 4 + 8];
        let written = encode_base64_fast(input, &mut out);
        assert_eq!(&out[..written], expected.as_bytes(), "encode mismatch");

        let mut decoded = vec![0u8; input.len() + 8];
        let decoded_len =
            decode_base64_fast(expected.as_bytes(), &mut decoded).expect("decode failed");
        assert_eq!(decoded_len, input.len(), "decode len mismatch");
        assert_eq!(&decoded[..decoded_len], input, "decode mismatch");
    }
}

#[test]
fn decode_roundtrip_many_sizes() {
    let max_len = if cfg!(miri) { 64usize } else { 8192usize };
    for len in 0..=max_len {
        let mut input = vec![0u8; len];
        fill_xorshift(&mut input);

        let mut encoded = vec![0u8; len.div_ceil(3) * 4 + 8];
        let enc_written = encode_base64_fast(&input, &mut encoded);
        let encoded = &encoded[..enc_written];

        let expected_len = decoded_len_strict(encoded).expect("decoded_len_strict failed");
        assert_eq!(expected_len, len, "decoded_len mismatch at len={len}");

        let mut out = vec![0u8; len + 8];
        let written = decode_base64_fast(encoded, &mut out).expect("decode failed");
        assert_eq!(written, len, "len mismatch at len={len}");
        assert_eq!(&out[..written], input.as_slice(), "mismatch at len={len}");
    }
}

#[test]
fn decode_rejects_invalid() {
    let mut out = [0u8; 64];
    assert_eq!(decoded_len_strict(b""), Some(0));
    assert!(decode_base64_fast(b"A", &mut out).is_none());
    assert!(decode_base64_fast(b"AAA", &mut out).is_none());
    assert!(decode_base64_fast(b"AAAAA", &mut out).is_none());
    assert!(decode_base64_fast(b"A===", &mut out).is_none());
    assert!(decode_base64_fast(b"AA*A", &mut out).is_none());
    assert!(decode_base64_fast(b"AA=A", &mut out).is_none());
    assert!(decode_base64_fast(b"TWG=TWG=", &mut out).is_none());
}

#[test]
fn decoded_len_strict_rejects_misplaced_padding() {
    // Padding in data positions of the last quad must be rejected.
    assert!(
        decoded_len_strict(b"=AAA").is_none(),
        "= in pos 0 of last quad"
    );
    assert!(
        decoded_len_strict(b"A=AA").is_none(),
        "= in pos 1 of last quad"
    );
    assert!(
        decoded_len_strict(b"AA=A").is_none(),
        "= in pos 2 without pos 3"
    );
    assert!(decoded_len_strict(b"A===").is_none(), "x=== pattern");

    // Multi-quad: padding in the middle.
    assert!(
        decoded_len_strict(b"AAAA=AAA").is_none(),
        "= in data position of non-final quad"
    );
    assert!(
        decoded_len_strict(b"AAAA=A==").is_none(),
        "= in pos 0 of last quad, multi"
    );
    assert!(
        decoded_len_strict(b"AAAAA=AA").is_none(),
        "= in pos 1 of last quad, multi"
    );

    // Valid padding patterns.
    assert_eq!(decoded_len_strict(b"AA=="), Some(1));
    assert_eq!(decoded_len_strict(b"AAA="), Some(2));
    assert_eq!(decoded_len_strict(b"AAAA"), Some(3));
    assert_eq!(decoded_len_strict(b"AAAAAAAA"), Some(6));
    assert_eq!(decoded_len_strict(b"AAAAAA=="), Some(4));
    assert_eq!(decoded_len_strict(b"AAAAAAA="), Some(5));
}

#[test]
fn decode_tail_3_handles_padding_contracts() {
    let mut out = [0u8; 3];
    let (written, _) = decode_tail_3(b"TQ==", &mut out).expect("single-byte tail must decode");
    assert_eq!(written, 1);
    assert_eq!(&out[..written], b"M");

    let (written, _) = decode_tail_3(b"TWE=", &mut out).expect("two-byte tail must decode");
    assert_eq!(written, 2);
    assert_eq!(&out[..written], b"Ma");

    let (written, _) = decode_tail_3(b"TWFu", &mut out).expect("three-byte tail must decode");
    assert_eq!(written, 3);
    assert_eq!(&out[..written], b"Man");
}

#[test]
fn decode_tail_3_rejects_invalid_contracts() {
    let mut out = [0u8; 3];
    assert!(decode_tail_3(b"T*==", &mut out).is_none());
    assert!(decode_tail_3(b"TW*=", &mut out).is_none());
    assert!(decode_tail_3(b"TWF*", &mut out).is_none());
}

#[test]
fn decode_tail_3_rejects_non_canonical_pad_bits() {
    // RFC 4648 §3.5: trailing bits in the last character before padding
    // must be zero. Non-canonical encodings carry information in bits that
    // are discarded, and a strict decoder must reject them.
    let mut out = [0u8; 3];

    // 2-pad case (XX==): v1 has 6 bits, only top 2 used.
    // 'TR==' → v1 = REV64['R'] = 17 = 0b010001. Bottom 4 bits = 0b0001 != 0.
    assert!(
        decode_tail_3(b"TR==", &mut out).is_none(),
        "non-canonical 2-pad must be rejected"
    );
    // 'TQ==' is canonical (v1 = 16 = 0b010000, bottom 4 bits = 0).
    assert!(
        decode_tail_3(b"TQ==", &mut out).is_some(),
        "canonical 2-pad must be accepted"
    );

    // 1-pad case (XXX=): v2 has 6 bits, only top 4 used.
    // 'TWF=' → v2 = REV64['F'] = 5 = 0b000101. Bottom 2 bits = 0b01 != 0.
    assert!(
        decode_tail_3(b"TWF=", &mut out).is_none(),
        "non-canonical 1-pad must be rejected"
    );
    // 'TWE=' is canonical (v2 = 4 = 0b000100, bottom 2 bits = 0).
    assert!(
        decode_tail_3(b"TWE=", &mut out).is_some(),
        "canonical 1-pad must be accepted"
    );
    // 'TWA=' is canonical (v2 = 0, bottom 2 bits = 0).
    assert!(
        decode_tail_3(b"TWA=", &mut out).is_some(),
        "canonical 1-pad with zero char must be accepted"
    );

    // Full decode path also rejects non-canonical.
    let mut full_out = [0u8; 16];
    assert!(
        decode_base64_fast(b"TR==", &mut full_out).is_none(),
        "full decode must reject non-canonical 2-pad"
    );
    assert!(
        decode_base64_fast(b"TWF=", &mut full_out).is_none(),
        "full decode must reject non-canonical 1-pad"
    );
}

#[test]
fn scalar_decoder_handles_exact_windows_and_subslices() {
    let dec = ScalarDecoder;

    for len in [0usize, 1, 2, 3, 15, 16, 17, 63, 64, 65] {
        let mut input = vec![0u8; len];
        fill_xorshift(&mut input);

        let mut encoded = vec![0u8; len.div_ceil(3) * 4];
        let enc_written = encode_base64_fast(&input, &mut encoded);
        encoded.truncate(enc_written);

        let mut encoded_backing = vec![b'!'; encoded.len() + 5];
        encoded_backing[2..2 + encoded.len()].copy_from_slice(&encoded);
        let encoded_slice = &encoded_backing[2..2 + encoded.len()];

        for offset in 1usize..4 {
            let canary = 0xB7u8;
            let mut out_backing = vec![canary; input.len() + offset + 7];
            let out = &mut out_backing[offset..offset + input.len()];
            let written = dec
                .decode_to_slice(encoded_slice, out)
                .expect("scalar decode should succeed");
            assert_eq!(written, input.len());
            assert_eq!(out, input.as_slice());
            assert!(out_backing[..offset].iter().all(|&b| b == canary));
            assert!(out_backing[offset + input.len()..]
                .iter()
                .all(|&b| b == canary));
        }
    }
}
