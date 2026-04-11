//! Shared low-level helpers for pointer/bounds contracts in SIMD engines.

use super::scalar::decoded_len_strict;

/// Remaining bytes between `ptr` and `end` as an unsigned distance.
///
/// This uses integer arithmetic so callers can perform guards without
/// constructing out-of-bounds pointers via `ptr.add()`/`ptr.sub()`.
#[inline(always)]
#[doc(hidden)]
pub fn remaining(ptr: *const u8, end: *const u8) -> usize {
    (end as usize).saturating_sub(ptr as usize)
}

/// Mutable variant of [`remaining`].
#[inline(always)]
#[doc(hidden)]
pub fn remaining_mut(ptr: *mut u8, end: *mut u8) -> usize {
    (end as usize).saturating_sub(ptr as usize)
}

/// True when at least `n` bytes can be read from `ptr` before `end`.
#[inline(always)]
#[doc(hidden)]
pub fn can_read(ptr: *const u8, end: *const u8, n: usize) -> bool {
    remaining(ptr, end) >= n
}

/// True when at least `n` bytes can be written to `ptr` before `end`.
#[inline(always)]
#[doc(hidden)]
pub fn can_write(ptr: *mut u8, end: *mut u8, n: usize) -> bool {
    remaining_mut(ptr, end) >= n
}

/// Returns `true` when both input and output pointers can advance by the
/// requested byte counts without crossing their respective end pointers.
#[inline(always)]
#[doc(hidden)]
pub fn can_advance(
    in_ptr: *const u8,
    in_end: *const u8,
    in_need: usize,
    out_ptr: *mut u8,
    out_end: *mut u8,
    out_need: usize,
) -> bool {
    can_read(in_ptr, in_end, in_need) && can_write(out_ptr, out_end, out_need)
}

/// DS64 decode guard: enough room for one 64-byte block (plus preload/slack).
#[inline(always)]
#[doc(hidden)]
pub fn can_process_ds64(
    in_ptr: *const u8,
    safe_end: *const u8,
    out_ptr: *mut u8,
    out_end: *mut u8,
) -> bool {
    can_advance(in_ptr, safe_end, 32 + 64, out_ptr, out_end, 48 + 4)
}

/// DS64 decode guard: enough room for two 64-byte blocks in one iteration.
#[inline(always)]
#[doc(hidden)]
pub fn can_process_ds64_double(
    in_ptr: *const u8,
    safe_end: *const u8,
    out_ptr: *mut u8,
    out_end: *mut u8,
) -> bool {
    can_advance(in_ptr, safe_end, 32 + 2 * 64, out_ptr, out_end, 96 + 4)
}

/// 16-byte decode-tail guard (16 input -> 12 output with 4-byte store slack).
#[inline(always)]
#[doc(hidden)]
pub fn can_process_tail16(
    in_ptr: *const u8,
    safe_end: *const u8,
    out_ptr: *mut u8,
    out_end: *mut u8,
) -> bool {
    can_advance(in_ptr, safe_end, 16, out_ptr, out_end, 12 + 4)
}

/// Shared decode preflight: strict decoded length + output capacity check.
#[inline]
#[doc(hidden)]
pub fn prepare_decode_output(input: &[u8], out: &[u8]) -> Option<usize> {
    let out_len = decoded_len_strict(input)?;
    if out.len() < out_len {
        return None;
    }
    Some(out_len)
}

/// Shared encode capacity assertion for non-allocating API surface.
#[inline]
#[track_caller]
pub(crate) fn assert_encode_capacity(input_len: usize, out_len: usize) {
    let needed = crate::engine::scalar::encoded_len(input_len);
    assert!(
        out_len >= needed,
        "encode_to_slice output too small: need {}, have {}",
        needed,
        out_len
    );
}

/// Return `(consumed, written)` offsets from pointer pairs.
#[inline(always)]
#[doc(hidden)]
pub fn decode_offsets(
    in_ptr: *const u8,
    out_ptr: *mut u8,
    in_base: *const u8,
    out_base: *mut u8,
) -> (usize, usize) {
    (
        in_ptr as usize - in_base as usize,
        out_ptr as usize - out_base as usize,
    )
}

/// Last position where 4 bytes can be read safely from `input`.
#[inline(always)]
#[doc(hidden)]
pub fn safe_in_end_4(input: &[u8]) -> *const u8 {
    safe_in_end_for_width(input, 4)
}

/// Last position where `width` bytes can be read safely from `input`.
///
/// Returns `input.as_ptr()` when `input.len() < width`.
#[inline(always)]
#[doc(hidden)]
pub fn safe_in_end_for_width(input: &[u8], width: usize) -> *const u8 {
    if input.len() >= width {
        // SAFETY: `width <= input.len()`, so subtraction is in-bounds.
        unsafe { input.as_ptr().add(input.len() - width) }
    } else {
        input.as_ptr()
    }
}
