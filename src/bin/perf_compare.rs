use oxid64::engine::avx2::{
    Avx2Decoder, decode_avx2_kernel_partial, decode_avx2_kernel_strict,
    decode_avx2_kernel_unchecked, encode_avx2_kernel,
};
use oxid64::engine::avx512vbmi::{
    Avx512VbmiDecoder, decode_avx512_kernel_partial, decode_avx512_kernel_strict,
    encode_avx512_kernel,
};
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::ssse3::{
    Ssse3Decoder, decode_ssse3_kernel_partial, decode_ssse3_kernel_strict, encode_ssse3_kernel,
};
use oxid64::engine::{Base64Decoder, DecodeOpts};
use std::hint::black_box;
use std::process::ExitCode;

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
    "fastbase64-avx2-check",
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
    "fastbase64-avx2-encode",
    "oxid64-avx512-encode-api",
    "oxid64-avx512-encode-kernel",
    "tb64-avx512-encode",
];

type DecodeKernel = unsafe fn(&[u8], &mut [u8]) -> Option<(usize, usize)>;
type EncodeKernel = unsafe fn(&[u8], &mut [u8]) -> (usize, usize);

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

    fn fast_avx2_base64_decode(out: *mut i8, src: *const i8, srclen: usize) -> usize;
    fn fast_avx2_base64_encode(dest: *mut i8, str_: *const i8, len: usize) -> usize;
}

fn has_ssse3() -> bool {
    std::arch::is_x86_feature_detected!("ssse3")
}

fn has_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}

fn has_avx512vbmi() -> bool {
    std::arch::is_x86_feature_detected!("avx512f")
        && std::arch::is_x86_feature_detected!("avx512bw")
        && std::arch::is_x86_feature_detected!("avx512vbmi")
}

fn mode_supported(mode: &str) -> bool {
    match mode {
        mode if mode.starts_with("oxid64-ssse3") || mode.starts_with("tb64-ssse3") => has_ssse3(),
        mode if mode.starts_with("oxid64-avx2")
            || mode.starts_with("tb64-avx2")
            || mode.starts_with("fastbase64-avx2") =>
        {
            has_avx2()
        }
        mode if mode.starts_with("oxid64-avx512") || mode.starts_with("tb64-avx512") => {
            has_avx512vbmi()
        }
        _ => false,
    }
}

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

fn run_fast_decode(encoded: &[u8], out: &mut [u8], iters: usize) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let written = unsafe {
            fast_avx2_base64_decode(
                black_box(out.as_mut_ptr() as *mut i8),
                black_box(encoded.as_ptr() as *const i8),
                black_box(encoded.len()),
            )
        };
        acc ^= sample_acc(out, written);
    }
    acc
}

