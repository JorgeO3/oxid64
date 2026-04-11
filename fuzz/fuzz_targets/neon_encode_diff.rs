#![no_main]

use libfuzzer_sys::fuzz_target;

#[cfg(target_arch = "aarch64")]
use oxid64::engine::neon::NeonDecoder;
#[cfg(target_arch = "aarch64")]
use oxid64::engine::scalar::encode_base64_fast;

fuzz_target!(|data: &[u8]| {
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = data;
        return;
    }

    #[cfg(target_arch = "aarch64")]
    {
        if !std::arch::is_aarch64_feature_detected!("neon") {
            return;
        }

        let input = &data[..data.len().min(4096)];
        let mut expected = vec![0u8; input.len().div_ceil(3) * 4];
        let expected_len = encode_base64_fast(input, &mut expected);
        expected.truncate(expected_len);

        for offset in 1usize..4 {
            let canary = 0xC3u8;
            let mut backing = vec![canary; expected.len() + offset + 17];
            let written = NeonDecoder::new()
                .encode_to_slice(input, &mut backing[offset..offset + expected.len()]);
            assert_eq!(written, expected.len());
            assert_eq!(
                &backing[offset..offset + expected.len()],
                expected.as_slice()
            );
            assert!(backing[..offset].iter().all(|&b| b == canary));
            assert!(backing[offset + expected.len()..]
                .iter()
                .all(|&b| b == canary));
        }
    }
});
