//! Base64 codec engine selection and dispatch.
//!
//! This module contains the [`Base64Decoder`] trait, the [`Decoder`] dispatch
//! enum, and platform-specific engine implementations. The correct engine is
//! selected at runtime via [`Decoder::detect`].

pub mod scalar;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod avx512vbmi;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod ssse3;

#[cfg(target_arch = "aarch64")]
pub mod neon;

#[cfg(target_arch = "wasm32")]
pub mod wasm_simd128;

// ---------------------------------------------------------------------------
// Byte-to-signed helpers (zero-cost cast for SIMD intrinsic arguments)
// ---------------------------------------------------------------------------

/// Reinterpret a `u8` bit pattern as `i8` (compile-time safe).
#[allow(dead_code)]
pub(crate) const fn b2i(v: u8) -> i8 {
    v as i8
}

/// Reinterpret a `u32` bit pattern as `i32` (compile-time safe).
#[allow(dead_code)]
pub(crate) const fn w2i(v: u32) -> i32 {
    v as i32
}

/// Reinterpret a `u64` bit pattern as `i64` (compile-time safe).
#[allow(dead_code)]
pub(crate) const fn d2i(v: u64) -> i64 {
    v as i64
}

// ---------------------------------------------------------------------------
// Shared SIMD decode helpers
// ---------------------------------------------------------------------------

/// Run a SIMD decode function, then fall back to scalar for the remaining tail.
///
/// Every SIMD engine uses the same pattern: call the `target_feature`-gated
/// kernel which returns `(consumed, written)`, then finish any leftover bytes
/// with the scalar decoder. This helper eliminates the duplication.
///
/// # Safety
///
/// The caller must ensure the target feature required by `simd_fn` is
/// available before calling this function.
#[inline]
#[allow(clippy::type_complexity)]
pub(crate) fn dispatch_decode(
    input: &[u8],
    out: &mut [u8],
    simd_fn: unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>,
) -> Option<usize> {
    // SAFETY: caller guarantees the required target feature is available.
    let (consumed, mut written) = unsafe { simd_fn(input, out)? };

    if consumed < input.len() {
        let tail_written = scalar::decode_base64_fast(&input[consumed..], &mut out[written..])?;
        written += tail_written;
    }
    Some(written)
}

/// Return `(consumed, written)` byte offsets between a current pointer pair
/// and their respective base pointers.
///
/// Used inside SIMD decode kernels to convert raw-pointer arithmetic back to
/// slice-friendly `usize` offsets.
#[inline]
pub(crate) fn offsets(
    in_ptr: *const u8,
    out_ptr: *const u8,
    in_base: *const u8,
    out_base: *const u8,
) -> (usize, usize) {
    (
        in_ptr as usize - in_base as usize,
        out_ptr as usize - out_base as usize,
    )
}

// ---------------------------------------------------------------------------
// Shared decoder configuration
// ---------------------------------------------------------------------------

/// Decoder configuration options.
///
/// Controls the strictness of input validation during SIMD Base64 decoding.
///
/// # Default
///
/// The default configuration enables strict mode (`strict: true`), which
/// validates every input byte. This is the safe choice for untrusted input.
///
/// ```
/// # use oxid64::engine::DecodeOpts;
/// let opts = DecodeOpts::default();
/// assert!(opts.strict);
/// ```
pub struct DecodeOpts {
    /// When `true`, every input vector is validated (CHECK1 mode).
    /// When `false`, only 1 of every N vectors is validated (CHECK0 mode).
    pub strict: bool,
}

