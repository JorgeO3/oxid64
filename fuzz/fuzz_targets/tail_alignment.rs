#![no_main]

mod common;

use common::{
    assert_encode_matches_scalar, assert_strict_decode_matches_scalar, clamp_input, scalar_encode,
    subslice_with_prefix,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = clamp_input(data);

    let tail_len = if input.is_empty() {
        0
    } else {
        input[0] as usize % 97
    };
    let tail = &input[..input.len().min(tail_len)];

    assert_encode_matches_scalar(tail);

    let encoded = scalar_encode(tail);
    let prefix = if input.len() > 1 {
        (input[1] as usize % 4) + 1
    } else {
        1
    };
    let backing = subslice_with_prefix(&encoded, prefix, b'!');
    let encoded_subslice = &backing[prefix..prefix + encoded.len()];

    assert_strict_decode_matches_scalar(encoded_subslice);
});
