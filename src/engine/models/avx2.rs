pub const DS128_INPUT_BYTES: usize = 128;
pub const DS128_OUTPUT_BYTES: usize = 96;
pub const TAIL16_INPUT_BYTES: usize = 16;
pub const TAIL16_OUTPUT_BYTES: usize = 12;
pub const SIMD_PRELOAD_BYTES: usize = 64;
pub const STORE_WIDTH_BYTES: usize = 16;
pub const DS128_STORE_OFFSETS: [usize; 8] = [0, 12, 24, 36, 48, 60, 72, 84];
/// In non-strict (partial-check) mode, only lane 0 of each DS128 is checked
/// (CHECK0 on iu0). This matches the C default mode where CHECK1 is a noop.
pub const CHECKED_LANE: usize = 0;
pub const PARTIAL_SINGLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + DS128_INPUT_BYTES + 4;
pub const PARTIAL_DOUBLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + 2 * DS128_INPUT_BYTES + 4;
pub const STRICT_SINGLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + DS128_INPUT_BYTES + 4;
pub const STRICT_DOUBLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + 2 * DS128_INPUT_BYTES + 4;
pub const STRICT_TRIPLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + 3 * DS128_INPUT_BYTES + 4;
pub const TAIL_THRESHOLD: usize = TAIL16_INPUT_BYTES + 4;

#[inline(always)]
pub const fn lane_in_ds128(byte_offset: usize) -> usize {
    (byte_offset % DS128_INPUT_BYTES) / 32
}

#[inline(always)]
pub const fn non_strict_checks_lane(lane: usize) -> bool {
    lane == CHECKED_LANE
}

#[inline]
pub fn non_strict_checks_offset(input_len: usize, offset: usize) -> bool {
    let mut ip = 0usize;

    while input_len.saturating_sub(ip) > PARTIAL_DOUBLE_THRESHOLD {
        if offset >= ip && offset < ip + DS128_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds128(offset - ip));
        }
        if offset >= ip + DS128_INPUT_BYTES && offset < ip + 2 * DS128_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds128(offset - (ip + DS128_INPUT_BYTES)));
        }
        ip += 2 * DS128_INPUT_BYTES;
    }

    if input_len.saturating_sub(ip) > PARTIAL_SINGLE_THRESHOLD {
        if offset >= ip && offset < ip + DS128_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds128(offset - ip));
        }
        ip += DS128_INPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > TAIL_THRESHOLD {
        if offset >= ip && offset < ip + TAIL16_INPUT_BYTES {
            return true;
        }
        ip += TAIL16_INPUT_BYTES;
    }

    true
}

#[inline]
pub fn simd_written_prefix_before_error_partial(input_len: usize) -> usize {
    let mut ip = 0usize;
    let mut op = 0usize;

    while input_len.saturating_sub(ip) > PARTIAL_DOUBLE_THRESHOLD {
        ip += 2 * DS128_INPUT_BYTES;
        op += 2 * DS128_OUTPUT_BYTES;
    }

    if input_len.saturating_sub(ip) > PARTIAL_SINGLE_THRESHOLD {
        ip += DS128_INPUT_BYTES;
        op += DS128_OUTPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > TAIL_THRESHOLD {
        ip += TAIL16_INPUT_BYTES;
        op += TAIL16_OUTPUT_BYTES;
    }

    op
}

#[inline]
pub fn simd_written_prefix_before_error_strict(input_len: usize) -> usize {
    let mut ip = 0usize;
    let mut op = 0usize;

    while input_len.saturating_sub(ip) > STRICT_TRIPLE_THRESHOLD {
        ip += 3 * DS128_INPUT_BYTES;
        op += 3 * DS128_OUTPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > STRICT_DOUBLE_THRESHOLD {
        ip += 2 * DS128_INPUT_BYTES;
        op += 2 * DS128_OUTPUT_BYTES;
    }

    if input_len.saturating_sub(ip) > STRICT_SINGLE_THRESHOLD {
        ip += DS128_INPUT_BYTES;
        op += DS128_OUTPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > TAIL_THRESHOLD {
        ip += TAIL16_INPUT_BYTES;
        op += TAIL16_OUTPUT_BYTES;
    }

    op
}

#[inline]
pub fn simd_touched_prefix_before_error_partial(input_len: usize) -> usize {
    let written = simd_written_prefix_before_error_partial(input_len);
    if written == 0 {
        0
    } else {
        written + 4
    }
}

#[inline]
pub fn simd_touched_prefix_before_error_strict(input_len: usize) -> usize {
    let written = simd_written_prefix_before_error_strict(input_len);
    if written == 0 {
        0
    } else {
        written + 4
    }
}
