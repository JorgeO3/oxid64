pub const DS64_INPUT_BYTES: usize = 64;
pub const DS64_OUTPUT_BYTES: usize = 48;
pub const TAIL16_INPUT_BYTES: usize = 16;
pub const TAIL16_OUTPUT_BYTES: usize = 12;
pub const SIMD_PRELOAD_BYTES: usize = 32;
pub const STORE_WIDTH_BYTES: usize = 16;
pub const DS64_STORE_OFFSETS: [usize; 4] = [0, 12, 24, 36];
pub const DS64_DOUBLE_STORE_OFFSETS: [usize; 8] = [0, 12, 24, 36, 48, 60, 72, 84];

#[inline(always)]
pub const fn non_strict_checks_lane(lane: usize) -> bool {
    lane == 0
}

#[inline(always)]
pub const fn lane_in_ds64(byte_offset: usize) -> usize {
    (byte_offset % DS64_INPUT_BYTES) / TAIL16_INPUT_BYTES
}

#[inline]
pub fn aligned_non_strict_checks_offset(input_len: usize, offset: usize) -> bool {
    let safe_end = input_len.saturating_sub(4);
    let mut in_ptr = 0usize;

    if in_ptr + SIMD_PRELOAD_BYTES > safe_end {
        return true;
    }

    while in_ptr + SIMD_PRELOAD_BYTES + 2 * DS64_INPUT_BYTES <= safe_end {
        if offset >= in_ptr && offset < in_ptr + DS64_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds64(offset - in_ptr));
        }
        if offset >= in_ptr + DS64_INPUT_BYTES && offset < in_ptr + 2 * DS64_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds64(offset - (in_ptr + DS64_INPUT_BYTES)));
        }
        in_ptr += 2 * DS64_INPUT_BYTES;
    }

    while in_ptr + SIMD_PRELOAD_BYTES + DS64_INPUT_BYTES <= safe_end {
        if offset >= in_ptr && offset < in_ptr + DS64_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds64(offset - in_ptr));
        }
        in_ptr += DS64_INPUT_BYTES;
    }

    while in_ptr + TAIL16_INPUT_BYTES <= safe_end {
        if offset >= in_ptr && offset < in_ptr + TAIL16_INPUT_BYTES {
            return true;
        }
        in_ptr += TAIL16_INPUT_BYTES;
    }

    true
}

#[inline]
pub fn aligned_written_prefix_before_error(input_len: usize) -> usize {
    let safe_end = input_len.saturating_sub(4);
    let mut in_ptr = 0usize;
    let mut out_ptr = 0usize;

    if in_ptr + SIMD_PRELOAD_BYTES > safe_end {
        return 0;
    }

    while in_ptr + SIMD_PRELOAD_BYTES + 2 * DS64_INPUT_BYTES <= safe_end {
        in_ptr += 2 * DS64_INPUT_BYTES;
        out_ptr += 2 * DS64_OUTPUT_BYTES;
    }

    while in_ptr + SIMD_PRELOAD_BYTES + DS64_INPUT_BYTES <= safe_end {
        in_ptr += DS64_INPUT_BYTES;
        out_ptr += DS64_OUTPUT_BYTES;
    }

    while in_ptr + TAIL16_INPUT_BYTES <= safe_end {
        in_ptr += TAIL16_INPUT_BYTES;
        out_ptr += TAIL16_OUTPUT_BYTES;
    }

    out_ptr
}

#[inline]
pub fn aligned_touched_prefix_before_error(input_len: usize) -> usize {
    let written = aligned_written_prefix_before_error(input_len);
    if written == 0 {
        0
    } else {
        written + 4
    }
}