fn run_fast_encode(raw: &[u8], out: &mut [u8], iters: usize) -> u64 {
    let mut acc = 0u64;
    for _ in 0..iters {
        let written = unsafe {
            fast_avx2_base64_encode(
                black_box(out.as_mut_ptr() as *mut i8),
                black_box(raw.as_ptr() as *const i8),
                black_box(raw.len()),
            )
        };
        acc ^= sample_acc(out, written);
    }
    acc
}

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

            let acc = match mode.as_str() {
                "oxid64-ssse3-strict-api" => {
                    let dec = Ssse3Decoder::new();
                    run_decode_api(&dec, &encoded, &mut decode_out, iters)
                }
                "oxid64-ssse3-strict-kernel" => {
                    run_decode_kernel(decode_ssse3_kernel_strict, &encoded, &mut decode_out, iters)
                }
                "oxid64-ssse3-nonstrict-api" => {
                    let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
                    run_decode_api(&dec, &encoded, &mut decode_out, iters)
                }
                "oxid64-ssse3-nonstrict-kernel" => run_decode_kernel(
                    decode_ssse3_kernel_partial,
                    &encoded,
                    &mut decode_out,
                    iters,
                ),
                "tb64-ssse3-check" => {
                    run_c_decode(tb64v128dec_b64check, &encoded, &mut decode_out, iters)
                }
                "tb64-ssse3-partial" => run_c_decode(tb64v128dec, &encoded, &mut decode_out, iters),
                "tb64-ssse3-unchecked" => {
                    run_c_decode(tb64v128dec_nb64check, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx2-strict-api" => {
                    let dec = Avx2Decoder::new();
                    run_decode_api(&dec, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx2-strict-kernel" => {
                    run_decode_kernel(decode_avx2_kernel_strict, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx2-nonstrict-api" => {
                    let dec = Avx2Decoder::with_opts(DecodeOpts { strict: false });
                    run_decode_api(&dec, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx2-nonstrict-kernel" => {
                    run_decode_kernel(decode_avx2_kernel_partial, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx2-unchecked-kernel" => run_decode_kernel(
                    decode_avx2_kernel_unchecked,
                    &encoded,
                    &mut decode_out,
                    iters,
                ),
                "tb64-avx2-check" => {
                    run_c_decode(tb64v256dec_b64check, &encoded, &mut decode_out, iters)
                }
                "tb64-avx2-partial" => run_c_decode(tb64v256dec, &encoded, &mut decode_out, iters),
                "tb64-avx2-unchecked" => {
                    run_c_decode(tb64v256dec_nb64check, &encoded, &mut decode_out, iters)
                }
                "fastbase64-avx2-check" => run_fast_decode(&encoded, &mut decode_out, iters),
                "oxid64-avx512-strict-api" => {
                    let dec = Avx512VbmiDecoder::new();
                    run_decode_api(&dec, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx512-strict-kernel" => run_decode_kernel(
                    decode_avx512_kernel_strict,
                    &encoded,
                    &mut decode_out,
                    iters,
                ),
                "oxid64-avx512-nonstrict-api" => {
                    let dec = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });
                    run_decode_api(&dec, &encoded, &mut decode_out, iters)
                }
                "oxid64-avx512-nonstrict-kernel" => run_decode_kernel(
                    decode_avx512_kernel_partial,
                    &encoded,
                    &mut decode_out,
                    iters,
                ),
                "tb64-avx512-check" => {
                    run_c_decode(tb64v512dec_b64check, &encoded, &mut decode_out, iters)
                }
                "tb64-avx512-partial" => {
                    run_c_decode(tb64v512dec, &encoded, &mut decode_out, iters)
                }
                "tb64-avx512-unchecked" => {
                    run_c_decode(tb64v512dec_nb64check, &encoded, &mut decode_out, iters)
                }
                "oxid64-ssse3-encode-api" => {
                    let enc = Ssse3Decoder::new();
                    run_encode_api(&enc, &raw, &mut encode_out, iters)
                }
                "oxid64-ssse3-encode-kernel" => {
                    run_encode_kernel(encode_ssse3_kernel, &raw, &mut encode_out, iters)
                }
                "tb64-ssse3-encode" => run_c_encode(tb64v128enc, &raw, &mut encode_out, iters),
                "oxid64-avx2-encode-api" => {
                    let enc = Avx2Decoder::new();
                    run_encode_api(&enc, &raw, &mut encode_out, iters)
                }
                "oxid64-avx2-encode-kernel" => {
                    run_encode_kernel(encode_avx2_kernel, &raw, &mut encode_out, iters)
                }
                "tb64-avx2-encode" => run_c_encode(tb64v256enc, &raw, &mut encode_out, iters),
                "fastbase64-avx2-encode" => run_fast_encode(&raw, &mut encode_out, iters),
                "oxid64-avx512-encode-api" => {
                    let enc = Avx512VbmiDecoder::new();
                    run_encode_api(&enc, &raw, &mut encode_out, iters)
                }
                "oxid64-avx512-encode-kernel" => {
                    run_encode_kernel(encode_avx512_kernel, &raw, &mut encode_out, iters)
                }
                "tb64-avx512-encode" => run_c_encode(tb64v512enc, &raw, &mut encode_out, iters),
                _ => unreachable!(),
            };

            println!("{acc}");
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}
