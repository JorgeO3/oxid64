//! Proptest differential: auto-detected Decoder vs. scalar oracle.
//!
//! Verifies that `Decoder::detect()` (which selects the best available SIMD
//! backend at runtime) produces identical encode/decode results to the scalar
//! reference implementation for arbitrary inputs.

use oxid64::engine::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use oxid64::{Base64Decoder, Decoder};
use proptest::prelude::*;

/// Scalar oracle: encode raw bytes to Base64.
fn encode_scalar(raw: &[u8]) -> Vec<u8> {
    let enc_len = raw.len().div_ceil(3) * 4;
    let mut buf = vec![0u8; enc_len];
    let written = encode_base64_fast(raw, &mut buf);
    buf.truncate(written);
    buf
}

/// Scalar oracle: decode Base64 to raw bytes.
fn decode_scalar(encoded: &[u8]) -> Option<Vec<u8>> {
    let out_len = decoded_len_strict(encoded)?;
    let mut buf = vec![0u8; out_len];
    let written = decode_base64_fast(encoded, &mut buf)?;
    buf.truncate(written);
    Some(buf)
}

/// Max input size for proptest vectors (smaller under Miri to avoid timeouts).
const MAX_INPUT_LEN: usize = if cfg!(miri) { 128 } else { 2048 };

proptest! {
    #![proptest_config(ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 1000 },
        .. ProptestConfig::default()
    })]

    /// Encode via auto-detected backend must match scalar oracle.
    #[test]
    fn encode_roundtrip_matches_scalar(raw in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let detector = Decoder::detect();
        let expected = encode_scalar(&raw);
        let actual = detector.encode(&raw);
        prop_assert_eq!(actual, expected, "encode mismatch at len={}", raw.len());
    }

    /// Decode (of self-encoded input) via auto-detected backend must match scalar oracle.
    #[test]
    fn decode_roundtrip_matches_scalar(raw in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let detector = Decoder::detect();
        let encoded = encode_scalar(&raw);

        let expected = decode_scalar(&encoded).expect("scalar oracle should decode self-encoded data");
        let actual = detector.decode(&encoded).expect("auto-detected decoder should decode self-encoded data");
        prop_assert_eq!(actual.as_slice(), expected.as_slice(), "decode mismatch at raw_len={}", raw.len());
    }

    /// Full roundtrip: encode via detected → decode via detected → matches original.
    #[test]
    fn full_roundtrip(raw in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let detector = Decoder::detect();
        let encoded = detector.encode(&raw);
        let decoded = detector.decode(&encoded)
            .expect("auto-detected decoder should decode its own output");
        prop_assert_eq!(decoded.as_slice(), raw.as_slice(), "roundtrip mismatch at len={}", raw.len());
    }

    /// Invalid Base64 must be rejected by both scalar and auto-detected backend.
    #[test]
    fn rejects_invalid_base64(raw in prop::collection::vec(any::<u8>(), 1..MAX_INPUT_LEN.min(512))) {
        let encoded = encode_scalar(&raw);
        if encoded.is_empty() {
            return Ok(());
        }

        // Corrupt a non-padding position.
        let pad_count = encoded.iter().rev().take_while(|&&b| b == b'=').count();
        let valid_end = encoded.len().saturating_sub(pad_count);
        if valid_end == 0 {
            return Ok(());
        }
        let corrupt_idx = valid_end / 2;

        let mut bad = encoded.clone();
        bad[corrupt_idx] = b'*';

        let detector = Decoder::detect();

        // Scalar must reject.
        let scalar_result = decode_scalar(&bad);
        prop_assert!(scalar_result.is_none(), "scalar should reject corrupted input");

        // Auto-detected backend must also reject.
        let auto_result = detector.decode(&bad);
        prop_assert!(auto_result.is_none(), "auto-detected decoder should reject corrupted input");
    }
}
