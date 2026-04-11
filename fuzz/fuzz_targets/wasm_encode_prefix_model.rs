#![no_main]

use libfuzzer_sys::fuzz_target;

const ENCODE_SIMD_ENTRY_THRESHOLD: usize = 52;
const ENCODE_MAIN_INPUT_BYTES: usize = 96;
const ENCODE_MAIN_REQUIRED_INPUT: usize = 108;
const ENCODE_DRAIN_INPUT_BYTES: usize = 48;
const ENCODE_DRAIN_REQUIRED_INPUT: usize = 60;
const ENCODE_TAIL_INPUT_BYTES: usize = 12;
const ENCODE_TAIL_REQUIRED_INPUT: usize = 16;

fn can_run_encode_main(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_MAIN_REQUIRED_INPUT
}

fn can_run_encode_drain(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_DRAIN_REQUIRED_INPUT
}

fn can_run_encode_tail(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_TAIL_REQUIRED_INPUT
}

fn encode_prefix_input_len(input_len: usize) -> usize {
    if input_len < ENCODE_SIMD_ENTRY_THRESHOLD {
        return 0;
    }

    let mut remaining = input_len;
    let mut consumed = 0usize;

    while can_run_encode_main(remaining) {
        remaining -= ENCODE_MAIN_INPUT_BYTES;
        consumed += ENCODE_MAIN_INPUT_BYTES;
    }
    while can_run_encode_drain(remaining) {
        remaining -= ENCODE_DRAIN_INPUT_BYTES;
        consumed += ENCODE_DRAIN_INPUT_BYTES;
    }
    while can_run_encode_tail(remaining) {
        remaining -= ENCODE_TAIL_INPUT_BYTES;
        consumed += ENCODE_TAIL_INPUT_BYTES;
    }

    consumed
}

fn encode_prefix_output_len(input_len: usize) -> usize {
    (encode_prefix_input_len(input_len) / 3) * 4
}

fuzz_target!(|data: &[u8]| {
    let len = data
        .iter()
        .take(4)
        .fold(0usize, |acc, &b| (acc << 8) | b as usize)
        % 4096;
    let prefix_in = encode_prefix_input_len(len);
    let prefix_out = encode_prefix_output_len(len);

    assert!(prefix_in <= len);
    assert_eq!(prefix_in % 12, 0);
    assert_eq!(prefix_out, (prefix_in / 3) * 4);

    if can_run_encode_main(len) {
        assert!(len >= ENCODE_MAIN_REQUIRED_INPUT);
    }
    if can_run_encode_drain(len) {
        assert!(len >= ENCODE_DRAIN_REQUIRED_INPUT);
    }
    if can_run_encode_tail(len) {
        assert!(len >= ENCODE_TAIL_REQUIRED_INPUT);
    }
});
