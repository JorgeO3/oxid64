#![no_main]

use libfuzzer_sys::fuzz_target;

fn simd_written_prefix_before_error(input_len: usize) -> usize {
    let safe_end = input_len.saturating_sub(4);
    let mut ip = 0usize;
    let mut op = 0usize;

    if !(ip + 32 <= safe_end) {
        return 0;
    }

    while ip + 32 + 128 <= safe_end {
        ip += 128;
        op += 96;
    }

    while ip + 32 + 64 <= safe_end {
        ip += 64;
        op += 48;
    }

    while ip + 16 <= safe_end {
        ip += 16;
        op += 12;
    }

    op
}

fn simd_touched_prefix_before_error(input_len: usize) -> usize {
    let written = simd_written_prefix_before_error(input_len);
    if written == 0 {
        0
    } else {
        written + 4
    }
}

fuzz_target!(|data: &[u8]| {
    let len = data
        .iter()
        .take(4)
        .fold(0usize, |acc, &b| (acc << 8) | b as usize)
        % 4096;
    let written = simd_written_prefix_before_error(len);
    let touched = simd_touched_prefix_before_error(len);
    let decoded_upper = (len / 4) * 3;

    assert!(written <= decoded_upper);
    if written == 0 {
        assert_eq!(touched, 0);
    } else {
        assert_eq!(touched, written + 4);
    }
});
