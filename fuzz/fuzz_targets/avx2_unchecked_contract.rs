#![no_main]

use libfuzzer_sys::fuzz_target;
use oxid64::decoded_len;
use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::scalar::{decode_base64_fast, encode_base64_fast};

fuzz_target!(|data: &[u8]| {
    if !cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    let raw = &data[..data.len().min(4096)];
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4];
    let written = encode_base64_fast(raw, &mut encoded);
    encoded.truncate(written);

    let mut input = encoded.clone();
    if !input.is_empty() && (data.first().copied().unwrap_or(0) & 1) == 1 {
        let idx = (data.len().wrapping_mul(13)) % input.len();
        if input[idx] != b'=' {
            input[idx] = b'*';
        }
    }

    let upper = if input.is_empty() {
        0
    } else {
        decoded_len(&input).unwrap_or((input.len() / 4) * 3)
    };

    for offset in 1usize..4 {
        let canary = 0xC3u8;
        let mut backing = vec![canary; upper + offset + 17];
        let got = {
            let (prefix, rest) = backing.split_at_mut(offset);
            let (out, suffix) = rest.split_at_mut(upper);
            let got = Avx2Decoder::new().decode_to_slice_unchecked(&input, out);
            assert!(prefix.iter().all(|&b| b == canary));
            assert!(suffix.iter().all(|&b| b == canary));
            got
        };

        if input == encoded {
            let mut scalar_out = vec![0u8; raw.len() + 16];
            let expected_len =
                decode_base64_fast(&encoded, &mut scalar_out).expect("scalar decode");
            assert_eq!(got, Some(expected_len));
            assert_eq!(
                &backing[offset..offset + expected_len],
                &scalar_out[..expected_len]
            );
        }
    }
});
