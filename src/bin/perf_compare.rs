// perf_compare: micro-benchmark harness for comparing oxid64 vs Turbo-Base64 C.
//
// Build requires features: c-benchmarks + perf-tools
// The binary is architecture-aware: x86/x86_64 exposes SSSE3/AVX2/AVX-512 modes,
// aarch64 exposes NEON modes.  All ISA-specific code is gated with #[cfg].

// ---------------------------------------------------------------------------
// x86 / x86_64 imports
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use oxid64::engine::avx2::{
    Avx2Decoder, decode_avx2_kernel_partial, decode_avx2_kernel_strict,
    decode_avx2_kernel_unchecked, encode_avx2_kernel,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use oxid64::engine::avx512vbmi::{
    Avx512VbmiDecoder, decode_avx512_kernel_partial, decode_avx512_kernel_strict,
    encode_avx512_kernel,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use oxid64::engine::ssse3::{
    Ssse3Decoder, decode_ssse3_kernel_partial, decode_ssse3_kernel_strict, encode_ssse3_kernel,
};

// ---------------------------------------------------------------------------
// aarch64 imports
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
use oxid64::engine::neon::{
    NeonDecoder, decode_neon_kernel_partial, decode_neon_kernel_strict, encode_neon_kernel,
};

// ---------------------------------------------------------------------------
// Common imports
// ---------------------------------------------------------------------------

use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::{Base64Decoder, DecodeOpts};
use std::hint::black_box;
use std::process::ExitCode;

// ---------------------------------------------------------------------------
// Mode lists — one per target architecture
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
const MODES: &[&str] = &[
    "oxid64-ssse3-strict-api",
    "oxid64-ssse3-strict-kernel",
    "oxid64-ssse3-nonstrict-api",
    "oxid64-ssse3-nonstrict-kernel",
    "tb64-ssse3-check",
    "tb64-ssse3-partial",
    "tb64-ssse3-unchecked",
    "oxid64-avx2-strict-api",
    "oxid64-avx2-strict-kernel",
    "oxid64-avx2-nonstrict-api",
    "oxid64-avx2-nonstrict-kernel",
    "oxid64-avx2-unchecked-kernel",
    "tb64-avx2-check",
    "tb64-avx2-partial",
    "tb64-avx2-unchecked",
    "oxid64-avx512-strict-api",
    "oxid64-avx512-strict-kernel",
    "oxid64-avx512-nonstrict-api",
    "oxid64-avx512-nonstrict-kernel",
    "tb64-avx512-check",
    "tb64-avx512-partial",
    "tb64-avx512-unchecked",
    "oxid64-ssse3-encode-api",
    "oxid64-ssse3-encode-kernel",
    "tb64-ssse3-encode",
    "oxid64-avx2-encode-api",
    "oxid64-avx2-encode-kernel",
    "tb64-avx2-encode",
    "oxid64-avx512-encode-api",
    "oxid64-avx512-encode-kernel",
    "tb64-avx512-encode",
];

#[cfg(target_arch = "aarch64")]
const MODES: &[&str] = &[
    "oxid64-neon-strict-api",
    "oxid64-neon-strict-kernel",
    "oxid64-neon-nonstrict-api",
    "oxid64-neon-nonstrict-kernel",
    "tb64-neon-check",
    "tb64-neon-partial",
    "tb64-neon-unchecked",
    "oxid64-neon-encode-api",
    "oxid64-neon-encode-kernel",
    "tb64-neon-encode",
];

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
const MODES: &[&str] = &[];

// ---------------------------------------------------------------------------
// Kernel type aliases
// ---------------------------------------------------------------------------

type DecodeKernel = unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>;
type EncodeKernel = unsafe fn(&[u8], &mut [u8]) -> (usize, usize);

// ---------------------------------------------------------------------------
// FFI: x86 / x86_64
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe extern "C" {
    fn tb64v128dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64v128dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

// ---------------------------------------------------------------------------
// FFI: aarch64
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    fn tb64v128dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

// ---------------------------------------------------------------------------
// CPU feature detection
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn has_ssse3() -> bool {
    std::arch::is_x86_feature_detected!("ssse3")
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn has_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn has_avx512vbmi() -> bool {
    std::arch::is_x86_feature_detected!("avx512f")
        && std::arch::is_x86_feature_detected!("avx512bw")
        && std::arch::is_x86_feature_detected!("avx512vbmi")
}

#[cfg(target_arch = "aarch64")]
fn has_neon() -> bool {
    std::arch::is_aarch64_feature_detected!("neon")
}

fn mode_supported(mode: &str) -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        match mode {
            m if m.starts_with("oxid64-ssse3") || m.starts_with("tb64-ssse3") => has_ssse3(),
            m if m.starts_with("oxid64-avx2") || m.starts_with("tb64-avx2") => has_avx2(),
            m if m.starts_with("oxid64-avx512") || m.starts_with("tb64-avx512") => has_avx512vbmi(),
            _ => false,
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        match mode {
            m if m.starts_with("oxid64-neon") || m.starts_with("tb64-neon") => has_neon(),
            _ => false,
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = mode;
        false
    }
}

// ---------------------------------------------------------------------------
// Harness helpers
// ---------------------------------------------------------------------------

fn usage() {
    eprintln!("Usage: perf_compare <mode> <size> <iters>");
    eprintln!("       perf_compare --list");
    eprintln!("       perf_compare --supported");
    eprintln!();
    eprintln!("Modes:");
    for mode in MODES {
        eprintln!("  {mode}");
    }
}

fn make_input(size: usize) -> Vec<u8> {
    let mut input = vec![0u8; size];
    for (i, b) in input.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    input
}

fn make_encoded(raw: &[u8]) -> Vec<u8> {
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4 + 64];
    let enc_len = encode_base64_fast(raw, &mut encoded);
    encoded.truncate(enc_len);
    encoded
}

fn sample_acc(buf: &[u8], written: usize) -> u64 {
    if written == 0 {
        return 0;
    }
    let mid = written / 2;
    (written as u64)
        ^ (buf[0] as u64)
        ^ ((buf[mid] as u64) << 8)
        ^ ((buf[written - 1] as u64) << 16)
}

fn run_decode_api(
    decoder: &impl Base64Decoder,
    encoded: &[u8],
    out: &mut [u8],
    iters: usize,
) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let written = decoder
            .decode_to_slice(black_box(encoded), black_box(out))
            .expect("decode");
        acc ^= sample_acc(out, written);
    }
    acc
}

fn run_encode_api(encoder: &impl Base64Decoder, raw: &[u8], out: &mut [u8], iters: usize) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let written = encoder.encode_to_slice(black_box(raw), black_box(out));
        acc ^= sample_acc(out, written);
    }
    acc
}

