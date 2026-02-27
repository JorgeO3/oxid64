use oxid64::simd::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::simd::ssse3::Ssse3Decoder;
use oxid64::simd::ssse3_cstyle::{
    Ssse3CStyleStrictArithDecoder, Ssse3CStyleStrictDecoder, Ssse3CStyleStrictRangeDecoder,
};
use oxid64::simd::ssse3_cstyle_experiments::{
    Ssse3CStyleStrictSse41ArithCheckDecoder, Ssse3CStyleStrictSse41PtestMaskDecoder,
    Ssse3CStyleStrictSse41PtestNoMaskDecoder,
};
use oxid64::simd::ssse3_cstyle_experiments_hybrid::{
    Ssse3CStyleStrictSse41HybridBucketDecoder, Ssse3CStyleStrictSse41ResynthA3Decoder,
    Ssse3CStyleStrictSse41ResynthA3SingleDecoder, Ssse3CStyleStrictSse41ResynthAdd4Decoder,
    Ssse3CStyleStrictSse41ResynthSharedBit6Decoder,
    Ssse3CStyleStrictSse42PcmpestrmDecoder,
};
use proptest::prelude::*;

fn decode_scalar_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len];
    decode_base64_fast(input, &mut out)?;
    Some(out)
}

fn decode_sse_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3Decoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_range_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictRangeDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_arith_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictArithDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_mask_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41PtestMaskDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_nomask_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41PtestNoMaskDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_arith_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41ArithCheckDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_hybrid_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41HybridBucketDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_resynth_a3_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41ResynthA3Decoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_resynth_a3_single_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41ResynthA3SingleDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_resynth_add4_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41ResynthAdd4Decoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse41_resynth_shared_bit6_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse41ResynthSharedBit6Decoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

fn decode_sse_cstyle_strict_sse42_pcmpestrm_reference(input: &[u8]) -> Option<Vec<u8>> {
    let n = input.len();
    if n == 0 {
        return Some(vec![]);
    }
    if (n & 3) != 0 {
        return None;
    }
    let pad = if input[n - 1] == b'=' {
        if input[n - 2] == b'=' { 2 } else { 1 }
    } else {
        0
    };
    let out_len = (n / 4) * 3 - pad;
    let mut out = vec![0u8; out_len + 64];
    let written = Ssse3CStyleStrictSse42PcmpestrmDecoder::decode_to_slice(input, &mut out)?;
    out.truncate(written);
    Some(out)
}

proptest! {
    #[test]
    fn test_sse_decode_matches_scalar(ref input in any::<Vec<u8>>()) {
        let mut encoded = vec![0u8; ((input.len() + 2) / 3) * 4];
        let _enc_len = encode_base64_fast(input, &mut encoded);

        if is_x86_feature_detected!("ssse3") {
            let expected = decode_scalar_reference(&encoded);
            let actual = decode_sse_reference(&encoded);
            prop_assert_eq!(expected.clone(), actual.clone());
            let actual_cstyle_strict = decode_sse_cstyle_strict_reference(&encoded);
            prop_assert_eq!(expected.clone(), actual_cstyle_strict);
            let actual_cstyle_strict_range = decode_sse_cstyle_strict_range_reference(&encoded);
            prop_assert_eq!(expected.clone(), actual_cstyle_strict_range.clone());
            let actual_cstyle_strict_arith = decode_sse_cstyle_strict_arith_reference(&encoded);
            prop_assert_eq!(expected.clone(), actual_cstyle_strict_arith);
            if is_x86_feature_detected!("sse4.1") {
                let actual_sse41_resynth =
                    decode_sse_cstyle_strict_sse41_resynth_a3_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_resynth);
                let actual_sse41_resynth_single =
                    decode_sse_cstyle_strict_sse41_resynth_a3_single_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_resynth_single);
                let actual_sse41_resynth_add4 =
                    decode_sse_cstyle_strict_sse41_resynth_add4_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_resynth_add4);
                let actual_sse41_resynth_shared_bit6 =
                    decode_sse_cstyle_strict_sse41_resynth_shared_bit6_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_resynth_shared_bit6);
                let actual_sse41_hybrid = decode_sse_cstyle_strict_sse41_hybrid_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_hybrid);
                let actual_sse41_arith = decode_sse_cstyle_strict_sse41_arith_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_arith);
                let actual_sse41_mask = decode_sse_cstyle_strict_sse41_mask_reference(&encoded);
                prop_assert_eq!(expected.clone(), actual_sse41_mask);
                let actual_sse41_nomask = decode_sse_cstyle_strict_sse41_nomask_reference(&encoded);
                prop_assert_eq!(expected, actual_sse41_nomask);
            }
            if is_x86_feature_detected!("sse4.2") {
                let actual_sse42_pcmpestrm =
                    decode_sse_cstyle_strict_sse42_pcmpestrm_reference(&encoded);
                prop_assert_eq!(decode_scalar_reference(&encoded), actual_sse42_pcmpestrm);
            }
        }
    }
}

