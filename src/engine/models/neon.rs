pub const DECODE_BLOCK_INPUT_BYTES: usize = 64;
pub const DECODE_BLOCK_OUTPUT_BYTES: usize = 48;
pub const DECODE_GROUP_INPUT_BYTES: usize = 256;
pub const DECODE_GROUP_OUTPUT_BYTES: usize = 192;
pub const ENCODE_BLOCK_INPUT_BYTES: usize = 48;
pub const ENCODE_BLOCK_OUTPUT_BYTES: usize = 64;
pub const ENCODE_PAIR_INPUT_BYTES: usize = 96;
pub const ENCODE_PAIR_OUTPUT_BYTES: usize = 128;

#[inline(always)]
pub const fn non_strict_checks_lane(lane: usize) -> bool {
    lane == 3
}

#[inline(always)]
pub const fn lane_in_dn256(byte_offset: usize) -> usize {
    (byte_offset % DECODE_GROUP_INPUT_BYTES) / DECODE_BLOCK_INPUT_BYTES
}

#[inline]
pub fn non_strict_checks_offset(input_len: usize, offset: usize) -> bool {
    let mut ip = 0usize;

    while input_len.saturating_sub(ip) > DECODE_GROUP_INPUT_BYTES {
        if offset >= ip && offset < ip + DECODE_GROUP_INPUT_BYTES {
            return non_strict_checks_lane(lane_in_dn256(offset - ip));
        }
        ip += DECODE_GROUP_INPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > DECODE_BLOCK_INPUT_BYTES {
        if offset >= ip && offset < ip + DECODE_BLOCK_INPUT_BYTES {
            return true;
        }
        ip += DECODE_BLOCK_INPUT_BYTES;
    }

    true
}

#[inline]
pub fn simd_written_prefix_before_error(input_len: usize) -> usize {
    let mut ip = 0usize;
    let mut op = 0usize;

    while input_len.saturating_sub(ip) > DECODE_GROUP_INPUT_BYTES {
        ip += DECODE_GROUP_INPUT_BYTES;
        op += DECODE_GROUP_OUTPUT_BYTES;
    }

    while input_len.saturating_sub(ip) > DECODE_BLOCK_INPUT_BYTES {
        ip += DECODE_BLOCK_INPUT_BYTES;
        op += DECODE_BLOCK_OUTPUT_BYTES;
    }

    op
}

#[inline]
pub fn simd_touched_prefix_before_error(input_len: usize) -> usize {
    simd_written_prefix_before_error(input_len)
}

#[inline(always)]
pub const fn can_run_encode_pair(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_PAIR_INPUT_BYTES
}

#[inline(always)]
pub const fn can_run_encode_block(remaining_input: usize) -> bool {
    remaining_input >= ENCODE_BLOCK_INPUT_BYTES
}

#[inline]
pub fn encode_prefix_input_len(input_len: usize) -> usize {
    let pair_prefix = (input_len / ENCODE_PAIR_INPUT_BYTES) * ENCODE_PAIR_INPUT_BYTES;
    let tail = input_len - pair_prefix;
    pair_prefix + (tail / ENCODE_BLOCK_INPUT_BYTES) * ENCODE_BLOCK_INPUT_BYTES
}

#[inline]
pub fn encode_prefix_output_len(input_len: usize) -> usize {
    (encode_prefix_input_len(input_len) / 3) * 4
}
