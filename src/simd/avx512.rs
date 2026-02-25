use super::{Base64Decoder, scalar};

pub struct Avx512Decoder;

impl Base64Decoder for Avx512Decoder {
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>> {
        let out_len = scalar::decoded_len_strict(input)?;
        let mut out = vec![0u8; out_len];
        let written = scalar::decode_base64_fast(input, &mut out)?;
        debug_assert_eq!(written, out_len);
        Some(out)
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let out_len = ((input.len() + 2) / 3) * 4;
        let mut out = vec![0u8; out_len];
        let written = scalar::encode_base64_fast(input, &mut out);
        debug_assert_eq!(written, out_len);
        out
    }
}
