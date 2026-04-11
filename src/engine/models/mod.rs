//! Pure verification models for backend scheduling and contracts.
//!
//! These modules intentionally avoid target intrinsics and runtime-only state so
//! they can be shared by integration tests, Miri, Kani, and model fuzzing.

#[doc(hidden)]
pub mod avx2;
#[doc(hidden)]
pub mod avx512vbmi;
#[doc(hidden)]
pub mod neon;
#[doc(hidden)]
pub mod ssse3;
#[doc(hidden)]
pub mod wasm_simd128;