impl Default for DecodeOpts {
    #[inline]
    fn default() -> Self {
        Self { strict: true }
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Trait implemented by all Base64 codec engines.
///
/// The two `_to_slice` methods are the required primitives; the convenience
/// `decode` / `encode` methods provide allocating wrappers with default
/// implementations.
///
/// # Examples
///
/// ```
/// use oxid64::{Decoder, Base64Decoder};
///
/// let engine = Decoder::detect();
/// let encoded = engine.encode(b"oxid64");
/// let decoded = engine.decode(&encoded).unwrap();
/// assert_eq!(decoded, b"oxid64");
/// ```
pub trait Base64Decoder {
    /// Decode Base64 `input` into `out`, returning the number of bytes written.
    ///
    /// Returns `None` if the input contains invalid Base64.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxid64::{Decoder, Base64Decoder, decoded_len};
    ///
    /// let engine = Decoder::detect();
    /// let b64 = b"SGVsbG8=";
    /// let len = decoded_len(b64).unwrap();
    /// let mut buf = vec![0u8; len];
    /// let n = engine.decode_to_slice(b64, &mut buf).unwrap();
    /// assert_eq!(&buf[..n], b"Hello");
    /// ```
    fn decode_to_slice(&self, input: &[u8], out: &mut [u8]) -> Option<usize>;

    /// Encode raw bytes `input` into Base64 `out`, returning the number of
    /// bytes written.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxid64::{Decoder, Base64Decoder, encoded_len};
    ///
    /// let engine = Decoder::detect();
    /// let raw = b"Hello";
    /// let mut buf = vec![0u8; encoded_len(raw.len())];
    /// let n = engine.encode_to_slice(raw, &mut buf);
    /// assert_eq!(&buf[..n], b"SGVsbG8=");
    /// ```
    fn encode_to_slice(&self, input: &[u8], out: &mut [u8]) -> usize;

    /// Convenience: decode Base64 input, returning a newly-allocated `Vec`.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxid64::{Decoder, Base64Decoder};
    ///
    /// let engine = Decoder::detect();
    /// let decoded = engine.decode(b"b3hpZDY0").unwrap();
    /// assert_eq!(decoded, b"oxid64");
    /// ```
    fn decode(&self, input: &[u8]) -> Option<Vec<u8>> {
        let out_len = scalar::decoded_len_strict(input)?;
        let mut out = vec![0u8; out_len];
        let written = self.decode_to_slice(input, &mut out)?;
        debug_assert_eq!(written, out_len);
        Some(out)
    }

    /// Convenience: encode raw bytes, returning a newly-allocated `Vec`.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxid64::{Decoder, Base64Decoder};
    ///
    /// let engine = Decoder::detect();
    /// assert_eq!(engine.encode(b"oxid64"), b"b3hpZDY0");
    /// ```
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
///
/// # Examples
///
/// ```
/// use oxid64::{Decoder, Base64Decoder};
///
/// let engine = Decoder::detect();
/// let encoded = engine.encode(b"hello");
/// assert_eq!(engine.decode(&encoded).unwrap(), b"hello");
/// ```
pub enum Decoder {
    /// Pure-scalar fallback (all platforms).
    Scalar(scalar::ScalarDecoder),

    /// SSSE3 vectorised codec (x86/x86-64 only).
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Ssse3(ssse3::Ssse3Decoder),

    /// AVX2 vectorised codec (x86/x86-64 only).
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx2(avx2::Avx2Decoder),

    /// AVX-512 VBMI vectorised codec (x86/x86-64 only, Ice Lake+).
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx512Vbmi(avx512vbmi::Avx512VbmiDecoder),

    /// NEON vectorised codec (AArch64 only).
    #[cfg(target_arch = "aarch64")]
    Neon(neon::NeonDecoder),

    /// WASM SIMD128 vectorised codec (wasm32 only).
    #[cfg(target_arch = "wasm32")]
    WasmSimd128(wasm_simd128::WasmSimd128Decoder),
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
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Avx512Vbmi(d) => d.decode_to_slice(input, out),
            #[cfg(target_arch = "aarch64")]
            Self::Neon(d) => d.decode_to_slice(input, out),
            #[cfg(target_arch = "wasm32")]
            Self::WasmSimd128(d) => d.decode_to_slice(input, out),
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
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Avx512Vbmi(d) => d.encode_to_slice(input, out),
            #[cfg(target_arch = "aarch64")]
            Self::Neon(d) => d.encode_to_slice(input, out),
            #[cfg(target_arch = "wasm32")]
            Self::WasmSimd128(d) => d.encode_to_slice(input, out),
        }
    }
}

impl Decoder {
    /// Detect the best available engine for the current CPU at runtime.
    ///
    /// Priority (x86/x86-64): AVX-512 VBMI > AVX2 > SSSE3 > Scalar.
    /// Priority (AArch64): NEON > Scalar.
    /// Priority (wasm32): SIMD128 (compile-time) > Scalar.
    /// All other architectures fall back to the scalar engine.
    ///
    /// # Examples
    ///
    /// ```
    /// use oxid64::Decoder;
    ///
    /// let engine = Decoder::detect();
    /// // `engine` is now the fastest codec for this CPU.
    /// ```
    pub fn detect() -> Self {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if std::arch::is_x86_feature_detected!("avx512vbmi") {
                return Self::Avx512Vbmi(avx512vbmi::Avx512VbmiDecoder::new());
            }
            if std::arch::is_x86_feature_detected!("avx2") {
                return Self::Avx2(avx2::Avx2Decoder::new());
            }
            if std::arch::is_x86_feature_detected!("ssse3") {
                return Self::Ssse3(ssse3::Ssse3Decoder::new());
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            if std::arch::is_aarch64_feature_detected!("neon") {
                return Self::Neon(neon::NeonDecoder::new());
            }
        }

        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        {
            Self::WasmSimd128(wasm_simd128::WasmSimd128Decoder::new())
        }

        #[cfg(not(all(target_arch = "wasm32", target_feature = "simd128")))]
        Self::Scalar(scalar::ScalarDecoder)
    }
}
