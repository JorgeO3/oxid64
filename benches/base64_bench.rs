#![allow(warnings)]
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxid64::engine::DecodeOpts;
use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use oxid64::engine::ssse3::Ssse3Decoder;
use std::sync::Once;

const TURBO_STYLE_SIZES: [usize; 2] = [10_000, 1_000_000];

// ---------------------------------------------------------------------------
// CPU feature detection
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn has_ssse3() -> bool {
    std::arch::is_x86_feature_detected!("ssse3")
}
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn has_ssse3() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn has_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn has_avx2() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn has_avx512vbmi() -> bool {
    std::arch::is_x86_feature_detected!("avx512f")
        && std::arch::is_x86_feature_detected!("avx512bw")
        && std::arch::is_x86_feature_detected!("avx512vbmi")
}
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn has_avx512vbmi() -> bool {
    false
}

fn yn(b: bool) -> &'static str {
    if b { "YES" } else { "no" }
}

/// Print a one-time banner showing which ISAs are available on this CPU.
fn print_cpu_banner() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let ssse3 = has_ssse3();
        let avx2 = has_avx2();
        let avx512 = has_avx512vbmi();
        eprintln!();
        eprintln!("  +-----------------------------------------+");
        eprintln!("  |  oxid64 benchmark — CPU feature probe   |");
        eprintln!("  +-----------------------------------------+");
        eprintln!("  |  SSSE3       : {:<26}|", yn(ssse3));
        eprintln!("  |  AVX2        : {:<26}|", yn(avx2));
        eprintln!("  |  AVX-512 VBMI: {:<26}|", yn(avx512));
        eprintln!("  +-----------------------------------------+");
        if !avx512 {
            eprintln!("  |  NOTE: AVX-512 benchmarks will be       |");
            eprintln!("  |        skipped on this CPU.              |");
            eprintln!("  +-----------------------------------------+");
        }
        eprintln!();
    });
}

// ---------------------------------------------------------------------------
// Turbo-Base64 C FFI declarations
// ---------------------------------------------------------------------------

unsafe extern "C" {
    // Scalar decode
    fn tb64sdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    // SIMD decode — checked (B64CHECK)
    fn tb64v128dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    // SIMD decode — default (CHECK0: first vector per DS64 validated)
    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    // SIMD decode — unchecked (NB64CHECK)
    fn tb64v128dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    // Scalar encode
    fn tb64senc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xenc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    // SIMD encode
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    // Lemire fastbase64
    fn fast_avx2_base64_decode(out: *mut i8, src: *const i8, srclen: usize) -> usize;
    fn fast_avx2_base64_encode(dest: *mut i8, str_: *const i8, len: usize) -> usize;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
    let dec_len = decoded_len_strict(&encoded).unwrap();
    (encoded, dec_len)
}

// ===========================================================================
// Decode — Checked / Strict
// ===========================================================================

