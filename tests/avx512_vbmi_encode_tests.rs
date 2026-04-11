use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::encode_base64_fast;
use proptest::prelude::*;

fn has_avx512vbmi_backend() -> bool {
    is_x86_feature_detected!("avx512f")
        && is_x86_feature_detected!("avx512bw")
        && is_x86_feature_detected!("avx512vbmi")
}

fn encode_scalar_reference(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = encode_base64_fast(input, &mut out);
    out.truncate(n);
    out
}

fn encode_avx512(input: &[u8]) -> Vec<u8> {
    let out_len = input.len().div_ceil(3) * 4;
    let mut out = vec![0u8; out_len + 64];
    let n = Avx512VbmiDecoder::new().encode_to_slice(input, &mut out);
    out.truncate(n);
    out
}

fn encode_with_canary<F>(out_len: usize, mut f: F) -> Vec<u8>
where
    F: FnMut(&mut [u8]) -> usize,
{
    let pre = 9usize;
    let post = 21usize;
    let canary = 0x5Au8;
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

proptest! {
    #[test]
    fn test_avx512_encode_matches_scalar(ref input in any::<Vec<u8>>()) {
        if has_avx512vbmi_backend() {
            let expected = encode_scalar_reference(input);
            let actual = encode_avx512(input);
            prop_assert_eq!(expected, actual);
        }
    }
}

#[test]
fn test_avx512_encode_specific_lengths() {
    if !has_avx512vbmi_backend() {
        return;
    }
    let max_len = if cfg!(miri) { 64usize } else { 2048usize };
    for len in 0..max_len {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_avx512(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_avx512_encode_large_lengths() {
    if !has_avx512vbmi_backend() {
        return;
    }
    // Exercise the double-unrolled ES256 loop (large inputs)
    let sizes: &[usize] = if cfg!(miri) {
        &[512, 768, 1024]
    } else {
        &[512, 768, 1024, 2048, 4096, 8192, 65536, 1048576]
    };
    for &len in sizes {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_avx512(&input);
        assert_eq!(expected, actual, "Failed at length {}", len);
    }
}

#[test]
fn test_avx512_encode_roundtrip() {
    if !has_avx512vbmi_backend() {
        return;
    }
    use oxid64::engine::scalar::decode_base64_fast;

    for len in [0, 1, 2, 3, 48, 96, 192, 384, 768, 1024] {
        let input: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let encoded = encode_avx512(&input);

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
fn test_avx512_encode_misaligned_and_canary_safe() {
    if !has_avx512vbmi_backend() {
        return;
    }

    let dec = Avx512VbmiDecoder::new();
    for len in [0usize, 1, 2, 3, 48, 49, 95, 96, 97, 255, 1024] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(13)).collect();
        let expected = encode_scalar_reference(&input);
        let out_len = expected.len();

        for offset in 1usize..4 {
            let canary = 0xC3u8;
            let mut backing = vec![canary; out_len + offset + 17];
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
fn test_avx512_encode_boundary_lengths() {
    if !has_avx512vbmi_backend() {
        return;
    }

    for len in [
        207usize, 208, 209, 383, 384, 385, 480, 481, 482, 495, 496, 497, 591, 592, 593, 768,
    ] {
        let input: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(17)).collect();
        let expected = encode_scalar_reference(&input);
        let actual = encode_avx512(&input);
        assert_eq!(expected, actual, "boundary failure at length {len}");
    }
}