fn run_decode_kernel(kernel: DecodeKernel, encoded: &[u8], out: &mut [u8], iters: usize) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let (_, written) =
            unsafe { kernel(black_box(encoded), black_box(out)).expect("kernel decode") };
        acc ^= sample_acc(out, written);
    }
    acc
}

fn run_encode_kernel(kernel: EncodeKernel, raw: &[u8], out: &mut [u8], iters: usize) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let (_, written) = unsafe { kernel(black_box(raw), black_box(out)) };
        acc ^= sample_acc(out, written);
    }
    acc
}

fn run_c_decode(
    func: unsafe extern "C" fn(*const u8, usize, *mut u8) -> usize,
    encoded: &[u8],
    out: &mut [u8],
    iters: usize,
) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let written = unsafe {
            func(
                black_box(encoded.as_ptr()),
                black_box(encoded.len()),
                black_box(out.as_mut_ptr()),
            )
        };
        acc ^= sample_acc(out, written);
    }
    acc
}

fn run_c_encode(
    func: unsafe extern "C" fn(*const u8, usize, *mut u8) -> usize,
    raw: &[u8],
    out: &mut [u8],
    iters: usize,
) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let written = unsafe {
            func(
                black_box(raw.as_ptr()),
                black_box(raw.len()),
                black_box(out.as_mut_ptr()),
            )
        };
        acc ^= sample_acc(out, written);
    }
    acc
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    match args.as_slice() {
        [_, flag] if flag == "--help" || flag == "-h" => {
            usage();
            ExitCode::SUCCESS
        }
        [_, flag] if flag == "--list" => {
            for mode in MODES {
                println!("{mode}");
            }
            ExitCode::SUCCESS
        }
        [_, flag] if flag == "--supported" => {
            for mode in MODES {
                if mode_supported(mode) {
                    println!("{mode}");
                }
            }
            ExitCode::SUCCESS
        }
        [_, mode, size, iters] => {
            if !MODES.contains(&mode.as_str()) {
                eprintln!("unknown mode: {mode}");
                usage();
                return ExitCode::from(2);
            }
            if !mode_supported(mode) {
                eprintln!("unsupported mode on this CPU: {mode}");
                return ExitCode::from(2);
            }

            let size: usize = size.parse().expect("size usize");
            let iters: usize = iters.parse().expect("iters usize");

            let raw = make_input(size);
            let encoded = make_encoded(&raw);
            let mut decode_out = vec![0u8; raw.len() + 64];
            let mut encode_out = vec![0u8; raw.len().div_ceil(3) * 4 + 64];

            let acc = dispatch(
                mode,
                &raw,
                &encoded,
                &mut decode_out,
                &mut encode_out,
                iters,
            );

            println!("{acc}");
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch — x86 / x86_64
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn dispatch(
    mode: &str,
    raw: &[u8],
    encoded: &[u8],
    decode_out: &mut [u8],
    encode_out: &mut [u8],
    iters: usize,
) -> u64 {
    match mode {
        "oxid64-ssse3-strict-api" => {
            let dec = Ssse3Decoder::new();
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-ssse3-strict-kernel" => {
            run_decode_kernel(decode_ssse3_kernel_strict, encoded, decode_out, iters)
        }
        "oxid64-ssse3-nonstrict-api" => {
            let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-ssse3-nonstrict-kernel" => {
            run_decode_kernel(decode_ssse3_kernel_partial, encoded, decode_out, iters)
        }
        "tb64-ssse3-check" => run_c_decode(tb64v128dec_b64check, encoded, decode_out, iters),
        "tb64-ssse3-partial" => run_c_decode(tb64v128dec, encoded, decode_out, iters),
        "tb64-ssse3-unchecked" => run_c_decode(tb64v128dec_nb64check, encoded, decode_out, iters),
        "oxid64-avx2-strict-api" => {
            let dec = Avx2Decoder::new();
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-avx2-strict-kernel" => {
            run_decode_kernel(decode_avx2_kernel_strict, encoded, decode_out, iters)
        }
        "oxid64-avx2-nonstrict-api" => {
            let dec = Avx2Decoder::with_opts(DecodeOpts { strict: false });
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-avx2-nonstrict-kernel" => {
            run_decode_kernel(decode_avx2_kernel_partial, encoded, decode_out, iters)
        }
        "oxid64-avx2-unchecked-kernel" => {
            run_decode_kernel(decode_avx2_kernel_unchecked, encoded, decode_out, iters)
        }
        "tb64-avx2-check" => run_c_decode(tb64v256dec_b64check, encoded, decode_out, iters),
        "tb64-avx2-partial" => run_c_decode(tb64v256dec, encoded, decode_out, iters),
        "tb64-avx2-unchecked" => run_c_decode(tb64v256dec_nb64check, encoded, decode_out, iters),
        "oxid64-avx512-strict-api" => {
            let dec = Avx512VbmiDecoder::new();
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-avx512-strict-kernel" => {
            run_decode_kernel(decode_avx512_kernel_strict, encoded, decode_out, iters)
        }
        "oxid64-avx512-nonstrict-api" => {
            let dec = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-avx512-nonstrict-kernel" => {
            run_decode_kernel(decode_avx512_kernel_partial, encoded, decode_out, iters)
        }
        "tb64-avx512-check" => run_c_decode(tb64v512dec_b64check, encoded, decode_out, iters),
        "tb64-avx512-partial" => run_c_decode(tb64v512dec, encoded, decode_out, iters),
        "tb64-avx512-unchecked" => run_c_decode(tb64v512dec_nb64check, encoded, decode_out, iters),
        "oxid64-ssse3-encode-api" => {
            let enc = Ssse3Decoder::new();
            run_encode_api(&enc, raw, encode_out, iters)
        }
        "oxid64-ssse3-encode-kernel" => {
            run_encode_kernel(encode_ssse3_kernel, raw, encode_out, iters)
        }
        "tb64-ssse3-encode" => run_c_encode(tb64v128enc, raw, encode_out, iters),
        "oxid64-avx2-encode-api" => {
            let enc = Avx2Decoder::new();
            run_encode_api(&enc, raw, encode_out, iters)
        }
        "oxid64-avx2-encode-kernel" => {
            run_encode_kernel(encode_avx2_kernel, raw, encode_out, iters)
        }
        "tb64-avx2-encode" => run_c_encode(tb64v256enc, raw, encode_out, iters),
        "oxid64-avx512-encode-api" => {
            let enc = Avx512VbmiDecoder::new();
            run_encode_api(&enc, raw, encode_out, iters)
        }
        "oxid64-avx512-encode-kernel" => {
            run_encode_kernel(encode_avx512_kernel, raw, encode_out, iters)
        }
        "tb64-avx512-encode" => run_c_encode(tb64v512enc, raw, encode_out, iters),
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Dispatch — aarch64
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
fn dispatch(
    mode: &str,
    raw: &[u8],
    encoded: &[u8],
    decode_out: &mut [u8],
    encode_out: &mut [u8],
    iters: usize,
) -> u64 {
    match mode {
        "oxid64-neon-strict-api" => {
            let dec = NeonDecoder::new();
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-neon-strict-kernel" => {
            run_decode_kernel(decode_neon_kernel_strict, encoded, decode_out, iters)
        }
        "oxid64-neon-nonstrict-api" => {
            let dec = NeonDecoder::with_opts(DecodeOpts { strict: false });
            run_decode_api(&dec, encoded, decode_out, iters)
        }
        "oxid64-neon-nonstrict-kernel" => {
            run_decode_kernel(decode_neon_kernel_partial, encoded, decode_out, iters)
        }
        "tb64-neon-check" => run_c_decode(tb64v128dec_b64check, encoded, decode_out, iters),
        "tb64-neon-partial" => run_c_decode(tb64v128dec, encoded, decode_out, iters),
        "tb64-neon-unchecked" => run_c_decode(tb64v128dec_nb64check, encoded, decode_out, iters),
        "oxid64-neon-encode-api" => {
            let enc = NeonDecoder::new();
            run_encode_api(&enc, raw, encode_out, iters)
        }
        "oxid64-neon-encode-kernel" => {
            run_encode_kernel(encode_neon_kernel, raw, encode_out, iters)
        }
        "tb64-neon-encode" => run_c_encode(tb64v128enc, raw, encode_out, iters),
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Dispatch — fallback (unsupported arch)
// ---------------------------------------------------------------------------

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
fn dispatch(
    _mode: &str,
    _raw: &[u8],
    _encoded: &[u8],
    _decode_out: &mut [u8],
    _encode_out: &mut [u8],
    _iters: usize,
) -> u64 {
    eprintln!("perf_compare: no modes available on this architecture");
    0
}
