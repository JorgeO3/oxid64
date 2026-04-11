use oxid64::engine::DecodeOpts;
use oxid64::engine::models::ssse3::{
    aligned_non_strict_checks_offset, aligned_touched_prefix_before_error,
    aligned_written_prefix_before_error,
};
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::ssse3::Ssse3Decoder;

#[repr(align(16))]
struct Aligned<const N: usize>([u8; N]);

fn encoded_valid(raw_len: usize) -> (Vec<u8>, Vec<u8>) {
    let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(29)).collect();
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let n = encode_base64_fast(&raw, &mut encoded);
    encoded.truncate(n);
    (raw, encoded)
}

fn invalidate_one(encoded: &[u8], pos: usize) -> Vec<u8> {
    let mut out = encoded.to_vec();
    out[pos] = b'*';
    out
}

#[test]
fn ssse3_model_schedule_and_prefix_bounds() {
    assert!(aligned_non_strict_checks_offset(128, 0));
    assert!(!aligned_non_strict_checks_offset(128, 20));
    assert!(aligned_non_strict_checks_offset(128, 80));
    assert!(aligned_non_strict_checks_offset(128, 120));

    assert!(aligned_non_strict_checks_offset(192, 0));
    assert!(!aligned_non_strict_checks_offset(192, 20));
    assert!(aligned_non_strict_checks_offset(192, 64));
    assert!(!aligned_non_strict_checks_offset(192, 96));
    assert!(aligned_non_strict_checks_offset(192, 144));
    assert!(aligned_non_strict_checks_offset(192, 180));

    assert_eq!(aligned_written_prefix_before_error(128), 84);
    assert_eq!(aligned_written_prefix_before_error(192), 132);
    assert_eq!(aligned_touched_prefix_before_error(128), 88);
    assert_eq!(aligned_touched_prefix_before_error(192), 136);
}

#[test]
fn ssse3_strict_rejects_invalid_every_position_single_and_double_ds64() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let dec = Ssse3Decoder::new();
    for raw_len in [96usize, 144] {
        let (raw, encoded) = encoded_valid(raw_len);
        for pos in 0..encoded.len() {
            if encoded[pos] == b'=' {
                continue;
            }

            let bad = invalidate_one(&encoded, pos);
            let mut out = Aligned([0u8; 160]);
            assert!(
                dec.decode_to_slice(&bad, &mut out.0[..raw.len()]).is_none(),
                "strict ssse3 accepted invalid byte at pos={pos} for encoded_len={}",
                encoded.len()
            );
        }
    }
}

#[test]
fn ssse3_non_strict_matches_documented_schedule() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
    for raw_len in [96usize, 144] {
        let (raw, encoded) = encoded_valid(raw_len);
        for pos in 0..encoded.len() {
            if encoded[pos] == b'=' {
                continue;
            }

            let bad = invalidate_one(&encoded, pos);
            let mut out = Aligned([0u8; 160]);
            let got = dec.decode_to_slice(&bad, &mut out.0[..raw.len()]);
            assert_eq!(
                got.is_none(),
                aligned_non_strict_checks_offset(encoded.len(), pos),
                "non-strict schedule mismatch at pos={pos} for encoded_len={}",
                encoded.len()
            );
        }
    }
}

#[test]
fn ssse3_strict_invalid_decode_has_bounded_partial_writes() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let dec = Ssse3Decoder::new();
    for raw_len in [96usize, 144] {
        let (raw, encoded) = encoded_valid(raw_len);
        let bound = aligned_touched_prefix_before_error(encoded.len());
        let bad = invalidate_one(&encoded, 0);
        let mut out = Aligned([0xA5u8; 160]);

        let got = dec.decode_to_slice(&bad, &mut out.0[..raw.len()]);
        assert!(got.is_none());
        assert!(out.0[..bound].iter().any(|&b| b != 0xA5));
        assert!(out.0[bound..raw.len()].iter().all(|&b| b == 0xA5));
    }
}

#[test]
fn ssse3_non_strict_invalid_decode_has_bounded_partial_writes() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }

    let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
    for raw_len in [96usize, 144] {
        let (raw, encoded) = encoded_valid(raw_len);
        let bound = aligned_touched_prefix_before_error(encoded.len());
        let bad = invalidate_one(&encoded, 0);
        let mut out = Aligned([0x5Au8; 160]);

        let got = dec.decode_to_slice(&bad, &mut out.0[..raw.len()]);
        assert!(got.is_none());
        assert!(out.0[..bound].iter().any(|&b| b != 0x5A));
        assert!(out.0[bound..raw.len()].iter().all(|&b| b == 0x5A));
    }
}
