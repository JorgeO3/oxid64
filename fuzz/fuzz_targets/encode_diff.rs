#![no_main]

mod common;

use common::{assert_encode_matches_scalar, clamp_input};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = clamp_input(data);
    assert_encode_matches_scalar(input);
});
