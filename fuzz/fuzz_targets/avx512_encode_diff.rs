#![no_main]
#![cfg_attr(not(any(target_arch = "x86", target_arch = "x86_64")), allow(unused))]

use libfuzzer_sys::fuzz_target;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod native {
    use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
    use oxid64::engine::scalar::encode_base64_fast;

    fn has_backend() -> bool {
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512vbmi")
    }

    pub(super) fn run(data: &[u8]) {
        if !has_backend() {
            return;
        }

        let input = &data[..data.len().min(4096)];
        let mut expected = vec![0u8; input.len().div_ceil(3) * 4];
        let expected_len = encode_base64_fast(input, &mut expected);
        expected.truncate(expected_len);

        for offset in 1usize..4 {
            let canary = 0xC1u8;
            let mut backing = vec![canary; expected.len() + offset + 17];
            let written = Avx512VbmiDecoder::new()
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
}

fuzz_target!(|data: &[u8]| {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    native::run(data);
});
