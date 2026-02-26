pub mod avx2;
pub mod avx512;
pub mod neon;
pub mod scalar;
pub mod sse42;

use avx2::Avx2Decoder;
use avx512::Avx512Decoder;
use enum_dispatch::enum_dispatch;
use neon::NeonDecoder;
use scalar::ScalarDecoder;
use sse42::Sse42Decoder;

#[enum_dispatch]
pub enum Decoder {
    ScalarDecoder,
    Sse42Decoder,
    NeonDecoder,
    Avx2Decoder,
    Avx512Decoder,
}

#[enum_dispatch(Decoder)]
pub trait Base64Decoder {
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>>;
    fn encode(&self, input: &[u8]) -> Vec<u8>;
}

pub fn select_best_decoder() -> Decoder {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx512f")
            && std::arch::is_x86_feature_detected!("avx512bw")
            && std::arch::is_x86_feature_detected!("avx512vl")
        {
            return Avx512Decoder.into();
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            return Avx2Decoder.into();
        }
        if std::arch::is_x86_feature_detected!("sse4.2") {
            return Sse42Decoder.into();
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return NeonDecoder.into();
        }
    }

    ScalarDecoder.into()
}