#[test]
fn test_sse_decode_specific_lengths() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }
    for len in 0..1024 {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let mut encoded = vec![0u8; ((len + 2) / 3) * 4];
        let _enc_len = encode_base64_fast(&input, &mut encoded);

        let expected = decode_scalar_reference(&encoded);
        let actual = decode_sse_reference(&encoded);
        assert_eq!(expected.clone(), actual, "Failed at length {}", len);
        let actual_cstyle_strict = decode_sse_cstyle_strict_reference(&encoded);
        assert_eq!(
            expected.clone(),
            actual_cstyle_strict,
            "C-style strict failed at length {}",
            len
        );
        let actual_cstyle_strict_range = decode_sse_cstyle_strict_range_reference(&encoded);
        assert_eq!(
            expected, actual_cstyle_strict_range,
            "C-style strict range failed at length {}",
            len
        );
        if is_x86_feature_detected!("sse4.1") {
            let actual_sse41_resynth =
                decode_sse_cstyle_strict_sse41_resynth_a3_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_resynth,
                "C-style strict sse4.1 resynth A3 failed at length {}",
                len
            );
            let actual_sse41_resynth_single =
                decode_sse_cstyle_strict_sse41_resynth_a3_single_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_resynth_single,
                "C-style strict sse4.1 resynth A3 single failed at length {}",
                len
            );
            let actual_sse41_resynth_add4 =
                decode_sse_cstyle_strict_sse41_resynth_add4_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_resynth_add4,
                "C-style strict sse4.1 resynth add4 failed at length {}",
                len
            );
            let actual_sse41_resynth_shared_bit6 =
                decode_sse_cstyle_strict_sse41_resynth_shared_bit6_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_resynth_shared_bit6,
                "C-style strict sse4.1 resynth shared bit6 failed at length {}",
                len
            );
            let actual_sse41_hybrid = decode_sse_cstyle_strict_sse41_hybrid_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_hybrid,
                "C-style strict sse4.1 hybrid buckets failed at length {}",
                len
            );
            let actual_sse41_arith = decode_sse_cstyle_strict_sse41_arith_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_arith,
                "C-style strict sse4.1 arith check failed at length {}",
                len
            );
            let actual_sse41_mask = decode_sse_cstyle_strict_sse41_mask_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_mask,
                "C-style strict sse4.1 ptest mask failed at length {}",
                len
            );
            let actual_sse41_nomask = decode_sse_cstyle_strict_sse41_nomask_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse41_nomask,
                "C-style strict sse4.1 ptest nomask failed at length {}",
                len
            );
        }
        if is_x86_feature_detected!("sse4.2") {
            let actual_sse42_pcmpestrm =
                decode_sse_cstyle_strict_sse42_pcmpestrm_reference(&encoded);
            assert_eq!(
                decode_scalar_reference(&encoded),
                actual_sse42_pcmpestrm,
                "C-style strict sse4.2 pcmpestrm failed at length {}",
                len
            );
        }
    }
}

#[test]
fn test_sse_decode_invalid_chars() {
    if !is_x86_feature_detected!("ssse3") {
        return;
    }
    let invalid_inputs = [
        "AAA*".as_bytes(),
        "AAAA*AAA".as_bytes(),
        "AAAAAAAAAAAA*AAA".as_bytes(),
        "AAAAAAAAAAAAAAAAAAAA*AAA".as_bytes(),
    ];

    for input in invalid_inputs {
        let actual = decode_sse_reference(input);
        assert!(
            actual.is_none(),
            "Should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_cstyle_strict = decode_sse_cstyle_strict_reference(input);
        assert!(
            actual_cstyle_strict.is_none(),
            "C-style strict should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_cstyle_strict_range = decode_sse_cstyle_strict_range_reference(input);
        assert!(
            actual_cstyle_strict_range.is_none(),
            "C-style strict range should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        let actual_cstyle_strict_arith = decode_sse_cstyle_strict_arith_reference(input);
        assert!(
            actual_cstyle_strict_arith.is_none(),
            "C-style strict arith should have failed for input {:?}",
            std::str::from_utf8(input)
        );
        if is_x86_feature_detected!("sse4.1") {
            let actual_sse41_resynth = decode_sse_cstyle_strict_sse41_resynth_a3_reference(input);
            assert!(
                actual_sse41_resynth.is_none(),
                "C-style strict sse4.1 resynth A3 should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_resynth_single =
                decode_sse_cstyle_strict_sse41_resynth_a3_single_reference(input);
            assert!(
                actual_sse41_resynth_single.is_none(),
                "C-style strict sse4.1 resynth A3 single should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_resynth_add4 =
                decode_sse_cstyle_strict_sse41_resynth_add4_reference(input);
            assert!(
                actual_sse41_resynth_add4.is_none(),
                "C-style strict sse4.1 resynth add4 should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_resynth_shared_bit6 =
                decode_sse_cstyle_strict_sse41_resynth_shared_bit6_reference(input);
            assert!(
                actual_sse41_resynth_shared_bit6.is_none(),
                "C-style strict sse4.1 resynth shared bit6 should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_hybrid = decode_sse_cstyle_strict_sse41_hybrid_reference(input);
            assert!(
                actual_sse41_hybrid.is_none(),
                "C-style strict sse4.1 hybrid buckets should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_arith = decode_sse_cstyle_strict_sse41_arith_reference(input);
            assert!(
                actual_sse41_arith.is_none(),
                "C-style strict sse4.1 arith check should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_mask = decode_sse_cstyle_strict_sse41_mask_reference(input);
            assert!(
                actual_sse41_mask.is_none(),
                "C-style strict sse4.1 ptest mask should have failed for input {:?}",
                std::str::from_utf8(input)
            );
            let actual_sse41_nomask = decode_sse_cstyle_strict_sse41_nomask_reference(input);
            assert!(
                actual_sse41_nomask.is_none(),
                "C-style strict sse4.1 ptest nomask should have failed for input {:?}",
                std::str::from_utf8(input)
            );
        }
        if is_x86_feature_detected!("sse4.2") {
            let actual_sse42_pcmpestrm = decode_sse_cstyle_strict_sse42_pcmpestrm_reference(input);
            assert!(
                actual_sse42_pcmpestrm.is_none(),
                "C-style strict sse4.2 pcmpestrm should have failed for input {:?}",
                std::str::from_utf8(input)
            );
        }
    }
}
