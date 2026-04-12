pub const BLOCK_IN_BYTES: usize = 48;
pub const BLOCK_OUT_BYTES: usize = 64;
pub const ES256_INPUT_BYTES: usize = 4 * BLOCK_IN_BYTES;
pub const ES256_OUTPUT_BYTES: usize = 4 * BLOCK_OUT_BYTES;
pub const DOUBLE_ES256_INPUT_BYTES: usize = 2 * ES256_INPUT_BYTES;
pub const DOUBLE_ES256_OUTPUT_BYTES: usize = 2 * ES256_OUTPUT_BYTES;
pub const SINGLE_ES256_REQUIRED_INPUT: usize = 4 * BLOCK_IN_BYTES + 16;
pub const DOUBLE_ES256_REQUIRED_INPUT: usize = 10 * BLOCK_IN_BYTES + 16;
pub const ES256_BLOCK_STARTS: [usize; 4] = [0, 48, 96, 144];
pub const DOUBLE_ES256_BLOCK_STARTS: [usize; 8] = [0, 48, 96, 144, 192, 240, 288, 336];
pub const DOUBLE_ES256_PRELOAD_STARTS: [usize; 2] = [384, 432];
pub const DS256_INPUT_BYTES: usize = 256;
pub const DS256_OUTPUT_BYTES: usize = 192;
pub const DS256_STORE_OFFSETS: [usize; 4] = [0, 48, 96, 144];
pub const TAIL64_INPUT_BYTES: usize = 64;
pub const TAIL64_OUTPUT_BYTES: usize = 48;
pub const DECODE_STORE_WIDTH_BYTES: usize = 64;
pub const DECODE_SINGLE_THRESHOLD: usize = 128 + DS256_INPUT_BYTES + 4;
pub const DECODE_DOUBLE_THRESHOLD: usize = 128 + 2 * DS256_INPUT_BYTES + 4;
pub const DECODE_TAIL_THRESHOLD: usize = TAIL64_INPUT_BYTES + 16 + 4;

#[inline(always)]
pub const fn can_run_single_es256(remaining_input: usize) -> bool {
    remaining_input >= SINGLE_ES256_REQUIRED_INPUT
}

#[inline(always)]
pub const fn can_run_double_es256(remaining_input: usize) -> bool {
    remaining_input >= DOUBLE_ES256_REQUIRED_INPUT
}

#[inline(always)]
pub const fn lane_in_ds256(byte_offset: usize) -> usize {
    (byte_offset % DS256_INPUT_BYTES) / TAIL64_INPUT_BYTES
}

#[inline(always)]
pub const fn non_strict_checks_lane(lane: usize) -> bool {
    lane != 1
}

#[inline]
pub fn non_strict_checks_offset(input_len: usize, offset: usize) -> bool {
    let mut ip = 0usize;

    while input_len.saturating_sub(ip) > DECODE_DOUBLE_THRESHOLD {
        if offset >= ip && offset < ip + DS256_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds256(offset - ip));
        }
        if offset >= ip + DS256_INPUT_BYTES && offset < ip + 2 * DS256_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds256(offset - (ip + DS256_INPUT_BYTES)));
        }
        ip += 2 * DS256_INPUT_BYTES;
    }

    if input_len.saturating_sub(ip) > DECODE_SINGLE_THRESHOLD {
        if offset >= ip && offset < ip + DS256_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds256(offset - ip));
        }
        ip += DS256_INPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > DECODE_TAIL_THRESHOLD {
        if offset >= ip && offset < ip + TAIL64_INPUT_BYTES {
            return true;
        }
        ip += TAIL64_INPUT_BYTES;
    }

    true
}

#[inline]
pub fn simd_written_prefix_before_error(input_len: usize) -> usize {
    let mut ip = 0usize;
    let mut op = 0usize;

    while input_len.saturating_sub(ip) > DECODE_DOUBLE_THRESHOLD {
        ip += 2 * DS256_INPUT_BYTES;
        op += 2 * DS256_OUTPUT_BYTES;
    }

    if input_len.saturating_sub(ip) > DECODE_SINGLE_THRESHOLD {
        ip += DS256_INPUT_BYTES;
        op += DS256_OUTPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > DECODE_TAIL_THRESHOLD {
        ip += TAIL64_INPUT_BYTES;
        op += TAIL64_OUTPUT_BYTES;
    }

    op
}

#[inline]
pub fn simd_touched_prefix_before_error(input_len: usize) -> usize {
    let written = simd_written_prefix_before_error(input_len);
    if written == 0 { 0 } else { written + 16 }
}