pub fn bench_decode_checked(c: &mut Criterion) {
    print_cpu_banner();
    let mut group = c.benchmark_group("Decode (Checked)");

    for size in TURBO_STYLE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        let input = make_input(size);
        let (encoded, decoded_len) = make_encoded(&input);
        let mut output = vec![0u8; decoded_len + 64];

        // -- oxid64 --------------------------------------------------------

        group.bench_with_input(
            BenchmarkId::new("oxid64 scalar strict", size),
            &encoded,
            |b, i| {
                b.iter(|| {
                    let _ = decode_base64_fast(
                        black_box(i.as_slice()),
                        black_box(output.as_mut_slice()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("oxid64 ssse3 strict", size),
            &encoded,
            |b, i| {
                let dec = Ssse3Decoder::new();
                b.iter(|| {
                    let _ = dec
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("oxid64 avx2 strict", size),
            &encoded,
            |b, i| {
                let dec = Avx2Decoder::new();
                b.iter(|| {
                    let _ = dec
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        if has_avx512vbmi() {
            group.bench_with_input(
                BenchmarkId::new("oxid64 avx512 strict", size),
                &encoded,
                |b, i| {
                    let dec = Avx512VbmiDecoder::new();
                    b.iter(|| {
                        let _ = dec.decode_to_slice(
                            black_box(i.as_slice()),
                            black_box(output.as_mut_slice()),
                        );
                    });
                },
            );
        }

        // -- tb64 ----------------------------------------------------------

        group.bench_with_input(
            BenchmarkId::new("tb64 scalar (mem)", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64sdec(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tb64 scalar (fast)", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64xdec(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tb64 ssse3 check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v128dec_b64check(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tb64 avx2 check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v256dec_b64check(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        if has_avx512vbmi() {
            group.bench_with_input(
                BenchmarkId::new("tb64 avx512 check", size),
                &encoded,
                |b, i| {
                    b.iter(|| unsafe {
                        tb64v512dec_b64check(
                            black_box(i.as_ptr()),
                            black_box(i.len()),
                            black_box(output.as_mut_ptr()),
                        );
                    });
                },
            );
        }

        // -- fastbase64 ----------------------------------------------------

        group.bench_with_input(
            BenchmarkId::new("fastbase64 avx2 check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    fast_avx2_base64_decode(
                        black_box(output.as_mut_ptr().cast::<i8>()),
                        black_box(i.as_ptr().cast::<i8>()),
                        black_box(i.len()),
                    );
                });
            },
        );
    }

    group.finish();
}

// ===========================================================================
// Decode — Unchecked / No-Check
// ===========================================================================

pub fn bench_decode_unchecked(c: &mut Criterion) {
    print_cpu_banner();
    let mut group = c.benchmark_group("Decode (No Check)");

    for size in TURBO_STYLE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        let input = make_input(size);
        let (encoded, decoded_len) = make_encoded(&input);
        let mut output = vec![0u8; decoded_len + 64];

        // -- oxid64 --------------------------------------------------------

        group.bench_with_input(
            BenchmarkId::new("oxid64 ssse3 non-strict", size),
            &encoded,
            |b, i| {
                let dec = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
                b.iter(|| {
                    let _ = dec
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("oxid64 avx2 non-strict", size),
            &encoded,
            |b, i| {
                let dec = Avx2Decoder::with_opts(DecodeOpts { strict: false });
                b.iter(|| {
                    let _ = dec
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("oxid64 avx2 unchecked", size),
            &encoded,
            |b, i| {
                let dec = Avx2Decoder::new();
                b.iter(|| {
                    let _ = dec.decode_to_slice_unchecked(
                        black_box(i.as_slice()),
                        black_box(output.as_mut_slice()),
                    );
                });
            },
        );

        if has_avx512vbmi() {
            group.bench_with_input(
                BenchmarkId::new("oxid64 avx512 non-strict", size),
                &encoded,
                |b, i| {
                    let dec = Avx512VbmiDecoder::with_opts(DecodeOpts { strict: false });
                    b.iter(|| {
                        let _ = dec.decode_to_slice(
                            black_box(i.as_slice()),
                            black_box(output.as_mut_slice()),
                        );
                    });
                },
            );
        }

        // -- tb64 (CHECK0 / default — partial check, matches oxid64 non-strict)

        group.bench_with_input(
            BenchmarkId::new("tb64 ssse3 partial-check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v128dec(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tb64 avx2 partial-check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v256dec(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        if has_avx512vbmi() {
            group.bench_with_input(
                BenchmarkId::new("tb64 avx512 partial-check", size),
                &encoded,
                |b, i| {
                    b.iter(|| unsafe {
                        tb64v512dec(
                            black_box(i.as_ptr()),
                            black_box(i.len()),
                            black_box(output.as_mut_ptr()),
                        );
                    });
                },
            );
        }

        // -- tb64 (NB64CHECK — fully unchecked) ----------------------------

        group.bench_with_input(
            BenchmarkId::new("tb64 ssse3 no-check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v128dec_nb64check(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tb64 avx2 no-check", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v256dec_nb64check(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        if has_avx512vbmi() {
            group.bench_with_input(
                BenchmarkId::new("tb64 avx512 no-check", size),
                &encoded,
                |b, i| {
                    b.iter(|| unsafe {
                        tb64v512dec_nb64check(
                            black_box(i.as_ptr()),
                            black_box(i.len()),
                            black_box(output.as_mut_ptr()),
                        );
                    });
                },
            );
        }
    }

    group.finish();
}

// ===========================================================================
// Encode
// ===========================================================================

pub fn bench_encode(c: &mut Criterion) {
    print_cpu_banner();
    let mut group = c.benchmark_group("Encode");

    for size in TURBO_STYLE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        let input = make_input(size);
        let out_len = input.len().div_ceil(3) * 4 + 64;
        let mut output = vec![0u8; out_len];

        // -- oxid64 --------------------------------------------------------

        group.bench_with_input(BenchmarkId::new("oxid64 scalar", size), &input, |b, i| {
            b.iter(|| {
                let _ =
                    encode_base64_fast(black_box(i.as_slice()), black_box(output.as_mut_slice()));
            });
        });

        group.bench_with_input(BenchmarkId::new("oxid64 ssse3", size), &input, |b, i| {
            let dec = Ssse3Decoder::new();
            b.iter(|| {
                let _ =
                    dec.encode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
            });
        });

        group.bench_with_input(BenchmarkId::new("oxid64 avx2", size), &input, |b, i| {
            let dec = Avx2Decoder::new();
            b.iter(|| {
                let _ =
                    dec.encode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
            });
        });

        if has_avx512vbmi() {
            group.bench_with_input(BenchmarkId::new("oxid64 avx512", size), &input, |b, i| {
                let dec = Avx512VbmiDecoder::new();
                b.iter(|| {
                    let _ = dec
                        .encode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            });
        }

        // -- tb64 ----------------------------------------------------------

        group.bench_with_input(
            BenchmarkId::new("tb64 scalar (mem)", size),
            &input,
            |b, i| {
                b.iter(|| unsafe {
                    tb64senc(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("tb64 scalar (fast)", size),
            &input,
            |b, i| {
                b.iter(|| unsafe {
                    tb64xenc(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("tb64 ssse3", size), &input, |b, i| {
            b.iter(|| unsafe {
                tb64v128enc(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("tb64 avx2", size), &input, |b, i| {
            b.iter(|| unsafe {
                tb64v256enc(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        if has_avx512vbmi() {
            group.bench_with_input(BenchmarkId::new("tb64 avx512", size), &input, |b, i| {
                b.iter(|| unsafe {
                    tb64v512enc(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            });
        }

        // -- fastbase64 ----------------------------------------------------

        group.bench_with_input(BenchmarkId::new("fastbase64 avx2", size), &input, |b, i| {
            b.iter(|| unsafe {
                fast_avx2_base64_encode(
                    black_box(output.as_mut_ptr().cast::<i8>()),
                    black_box(i.as_ptr().cast::<i8>()),
                    black_box(i.len()),
                );
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_decode_checked,
    bench_decode_unchecked,
    bench_encode
);
criterion_main!(benches);
