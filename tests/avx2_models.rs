#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

use oxid64::engine::models::avx2::{
    non_strict_checks_offset, simd_touched_prefix_before_error_partial,
    simd_touched_prefix_before_error_strict, simd_written_prefix_before_error_partial,
    simd_written_prefix_before_error_strict,
};

#[test]
fn avx2_model_schedule_and_prefix_bounds() {
    assert!(non_strict_checks_offset(200, 0));
    assert!(!non_strict_checks_offset(200, 40));
    assert!(!non_strict_checks_offset(200, 80));
    assert!(non_strict_checks_offset(200, 160));

    assert!(non_strict_checks_offset(328, 0));
    assert!(!non_strict_checks_offset(328, 40));
    assert!(!non_strict_checks_offset(328, 80));
    assert!(non_strict_checks_offset(328, 128));
    assert!(!non_strict_checks_offset(328, 168));
    assert!(!non_strict_checks_offset(328, 232));
    assert!(non_strict_checks_offset(328, 320));

    assert_eq!(simd_written_prefix_before_error_partial(200), 144);
    assert_eq!(simd_written_prefix_before_error_partial(328), 240);
    assert_eq!(simd_written_prefix_before_error_partial(456), 336);
    assert_eq!(simd_written_prefix_before_error_strict(200), 144);
    assert_eq!(simd_written_prefix_before_error_strict(328), 240);
    assert_eq!(simd_written_prefix_before_error_strict(456), 336);
    assert_eq!(simd_touched_prefix_before_error_partial(200), 148);
    assert_eq!(simd_touched_prefix_before_error_strict(456), 340);
}

mod native {
    use super::{
        non_strict_checks_offset, simd_touched_prefix_before_error_partial,
        simd_touched_prefix_before_error_strict,
    };
    use oxid64::engine::DecodeOpts;
    use oxid64::engine::avx2::Avx2Decoder;
    use oxid64::engine::scalar::encode_base64_fast;

    #[repr(align(32))]
    struct Aligned<const N: usize>([u8; N]);

    fn encoded_valid(raw_len: usize) -> (Vec<u8>, Vec<u8>) {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(17)).collect();
        let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
        let written = encode_base64_fast(&raw, &mut encoded);
        encoded.truncate(written);
        (raw, encoded)
    }

    fn invalidate_one(encoded: &[u8], pos: usize, byte: u8) -> Vec<u8> {
        let mut out = encoded.to_vec();
        out[pos] = byte;
        out
    }

    #[test]
    fn avx2_strict_rejects_invalid_every_position_in_ds128_regimes() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }

        let dec = Avx2Decoder::new();
        for raw_len in [150usize, 246, 342] {
            let (_, encoded) = encoded_valid(raw_len);
            for pos in 0..encoded.len() {
                if encoded[pos] == b'=' {
                    continue;
                }
                let bad = invalidate_one(&encoded, pos, b'*');
                let mut out = Aligned([0u8; 400]);
                assert!(
                    dec.decode_to_slice(&bad, &mut out.0[..raw_len]).is_none(),
                    "strict avx2 accepted invalid byte at pos={pos} for encoded_len={}",
                    encoded.len()
                );
            }
        }
    }

    #[test]
    fn avx2_strict_rejects_internal_padding_across_boundaries() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }

        let dec = Avx2Decoder::new();
        for raw_len in [150usize, 246, 342] {
            let (_, encoded) = encoded_valid(raw_len);
            let mut cases = Vec::new();
            for &pos in &[
                0usize,
                31,
                32,
                63,
                64,
                95,
                96,
                encoded.len().saturating_sub(21),
            ] {
                if pos < encoded.len().saturating_sub(4) {
                    cases.push(pos);
                }
            }
            cases.sort_unstable();
            cases.dedup();
            for pos in cases {
                let bad = invalidate_one(&encoded, pos, b'=');
                let mut out = Aligned([0u8; 400]);
                assert!(
                    dec.decode_to_slice(&bad, &mut out.0[..raw_len]).is_none(),
                    "strict avx2 accepted internal '=' at pos={pos} for encoded_len={}",
                    encoded.len()
                );
            }
        }
    }

    #[test]
    fn avx2_non_strict_matches_documented_schedule() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }

        let dec = Avx2Decoder::with_opts(DecodeOpts { strict: false });
        for raw_len in [150usize, 246, 342] {
            let (_, encoded) = encoded_valid(raw_len);
            for pos in 0..encoded.len() {
                if encoded[pos] == b'=' {
                    continue;
                }
                let bad = invalidate_one(&encoded, pos, b'*');
                let mut out = Aligned([0u8; 400]);
                let got = dec.decode_to_slice(&bad, &mut out.0[..raw_len]);
                assert_eq!(
                    got.is_none(),
                    non_strict_checks_offset(encoded.len(), pos),
                    "non-strict schedule mismatch at pos={pos} for encoded_len={}",
                    encoded.len()
                );
            }
        }
    }

    #[test]
    fn avx2_strict_invalid_decode_has_bounded_partial_writes() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }

        let dec = Avx2Decoder::new();
        for raw_len in [150usize, 246, 342] {
            let (_, encoded) = encoded_valid(raw_len);
            let bad = invalidate_one(&encoded, 0, b'*');
            let bound = simd_touched_prefix_before_error_strict(encoded.len());
            let mut out = Aligned([0xA5u8; 400]);

            let got = dec.decode_to_slice(&bad, &mut out.0[..raw_len]);
            assert!(got.is_none());
            assert!(out.0[..bound].iter().any(|&b| b != 0xA5));
            assert!(out.0[bound..raw_len].iter().all(|&b| b == 0xA5));
        }
    }

    #[test]
    fn avx2_non_strict_invalid_decode_has_bounded_partial_writes() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }

        let dec = Avx2Decoder::with_opts(DecodeOpts { strict: false });
        for raw_len in [150usize, 246, 342] {
            let (_, encoded) = encoded_valid(raw_len);
            let bad = invalidate_one(&encoded, 0, b'*');
            let bound = simd_touched_prefix_before_error_partial(encoded.len());
            let mut out = Aligned([0x5Au8; 400]);

            let got = dec.decode_to_slice(&bad, &mut out.0[..raw_len]);
            assert!(got.is_none());
            assert!(out.0[..bound].iter().any(|&b| b != 0x5A));
            assert!(out.0[bound..raw_len].iter().all(|&b| b == 0x5A));
        }
    }
}
