use super::Base64Decoder;
use super::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};

pub struct Sse42Decoder;

impl Base64Decoder for Sse42Decoder {
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>> {
        let out_len = decoded_len_strict(input)?;
        let mut out = vec![0u8; out_len];
        let written = decode_base64_fast(input, &mut out)?;
        debug_assert_eq!(written, out_len);
        Some(out)
    }

    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let out_len = ((input.len() + 2) / 3) * 4;
        let mut out = vec![0u8; out_len];
        let written = encode_base64_fast(input, &mut out);
        debug_assert_eq!(written, out_len);
        out
    }
}
