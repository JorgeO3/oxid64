use oxid64::engine::avx2::{decode_avx2_kernel_partial, Avx2Decoder};
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::ssse3::{decode_ssse3_kernel_partial, encode_ssse3_kernel, Ssse3Decoder};
use oxid64::engine::DecodeOpts;
use std::hint::black_box;

unsafe extern "C" {
    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn fast_avx2_base64_decode(out: *mut i8, src: *const i8, srclen: usize) -> usize;
}

fn make_input(size: usize) -> Vec<u8> {
    let mut input = vec![0u8; size];
    for (i, b) in input.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    input
}

fn make_encoded(raw: &[u8]) -> (Vec<u8>, usize) {
    let mut encoded = vec![0u8; raw.len().div_ceil(3) * 4 + 64];
    let enc_len = encode_base64_fast(raw, &mut encoded);
    encoded.truncate(enc_len);
    let decoded_len = (raw.len() / 3) * 3 + (raw.len() % 3);
    (encoded, decoded_len)
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

fn main() {
    let mut args = std::env::args();
    let _bin = args.next();
    let mode = args.next().expect("mode");
    let size: usize = args.next().expect("size").parse().expect("size usize");
    let iters: usize = args.next().expect("iters").parse().expect("iters usize");

    let raw = make_input(size);
    let (encoded, decoded_len) = make_encoded(&raw);
    let mut decode_out = vec![0u8; decoded_len + 64];
    let mut encode_out = vec![0u8; raw.len().div_ceil(3) * 4 + 64];

    let mut acc = 0u64;

    match mode.as_str() {
        "oxid64-avx2-nonstrict-api" => {
            assert!(std::arch::is_x86_feature_detected!("avx2"));
            let dec = Avx2Decoder::with_opts(DecodeOpts { strict: false });
            for _ in 0..iters {
                let written = dec
                    .decode_to_slice(
                        black_box(encoded.as_slice()),
                        black_box(decode_out.as_mut_slice()),
                    )
                    .expect("decode");
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "oxid64-avx2-nonstrict-kernel" => {
            assert!(std::arch::is_x86_feature_detected!("avx2"));
            for _ in 0..iters {
                let (_consumed, written) = unsafe {
                    decode_avx2_kernel_partial(
                        black_box(encoded.as_slice()),
                        black_box(decode_out.as_mut_slice()),
                    )
                    .expect("kernel decode")
                };
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "tb64-avx2-partial" => {
            for _ in 0..iters {
                let written = unsafe {
                    tb64v256dec(
                        black_box(encoded.as_ptr()),
                        black_box(encoded.len()),
                        black_box(decode_out.as_mut_ptr()),
                    )
                };
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "fastbase64-avx2-check" => {
            for _ in 0..iters {
                let written = unsafe {
                    fast_avx2_base64_decode(
                        black_box(decode_out.as_mut_ptr() as *mut i8),
                        black_box(encoded.as_ptr() as *const i8),
                        black_box(encoded.len()),
                    )
                };
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "oxid64-ssse3-nonstrict-api" => {
            assert!(std::arch::is_x86_feature_detected!("ssse3"));
            let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
            for _ in 0..iters {
                let written = dec
                    .decode_to_slice(
                        black_box(encoded.as_slice()),
                        black_box(decode_out.as_mut_slice()),
                    )
                    .expect("decode");
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "oxid64-ssse3-nonstrict-kernel" => {
            assert!(std::arch::is_x86_feature_detected!("ssse3"));
            for _ in 0..iters {
                let (_consumed, written) = unsafe {
                    decode_ssse3_kernel_partial(
                        black_box(encoded.as_slice()),
                        black_box(decode_out.as_mut_slice()),
                    )
                    .expect("kernel decode")
                };
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "tb64-ssse3-partial" => {
            for _ in 0..iters {
                let written = unsafe {
                    tb64v128dec(
                        black_box(encoded.as_ptr()),
                        black_box(encoded.len()),
                        black_box(decode_out.as_mut_ptr()),
                    )
                };
                acc ^= sample_acc(&decode_out, written);
            }
        }
        "oxid64-ssse3-encode-api" => {
            assert!(std::arch::is_x86_feature_detected!("ssse3"));
            let enc = Ssse3Decoder::new();
            for _ in 0..iters {
                let written = enc.encode_to_slice(
                    black_box(raw.as_slice()),
                    black_box(encode_out.as_mut_slice()),
                );
                acc ^= sample_acc(&encode_out, written);
            }
        }
        "oxid64-ssse3-encode-kernel" => {
            assert!(std::arch::is_x86_feature_detected!("ssse3"));
            for _ in 0..iters {
                let (_consumed, written) = unsafe {
                    encode_ssse3_kernel(
                        black_box(raw.as_slice()),
                        black_box(encode_out.as_mut_slice()),
                    )
                };
                acc ^= sample_acc(&encode_out, written);
            }
        }
        "tb64-ssse3-encode" => {
            for _ in 0..iters {
                let written = unsafe {
                    tb64v128enc(
                        black_box(raw.as_ptr()),
                        black_box(raw.len()),
                        black_box(encode_out.as_mut_ptr()),
                    )
                };
                acc ^= sample_acc(&encode_out, written);
            }
        }
        _ => panic!("unknown mode: {mode}"),
    }

    println!("{acc}");
}
