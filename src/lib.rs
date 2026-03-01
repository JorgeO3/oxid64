//! High-performance Base64 codec with SIMD acceleration.
//!
//! `oxid64` provides Base64 encoding and decoding using hand-tuned SIMD
//! kernels for x86-64 (SSSE3, AVX2, AVX-512 VBMI), AArch64 (NEON), and
//! WebAssembly (SIMD128), with a fast scalar fallback on all platforms.
//!
//! # Quick start
//!
//! Use [`Decoder::detect`] to obtain the fastest engine available at runtime,
//! then call [`Base64Decoder::decode`] or [`Base64Decoder::encode`]:
//!
//! ```
//! use oxid64::{Decoder, Base64Decoder};
//!
//! let engine = Decoder::detect();
//!
//! // Encode
//! let encoded = engine.encode(b"Hello, world!");
//! assert_eq!(encoded, b"SGVsbG8sIHdvcmxkIQ==");
//!
//! // Decode
//! let decoded = engine.decode(&encoded).expect("valid base64");
//! assert_eq!(decoded, b"Hello, world!");
//! ```
//!
//! # Low-level slice API
//!
//! For zero-allocation paths, use [`Base64Decoder::decode_to_slice`] and
//! [`Base64Decoder::encode_to_slice`] with pre-allocated buffers:
//!
//! ```
//! use oxid64::{Decoder, Base64Decoder, encoded_len, decoded_len};
//!
//! let engine = Decoder::detect();
//! let raw = b"Hello, world!";
//!
//! // Encode into a caller-provided buffer.
//! let enc_len = encoded_len(raw.len());
//! let mut enc_buf = vec![0u8; enc_len];
//! let written = engine.encode_to_slice(raw, &mut enc_buf);
//! assert_eq!(written, enc_len);
//!
//! // Decode back.
//! let dec_len = decoded_len(&enc_buf).expect("valid length");
//! let mut dec_buf = vec![0u8; dec_len];
//! let decoded = engine.decode_to_slice(&enc_buf, &mut dec_buf).expect("valid base64");
//! assert_eq!(&dec_buf[..decoded], raw);
//! ```
//!
//! # Engine selection
//!
//! [`Decoder::detect`] probes CPU features at runtime and returns the best
//! available engine. You can also instantiate a specific engine directly
//! through its module (e.g. [`engine::scalar::ScalarDecoder`]).
//!
//! # Strictness
//!
//! SIMD engines accept a [`engine::DecodeOpts`] to control validation
//! strictness. The default (`strict: true`) validates every input byte,
//! suitable for untrusted data. Setting `strict: false` samples fewer
//! vectors for higher throughput on trusted input.

#![warn(missing_docs)]

pub mod engine;

// Re-export the primary public API at the crate root.
pub use engine::{Base64Decoder, Decoder};

/// Compute the Base64-encoded length for `n` raw input bytes (with padding).
///
/// # Examples
///
/// ```
/// assert_eq!(oxid64::encoded_len(0), 0);
/// assert_eq!(oxid64::encoded_len(1), 4);
/// assert_eq!(oxid64::encoded_len(3), 4);
/// assert_eq!(oxid64::encoded_len(13), 20);
/// ```
#[inline]
pub const fn encoded_len(n: usize) -> usize {
    engine::scalar::encoded_len(n)
}

/// Compute the decoded byte length for a Base64 input slice (strict, with
/// padding). Returns `None` if the input length is not a multiple of 4.
///
/// # Examples
///
/// ```
/// assert_eq!(oxid64::decoded_len(b"SGVsbG8="), Some(5));
/// assert_eq!(oxid64::decoded_len(b"SGVsbG8sIHdvcmxkIQ=="), Some(13));
/// assert_eq!(oxid64::decoded_len(b"abc"), None); // not a multiple of 4
/// ```
#[inline]
pub const fn decoded_len(b64: &[u8]) -> Option<usize> {
    engine::scalar::decoded_len_strict(b64)
}
