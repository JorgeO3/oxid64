//! Base64 codec engine selection and dispatch.
//!
//! This module contains the [`Base64Decoder`] trait, the [`Decoder`] dispatch
//! enum, and platform-specific engine implementations. The correct engine is
//! selected at runtime via [`Decoder::detect`].

pub mod scalar;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod ssse3;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Trait implemented by all Base64 codec engines.
///
/// The two `_to_slice` methods are the required primitives; the convenience
/// `decode` / `encode` methods provide allocating wrappers with default
/// implementations.
pub trait Base64Decoder {
    /// Decode Base64 `input` into `out`, returning the number of bytes written.
    ///
    /// Returns `None` if the input contains invalid Base64.
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize>;

    /// Encode raw bytes `input` into Base64 `out`, returning the number of
    /// bytes written.
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize;

    /// Convenience: decode Base64 input, returning a newly-allocated `Vec`.
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>> {
        let out_len = scalar::decoded_len_strict(input)?;
        let mut out = vec![0u8; out_len];
        let written = self.decode_to_slice(input, &mut out)?;
        debug_assert_eq!(written, out_len);
        Some(out)
    }

    /// Convenience: encode raw bytes, returning a newly-allocated `Vec`.
    fn encode(&self, input: &[u8]) -> Vec<u8> {
        let out_len = crate::encoded_len(input.len());
        let mut out = vec![0u8; out_len];
        let written = self.encode_to_slice(input, &mut out);
        debug_assert_eq!(written, out_len);
        out
    }
}

// ---------------------------------------------------------------------------
// Dispatch enum
// ---------------------------------------------------------------------------

/// Runtime-dispatched Base64 codec.
///
/// Wraps the best available engine for the current CPU. Obtain one via
/// [`Decoder::detect`].
pub enum Decoder {
    /// Pure-scalar fallback (all platforms).
    Scalar(scalar::ScalarDecoder),

    /// SSSE3 vectorised codec (x86/x86-64 only).
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Ssse3(ssse3::Ssse3Decoder),

    /// AVX2 vectorised codec (x86/x86-64 only).
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx2(avx2::Avx2Decoder),
}

impl Base64Decoder for Decoder {
    #[inline]
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize> {
        match self {
            Self::Scalar(d) => d.decode_to_slice(input, out),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Ssse3(d) => d.decode_to_slice(input, out),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Avx2(d) => d.decode_to_slice(input, out),
        }
    }

    #[inline]
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize {
        match self {
            Self::Scalar(d) => d.encode_to_slice(input, out),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Ssse3(d) => d.encode_to_slice(input, out),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Avx2(d) => d.encode_to_slice(input, out),
        }
    }
}

impl Decoder {
    /// Detect the best available engine for the current CPU at runtime.
    ///
    /// Priority (x86/x86-64): AVX2 > SSSE3 > Scalar.
    /// All other architectures fall back to the scalar engine.
    pub fn detect() -> Self {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::arch::is_x86_feature_detected!("avx2") {
                return Self::Avx2(avx2::Avx2Decoder);
            }
            if std::arch::is_x86_feature_detected!("ssse3") {
                return Self::Ssse3(ssse3::Ssse3Decoder::new());
            }
        }

        Self::Scalar(scalar::ScalarDecoder)
    }
}
