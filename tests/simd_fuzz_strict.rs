use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use oxid64::engine::ssse3::Ssse3Decoder;
use proptest::prelude::*;

const CANARY: u8 = 0xA5;
const PAD: usize = 64;

fn interesting_len() -> impl Strategy<Value = usize> {
    if cfg!(miri) {
        // Under Miri, keep inputs small to avoid excessive interpretation time.
        prop_oneof![
            Just(0usize),
            Just(1),
            Just(2),
            Just(3),
            Just(4),
            Just(15),
            Just(16),
            Just(17),
            Just(31),
            Just(32),
            Just(33),
            Just(48),
            Just(63),
            Just(64),
            Just(65),
            0usize..128usize,
        ]
        .boxed()
    } else {
        prop_oneof![
            Just(0usize),
            Just(1),
            Just(2),
            Just(3),
            Just(4),
            Just(15),
            Just(16),
            Just(17),
            Just(23),
            Just(24),
            Just(25),
            Just(31),
            Just(32),
            Just(33),
            Just(47),
            Just(48),
            Just(49),
            Just(63),
            Just(64),
            Just(65),
            Just(95),
            Just(96),
            Just(97),
            Just(127),
            Just(128),
            Just(129),
            Just(191),
            Just(192),
            Just(193),
            Just(255),
            Just(256),
            Just(257),
            Just(511),
            Just(512),
            Just(513),
            Just(1023),
            Just(1024),
            Just(1025),
            Just(4095),
            Just(4096),
            Just(4097),
            0usize..8192usize,
        ]
        .boxed()
    }
}

fn raw_input_strategy() -> impl Strategy<Value = Vec<u8>> {
    interesting_len().prop_flat_map(|n| proptest::collection::vec(any::<u8>(), n))
}

fn encoded_from_raw(raw: &[u8]) -> Vec<u8> {
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4 + 64];
    let n = encode_base64_fast(raw, &mut encoded);
    encoded.truncate(n);
    encoded
}

fn decode_scalar_vec(encoded: &[u8]) -> Option<Vec<u8>> {
    let out_len = decoded_len_strict(encoded)?;
    let mut out = vec![0u8; out_len];
    let written = decode_base64_fast(encoded, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_with_canary(out_len: usize, f: impl FnOnce(&mut [u8]) -> Option<usize>) {
    let mut backing = vec![CANARY; PAD + out_len + PAD];
    let (prefix, rest) = backing.split_at_mut(PAD);
    let (out_slice, suffix) = rest.split_at_mut(out_len);
    let written = f(out_slice).expect("decode returned None on valid input");
    assert_eq!(written, out_len, "decode wrote unexpected output length");
    assert!(
        prefix.iter().all(|&b| b == CANARY),
        "decode clobbered prefix canary"
    );
    assert!(
        suffix.iter().all(|&b| b == CANARY),
        "decode clobbered suffix canary"
    );
}

fn encode_with_canary(out_len: usize, f: impl FnOnce(&mut [u8]) -> usize) -> Vec<u8> {
    let mut backing = vec![CANARY; PAD + out_len + PAD];
    let (prefix, rest) = backing.split_at_mut(PAD);
    let (out_slice, suffix) = rest.split_at_mut(out_len);
    let written = f(out_slice);
    let out = out_slice.to_vec();
    assert_eq!(written, out_len, "encode wrote unexpected output length");
    assert!(
        prefix.iter().all(|&b| b == CANARY),
        "encode clobbered prefix canary"
    );
    assert!(
        suffix.iter().all(|&b| b == CANARY),
        "encode clobbered suffix canary"
    );
    out
}

fn mutate_to_invalid_b64(encoded: &[u8], idx: usize) -> Vec<u8> {
    let invalid_bytes = [b'*', b'!', b' ', b'\n', b'~', 0x7f];
    let mut bad = encoded.to_vec();
    bad[idx] = invalid_bytes[idx % invalid_bytes.len()];
    bad
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 600 },
        .. ProptestConfig::default()
    })]

    #[test]
    fn prop_simd_decode_strict_matches_scalar_and_preserves_canaries(raw in raw_input_strategy()) {
        let encoded = encoded_from_raw(&raw);
        let expected = decode_scalar_vec(&encoded).expect("scalar decode failed on self-encoded input");
        let out_len = expected.len();

        if is_x86_feature_detected!("ssse3") {
            let sse = Ssse3Decoder::new();
            decode_with_canary(out_len, |out| sse.decode_to_slice(&encoded, out));
            let mut out = vec![0u8; out_len];
            let n = sse.decode_to_slice(&encoded, &mut out).expect("ssse3 strict decode returned None");
            out.truncate(n);
            prop_assert_eq!(out, expected.clone());
        }

        if is_x86_feature_detected!("avx2") {
            let legacy = Avx2Decoder::new();

            decode_with_canary(out_len, |out| legacy.decode_to_slice(&encoded, out));

            let mut out = vec![0u8; out_len];
            let n = legacy
                .decode_to_slice(&encoded, &mut out)
                .expect("avx2 strict decode returned None");
            out.truncate(n);
            prop_assert_eq!(out, expected.clone());
        }
    }

    #[test]
    fn prop_simd_encode_matches_scalar_and_preserves_canaries(raw in raw_input_strategy()) {
        let expected = encoded_from_raw(&raw);
        let out_len = expected.len();

        if is_x86_feature_detected!("ssse3") {
            let sse = Ssse3Decoder::new();
            let got = encode_with_canary(out_len, |out| sse.encode_to_slice(&raw, out));
            prop_assert_eq!(got, expected.clone());
        }

        if is_x86_feature_detected!("avx2") {
            let avx2 = Avx2Decoder::new();
            let got = encode_with_canary(out_len, |out| avx2.encode_to_slice(&raw, out));
            prop_assert_eq!(got, expected);
        }
    }

    #[test]
    fn prop_strict_rejects_invalid_inputs(raw in raw_input_strategy()) {
        let encoded = encoded_from_raw(&raw);
        if encoded.is_empty() {
            return Ok(());
        }

        let pad = encoded.iter().rev().take_while(|&&b| b == b'=').count();
        let valid_len = encoded.len().saturating_sub(pad);
        if valid_len == 0 {
            return Ok(());
        }

        let idx = valid_len / 2;
        let bad = mutate_to_invalid_b64(&encoded, idx);

        let mut scalar_out = vec![0u8; raw.len() + 16];
        prop_assert!(decode_base64_fast(&bad, &mut scalar_out).is_none());

        if is_x86_feature_detected!("ssse3") {
            let mut out = vec![0u8; raw.len() + 16];
            prop_assert!(Ssse3Decoder::new().decode_to_slice(&bad, &mut out).is_none());
        }

        if is_x86_feature_detected!("avx2") {
            let mut out = vec![0u8; raw.len() + 16];
            prop_assert!(Avx2Decoder::new().decode_to_slice(&bad, &mut out).is_none());
        }
    }
}
