#![cfg(target_arch = "aarch64")]

use oxid64::engine::models::neon::{
    DECODE_BLOCK_OUTPUT_BYTES, DECODE_GROUP_OUTPUT_BYTES, can_run_encode_block,
    can_run_encode_pair, encode_prefix_input_len, encode_prefix_output_len,
    non_strict_checks_offset, simd_touched_prefix_before_error, simd_written_prefix_before_error,
};

#[test]
fn neon_encode_model_required_input_matches_block_boundaries() {
    assert!(!can_run_encode_block(47));
    assert!(can_run_encode_block(48));
    assert!(!can_run_encode_pair(95));
    assert!(can_run_encode_pair(96));

    assert_eq!(encode_prefix_input_len(47), 0);
    assert_eq!(encode_prefix_input_len(48), 48);
    assert_eq!(encode_prefix_input_len(95), 48);
    assert_eq!(encode_prefix_input_len(96), 96);
    assert_eq!(encode_prefix_input_len(143), 96);
    assert_eq!(encode_prefix_input_len(144), 144);

    assert_eq!(encode_prefix_output_len(47), 0);
    assert_eq!(encode_prefix_output_len(48), 64);
    assert_eq!(encode_prefix_output_len(95), 64);
    assert_eq!(encode_prefix_output_len(96), 128);
}

#[test]
fn neon_decode_model_schedule_and_prefix_bounds() {
    assert!(non_strict_checks_offset(256, 0));
    assert!(non_strict_checks_offset(64, 0));

    assert!(!non_strict_checks_offset(320, 0));
    assert!(!non_strict_checks_offset(320, 80));
    assert!(!non_strict_checks_offset(320, 160));
    assert!(non_strict_checks_offset(320, 208));
    assert!(non_strict_checks_offset(320, 300));

    assert!(!non_strict_checks_offset(576, 0));
    assert!(non_strict_checks_offset(576, 200));
    assert!(!non_strict_checks_offset(576, 260));
    assert!(non_strict_checks_offset(576, 500));

    assert_eq!(simd_written_prefix_before_error(64), 0);
    assert_eq!(
        simd_written_prefix_before_error(320),
        DECODE_GROUP_OUTPUT_BYTES
    );
    assert_eq!(
        simd_written_prefix_before_error(576),
        DECODE_GROUP_OUTPUT_BYTES + 4 * DECODE_BLOCK_OUTPUT_BYTES
    );
    assert_eq!(
        simd_touched_prefix_before_error(320),
        DECODE_GROUP_OUTPUT_BYTES
    );
}

mod native {
    use super::{non_strict_checks_offset, simd_touched_prefix_before_error};
    use oxid64::Base64Decoder;
    use oxid64::engine::DecodeOpts;
    use oxid64::engine::neon::NeonDecoder;
    use oxid64::engine::scalar::encode_base64_fast;

    #[repr(align(16))]
    struct Aligned<const N: usize>([u8; N]);

    fn has_backend() -> bool {
        std::arch::is_aarch64_feature_detected!("neon")
    }

    fn encoded_valid(raw_len: usize) -> (Vec<u8>, Vec<u8>) {
        let raw: Vec<u8> = (0..raw_len).map(|i| (i as u8).wrapping_mul(13)).collect();
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
    fn neon_strict_rejects_invalid_every_position() {
        if !has_backend() {
            return;
        }

        let dec = NeonDecoder::new();
        for raw_len in [240usize, 432] {
            let (_, encoded) = encoded_valid(raw_len);
            for pos in 0..encoded.len() {
                if encoded[pos] == b'=' {
                    continue;
                }
                let bad = invalidate_one(&encoded, pos, b'*');
                let mut out = Aligned([0u8; 640]);
                assert!(
                    dec.decode_to_slice(&bad, &mut out.0[..raw_len]).is_none(),
                    "strict neon accepted invalid byte at pos={pos} for encoded_len={}",
                    encoded.len()
                );
            }
        }
    }

    #[test]
    fn neon_strict_rejects_high_bit_bytes() {
        if !has_backend() {
            return;
        }

        let dec = NeonDecoder::new();
        for raw_len in [240usize, 432] {
            let (_, encoded) = encoded_valid(raw_len);
            for &byte in &[0x80u8, 0xFFu8] {
                for pos in 0..encoded.len().saturating_sub(4) {
                    let bad = invalidate_one(&encoded, pos, byte);
                    let mut out = Aligned([0u8; 640]);
                    assert!(
                        dec.decode_to_slice(&bad, &mut out.0[..raw_len]).is_none(),
                        "strict neon accepted high-bit byte {byte:#04x} at pos={pos} for encoded_len={}",
                        encoded.len()
                    );
                }
            }
        }
    }

    #[test]
    fn neon_non_strict_matches_documented_schedule() {
        if !has_backend() {
            return;
        }

        let dec = NeonDecoder::with_opts(DecodeOpts { strict: false });
        for raw_len in [240usize, 432] {
            let (_, encoded) = encoded_valid(raw_len);
            for pos in 0..encoded.len() {
                if encoded[pos] == b'=' {
                    continue;
                }
                let bad = invalidate_one(&encoded, pos, b'*');
                let mut out = Aligned([0u8; 640]);
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
    fn neon_invalid_decode_has_bounded_partial_writes() {
        if !has_backend() {
            return;
        }

        for strict in [true, false] {
            let dec = NeonDecoder::with_opts(DecodeOpts { strict });
            for raw_len in [240usize, 432] {
                let (_, encoded) = encoded_valid(raw_len);
                let bad_pos = if strict { 0 } else { 192 };
                let bad = invalidate_one(&encoded, bad_pos, b'*');
                let bound = simd_touched_prefix_before_error(encoded.len());
                let canary = if strict { 0xA5u8 } else { 0x5Au8 };
                let mut out = Aligned([canary; 640]);
                let got = dec.decode_to_slice(&bad, &mut out.0[..raw_len]);
                assert!(got.is_none());
                assert!(out.0[..bound].iter().any(|&b| b != canary));
                assert!(out.0[bound..raw_len].iter().all(|&b| b == canary));
            }
        }
    }
}
