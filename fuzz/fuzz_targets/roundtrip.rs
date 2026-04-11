#![no_main]

mod common;

use common::{assert_roundtrip_all, clamp_input};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = clamp_input(data);
    assert_roundtrip_all(input);
});
