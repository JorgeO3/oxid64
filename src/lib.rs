pub mod engine;

// Re-export the primary public API at the crate root.
pub use engine::{Base64Decoder, Decoder};

/// Compute the Base64-encoded length for `n` raw input bytes (with padding).
#[inline]
pub const fn encoded_len(n: usize) -> usize {
    ((n + 2) / 3) * 4
}

/// Compute the decoded byte length for a Base64 input slice (strict, with
/// padding). Returns `None` if the input length is not a multiple of 4.
#[inline]
pub const fn decoded_len(b64: &[u8]) -> Option<usize> {
    engine::scalar::decoded_len_strict(b64)
}
