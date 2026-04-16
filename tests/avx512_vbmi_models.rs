use oxid64::engine::models::avx512vbmi::{
    DECODE_DOUBLE_THRESHOLD, DECODE_SINGLE_THRESHOLD, DECODE_TAIL_THRESHOLD,
    DOUBLE_ES256_BLOCK_STARTS, DOUBLE_ES256_INPUT_BYTES, DOUBLE_ES256_OUTPUT_BYTES,
    DOUBLE_ES256_PRELOAD_STARTS, DOUBLE_ES256_REQUIRED_INPUT, DS256_OUTPUT_BYTES,
    ES256_BLOCK_STARTS, ES256_INPUT_BYTES, ES256_OUTPUT_BYTES, SINGLE_ES256_REQUIRED_INPUT,
    can_run_double_es256, can_run_single_es256, non_strict_checks_offset,
    simd_touched_prefix_before_error, simd_written_prefix_before_error,
};

#[test]
fn avx512_encode_model_block_schedule_is_contiguous() {
    assert_eq!(ES256_BLOCK_STARTS, [0, 48, 96, 144]);
    assert_eq!(
        DOUBLE_ES256_BLOCK_STARTS,
        [0, 48, 96, 144, 192, 240, 288, 336]
    );
    assert_eq!(DOUBLE_ES256_PRELOAD_STARTS, [384, 432]);
}

#[test]
fn avx512_encode_model_required_input_matches_farthest_loads() {
    assert_eq!(SINGLE_ES256_REQUIRED_INPUT, 208);
    assert_eq!(DOUBLE_ES256_REQUIRED_INPUT, 496);
    assert_eq!(ES256_INPUT_BYTES, 192);
    assert_eq!(ES256_OUTPUT_BYTES, 256);
    assert_eq!(DOUBLE_ES256_INPUT_BYTES, 384);
    assert_eq!(DOUBLE_ES256_OUTPUT_BYTES, 512);

    assert!(!can_run_single_es256(207));
    assert!(can_run_single_es256(208));
    assert!(!can_run_double_es256(495));
    assert!(can_run_double_es256(496));
}

#[test]
fn avx512_decode_model_schedule_and_prefix_bounds() {
    assert_eq!(DECODE_SINGLE_THRESHOLD, 388);
    assert_eq!(DECODE_DOUBLE_THRESHOLD, 644);
    assert_eq!(DECODE_TAIL_THRESHOLD, 84);

    assert!(non_strict_checks_offset(392, 0));
    assert!(!non_strict_checks_offset(392, 80));
    assert!(non_strict_checks_offset(392, 160));
    assert!(non_strict_checks_offset(392, 320));

    assert!(non_strict_checks_offset(648, 0));
    assert!(!non_strict_checks_offset(648, 80));
    assert!(non_strict_checks_offset(648, 160));
    assert!(non_strict_checks_offset(648, 256));
    assert!(!non_strict_checks_offset(648, 336));
    assert!(non_strict_checks_offset(648, 576));

    assert_eq!(
        simd_written_prefix_before_error(392),
        DS256_OUTPUT_BYTES + 48
    );
    assert_eq!(
        simd_written_prefix_before_error(648),
        2 * DS256_OUTPUT_BYTES + 48
    );
    assert_eq!(
        simd_touched_prefix_before_error(392),
        DS256_OUTPUT_BYTES + 64
    );
    assert_eq!(
        simd_touched_prefix_before_error(648),
        2 * DS256_OUTPUT_BYTES + 64
    );
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod native {
    use super::{non_strict_checks_offset, simd_touched_prefix_before_error};
    use oxid64::engine::DecodeOpts;
    use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
    use oxid64::engine::scalar::encode_base64_fast;

    #[repr(align(64))]
    struct Aligned<const N: usize>([u8; N]);

    fn has_backend() -> bool {
        std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
            && std::arch::is_x86_feature_detected!("avx512vbmi")
    }

    fn encoded_valid(raw_len: usize) -> (Vec<u8>, Vec<u8>) {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(19)).collect();
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
    fn avx512_strict_rejects_invalid_every_position() {
        if !has_backend() {
            return;
        }

        let dec = Avx512VbmiDecoder::new();
        for raw_len in [294usize, 486] {
            let (_, encoded) = encoded_valid(raw_len);
            for pos in 0..encoded.len() {
                if encoded[pos] == b'=' {
                    continue;
                }
                let bad = invalidate_one(&encoded, pos, b'*');
                let mut out = Aligned([0u8; 800]);
                assert!(
                    dec.decode_to_slice(&bad, &mut out.0[..raw_len]).is_none(),
                    "strict avx512 accepted invalid byte at pos={pos} for encoded_len={}",
                    encoded.len()
                );
            }
        }
    }

    #[test]
    fn avx512_non_strict_matches_documented_schedule() {
        if !has_backend() {
            return;
        }

        let dec = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });
        for raw_len in [294usize, 486] {
            let (_, encoded) = encoded_valid(raw_len);
            for pos in 0..encoded.len() {
                if encoded[pos] == b'=' {
                    continue;
                }
                let bad = invalidate_one(&encoded, pos, b'*');
                let mut out = Aligned([0u8; 800]);
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
    fn avx512_invalid_decode_has_bounded_partial_writes() {
        if !has_backend() {
            return;
        }

        for strict in [true, false] {
            let dec = Avx512VbmiDecoder::with_opts(DecodeOpts { strict });
            for raw_len in [294usize, 486] {
                let (_, encoded) = encoded_valid(raw_len);
                let bad = invalidate_one(&encoded, 0, b'*');
                let bound = simd_touched_prefix_before_error(encoded.len());
                let canary = if strict { 0xA5u8 } else { 0x5Au8 };
                let mut out = Aligned([canary; 800]);
                let got = dec.decode_to_slice(&bad, &mut out.0[..raw_len]);
                assert!(got.is_none());
                assert!(out.0[..bound].iter().any(|&b| b != canary));
                assert!(out.0[bound..raw_len].iter().all(|&b| b == canary));
            }
        }
    }
}
