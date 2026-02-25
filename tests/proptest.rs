// use base64::{Engine as _, engine::general_purpose::STANDARD as b64_std};
// use oxid64::engine::{Base64Engine, CpuBackend};
// use oxid64::ffi::{c_decode, c_encode};
// use proptest::prelude::*;

// proptest! {
//     #[test]
//     fn encode_matches_c_and_std_implementation(input in prop::collection::vec(any::<u8>(), 0..2048)) {
//         let engine = oxid64::simd::avx2::Avx2Engine;

//         // C encode
//         let mut c_output = vec![0u8; engine.encoded_len(input.len()) + 64];
//         let c_len = c_encode(&input, &mut c_output);

//         // Std base64 crate encode
//         let std_output = b64_std.encode(&input);

//         // Verify C implementation matches std crate oracle first!
//         assert_eq!(&c_output[..c_len], std_output.as_bytes(), "C library differs from std oracle!");

//         // We comment out the Rust Engine execution for now until we implement Scalar

//         let mut rust_output = vec![0u8; engine.encoded_len(input.len()) + 64];
//         let rust_len = engine.encode(&input, &mut rust_output);
//         assert_eq!(rust_len, c_len, "Length mismatch");
//         assert_eq!(&rust_output[..rust_len], &c_output[..c_len], "Content mismatch");

//     }

//     #[test]
//     fn decode_matches_c_and_std_implementation(input in prop::collection::vec(any::<u8>(), 0..2048)) {
//         let engine = oxid64::simd::avx2::Avx2Engine;

//         let mut encoded = vec![0u8; engine.encoded_len(input.len()) + 64];
//         let encoded_len = c_encode(&input, &mut encoded);
//         let valid_b64 = &encoded[..encoded_len];

//         // Std base64 decode
//         let std_decoded = b64_std.decode(valid_b64).unwrap();

//         // C decode
//         let mut c_output = vec![0u8; engine.decoded_len(valid_b64.len()) + 64];
//         let c_len = c_decode(valid_b64, &mut c_output);

//         // Verify C implementation matches std crate oracle first!
//         assert_eq!(&c_output[..c_len], std_decoded.as_slice(), "C library differs from std oracle!");
//         assert_eq!(&c_output[..c_len], input.as_slice(), "C library decoded wrongly!");

//         // We comment out the Rust Engine execution for now until we implement Scalar

//         let mut rust_output = vec![0u8; engine.decoded_len(valid_b64.len()) + 64];
//         let rust_len = engine.decode(valid_b64, &mut rust_output).unwrap();
//         assert_eq!(rust_len, c_len, "Length mismatch");
//         assert_eq!(&rust_output[..rust_len], &c_output[..c_len], "Content mismatch");

//     }
// }
