pub const DS64_INPUT_BYTES: usize = 64;
pub const DS64_OUTPUT_BYTES: usize = 48;
pub const TAIL16_INPUT_BYTES: usize = 16;
pub const TAIL16_OUTPUT_BYTES: usize = 12;
pub const SIMD_PRELOAD_BYTES: usize = 32;
pub const STORE_WIDTH_BYTES: usize = 16;
pub const STORE_OVERHANG_BYTES: usize = 4;
pub const DS64_STORE_OFFSETS: [usize; 4] = [0, 12, 24, 36];
pub const DS64_DOUBLE_STORE_OFFSETS: [usize; 8] = [0, 12, 24, 36, 48, 60, 72, 84];
pub const SINGLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + DS64_INPUT_BYTES;
pub const DOUBLE_THRESHOLD: usize = SIMD_PRELOAD_BYTES + 2 * DS64_INPUT_BYTES;
pub const TAIL_THRESHOLD: usize = TAIL16_INPUT_BYTES;
pub const ENCODE_SIMD_ENTRY_THRESHOLD: usize = 52;
pub const ENCODE_MAIN_INPUT_BYTES: usize = 96;
pub const ENCODE_MAIN_OUTPUT_BYTES: usize = 128;
pub const ENCODE_MAIN_REQUIRED_INPUT: usize = 108;
pub const ENCODE_DRAIN_INPUT_BYTES: usize = 48;
pub const ENCODE_DRAIN_OUTPUT_BYTES: usize = 64;
pub const ENCODE_DRAIN_REQUIRED_INPUT: usize = 60;
pub const ENCODE_TAIL_INPUT_BYTES: usize = 12;
pub const ENCODE_TAIL_OUTPUT_BYTES: usize = 16;
pub const ENCODE_TAIL_REQUIRED_INPUT: usize = 16;

#[inline(always)]
pub const fn pshufb_select_index(ctrl: u8) -> Option<usize> {
    if ctrl & 0x80 != 0 {
        None
    } else {
        Some((ctrl & 0x0f) as usize)
    }
}

#[inline(always)]
pub const fn wasm_swizzle_select_index(ctrl: u8) -> Option<usize> {
    if ctrl < 16 { Some(ctrl as usize) } else { None }
}

#[inline(always)]
pub const fn pshufb_lookup_byte(table: [u8; 16], ctrl: u8) -> u8 {
    match pshufb_select_index(ctrl) {
        Some(idx) => table[idx],
        None => 0,
    }
}

#[inline(always)]
pub const fn non_strict_checks_lane(lane: usize) -> bool {
    lane == 0
}

#[inline(always)]
pub const fn lane_in_ds64(byte_offset: usize) -> usize {
    (byte_offset % DS64_INPUT_BYTES) / TAIL16_INPUT_BYTES
}

#[inline]
pub fn non_strict_checks_offset(input_len: usize, offset: usize) -> bool {
    let safe_end = input_len.saturating_sub(STORE_OVERHANG_BYTES);
    let mut ip = 0usize;

    if ip + SIMD_PRELOAD_BYTES > safe_end {
        return true;
    }

    while ip + DOUBLE_THRESHOLD <= safe_end {
        if offset >= ip && offset < ip + DS64_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds64(offset - ip));
        }
        if offset >= ip + DS64_INPUT_BYTES && offset < ip + 2 * DS64_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds64(offset - (ip + DS64_INPUT_BYTES)));
        }
        ip += 2 * DS64_INPUT_BYTES;
    }

    while ip + SINGLE_THRESHOLD <= safe_end {
        if offset >= ip && offset < ip + DS64_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_ds64(offset - ip));
        }
        ip += DS64_INPUT_BYTES;
    }

    while ip + TAIL_THRESHOLD <= safe_end {
        if offset >= ip && offset < ip + TAIL16_INPUT_BYTES {
            return true;
        }
        ip += TAIL16_INPUT_BYTES;
    }

    true
}

#[inline]
pub fn simd_written_prefix_before_error(input_len: usize) -> usize {
    let safe_end = input_len.saturating_sub(STORE_OVERHANG_BYTES);
    let mut ip = 0usize;
    let mut op = 0usize;

    if ip + SIMD_PRELOAD_BYTES > safe_end {
        return 0;
    }

    while ip + DOUBLE_THRESHOLD <= safe_end {
        ip += 2 * DS64_INPUT_BYTES;
        op += 2 * DS64_OUTPUT_BYTES;
    }

    while ip + SINGLE_THRESHOLD <= safe_end {
        ip += DS64_INPUT_BYTES;
        op += DS64_OUTPUT_BYTES;
    }

    while ip + TAIL_THRESHOLD <= safe_end {
        ip += TAIL16_INPUT_BYTES;
        op += TAIL16_OUTPUT_BYTES;
    }

    op
}

#[inline]
pub fn simd_touched_prefix_before_error(input_len: usize) -> usize {
    let written = simd_written_prefix_before_error(input_len);
    if written == 0 {
        0
    } else {
        written + STORE_OVERHANG_BYTES
    }
}

#[inline(always)]
pub const fn can_run_encode_main(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_MAIN_REQUIRED_INPUT
}

#[inline(always)]
pub const fn can_run_encode_drain(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_DRAIN_REQUIRED_INPUT
}

#[inline(always)]
pub const fn can_run_encode_tail(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_TAIL_REQUIRED_INPUT
}

#[inline]
pub fn encode_prefix_input_len(input_len: usize) -> usize {
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

#[inline]
pub fn encode_prefix_output_len(input_len: usize) -> usize {
    if input_len < ENCODE_SIMD_ENTRY_THRESHOLD {
        return 0;
    }

    let mut remaining = input_len;
    let mut written = 0usize;

    while can_run_encode_main(remaining) {
        remaining -= ENCODE_MAIN_INPUT_BYTES;
        written += ENCODE_MAIN_OUTPUT_BYTES;
    }
    while can_run_encode_drain(remaining) {
        remaining -= ENCODE_DRAIN_INPUT_BYTES;
        written += ENCODE_DRAIN_OUTPUT_BYTES;
    }
    while can_run_encode_tail(remaining) {
        remaining -= ENCODE_TAIL_INPUT_BYTES;
        written += ENCODE_TAIL_OUTPUT_BYTES;
    }

    written
}
