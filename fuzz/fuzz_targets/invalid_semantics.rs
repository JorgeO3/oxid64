#![no_main]

mod common;

use common::{assert_strict_decode_matches_scalar, clamp_input};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut mutated = clamp_input(data).to_vec();

    if !mutated.is_empty() {
        let asciiish = mutated[0] & 0x7f;
        mutated[0] = if asciiish == b'=' { b'*' } else { asciiish };
    }
    if mutated.len() >= 4 {
        let idx = mutated.len() / 2;
        mutated[idx] ^= 0x80;
    }

    assert_strict_decode_matches_scalar(&mutated);
});
