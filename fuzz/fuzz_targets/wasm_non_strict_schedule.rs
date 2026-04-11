#![no_main]

use libfuzzer_sys::fuzz_target;

fn non_strict_checks_offset(input_len: usize, offset: usize) -> bool {
    let safe_end = input_len.saturating_sub(4);
    let mut ip = 0usize;

    if !(ip + 32 <= safe_end) {
        return true;
    }

    while ip + 32 + 128 <= safe_end {
        if offset >= ip && offset < ip + 64 {
            return (offset - ip) < 16;
        }
        if offset >= ip + 64 && offset < ip + 128 {
            return (offset - (ip + 64)) < 16;
        }
        ip += 128;
    }

    while ip + 32 + 64 <= safe_end {
        if offset >= ip && offset < ip + 64 {
            return (offset - ip) < 16;
        }
        ip += 64;
    }

    while ip + 16 <= safe_end {
        if offset >= ip && offset < ip + 16 {
            return true;
        }
        ip += 16;
    }

    true
}

fn expected_checked(encoded_len: usize, pos: usize) -> bool {
    match encoded_len {
        128 => {
            if pos < 64 {
                pos < 16
            } else {
                true
            }
        }
        192 => {
            if pos < 64 {
                pos < 16
            } else if pos < 128 {
                pos < 80
            } else {
                true
            }
        }
        _ => true,
    }
}

fuzz_target!(|data: &[u8]| {
    let encoded_len = if data.first().copied().unwrap_or(0) & 1 == 0 {
        128usize
    } else {
        192usize
    };
    let pos = data.get(1).copied().unwrap_or(0) as usize % encoded_len;
    assert_eq!(
        non_strict_checks_offset(encoded_len, pos),
        expected_checked(encoded_len, pos)
    );
});
