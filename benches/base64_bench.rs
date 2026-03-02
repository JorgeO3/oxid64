#![allow(warnings)]
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxid64::engine::DecodeOpts;
use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use oxid64::engine::ssse3::Ssse3Decoder;

const TURBO_STYLE_SIZES: [usize; 2] = [10_000, 1_000_000];

#[inline]
fn has_avx512vbmi() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::arch::is_x86_feature_detected!("avx512vbmi")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

unsafe extern "C" {
    fn tb64sdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64senc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xenc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v512enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn fast_avx2_base64_decode(out: *mut i8, src: *const i8, srclen: usize) -> usize;
    fn fast_avx2_base64_encode(dest: *mut i8, str_: *const i8, len: usize) -> usize;
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
    let dec_len = decoded_len_strict(&encoded).unwrap();
    (encoded, dec_len)
}

pub fn bench_turbo_style_decode_checked(c: &mut Criterion) {
    let mut group = c.benchmark_group("Turbo-Style Decode (Checked)");

    for size in TURBO_STYLE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        let input = make_input(size);
        let (encoded, decoded_len) = make_encoded(&input);
        let mut output = vec![0u8; decoded_len + 64];

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
                BenchmarkId::new("oxid64 avx512vbmi strict", size),
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

        group.bench_with_input(BenchmarkId::new("tb64s scalar", size), &encoded, |b, i| {
            b.iter(|| unsafe {
                tb64sdec(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("tb64x scalar", size), &encoded, |b, i| {
            b.iter(|| unsafe {
                tb64xdec(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        group.bench_with_input(
            BenchmarkId::new("tb64v128 check", size),
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
            BenchmarkId::new("tb64v256 check", size),
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

        group.bench_with_input(
            BenchmarkId::new("fastbase64 avx2 validating", size),
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

pub fn bench_turbo_style_decode_unchecked(c: &mut Criterion) {
    let mut group = c.benchmark_group("Turbo-Style Decode (No Check)");

    for size in TURBO_STYLE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        let input = make_input(size);
        let (encoded, decoded_len) = make_encoded(&input);
        let mut output = vec![0u8; decoded_len + 64];

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

        group.bench_with_input(
            BenchmarkId::new("tb64v128 no-check", size),
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
            BenchmarkId::new("tb64v256 no-check", size),
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
    }

    group.finish();
}

pub fn bench_turbo_style_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("Turbo-Style Encode");

    for size in TURBO_STYLE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        let input = make_input(size);
        let out_len = input.len().div_ceil(3) * 4 + 64;
        let mut output = vec![0u8; out_len];

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
            group.bench_with_input(
                BenchmarkId::new("oxid64 avx512vbmi", size),
                &input,
                |b, i| {
                    let dec = Avx512VbmiDecoder::new();
                    b.iter(|| {
                        let _ = dec.encode_to_slice(
                            black_box(i.as_slice()),
                            black_box(output.as_mut_slice()),
                        );
                    });
                },
            );
        }

        group.bench_with_input(BenchmarkId::new("tb64s scalar", size), &input, |b, i| {
            b.iter(|| unsafe {
                tb64senc(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("tb64x scalar", size), &input, |b, i| {
            b.iter(|| unsafe {
                tb64xenc(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("tb64v128", size), &input, |b, i| {
            b.iter(|| unsafe {
                tb64v128enc(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        group.bench_with_input(BenchmarkId::new("tb64v256", size), &input, |b, i| {
            b.iter(|| unsafe {
                tb64v256enc(
                    black_box(i.as_ptr()),
                    black_box(i.len()),
                    black_box(output.as_mut_ptr()),
                );
            });
        });

        if has_avx512vbmi() {
            group.bench_with_input(BenchmarkId::new("tb64v512", size), &input, |b, i| {
                b.iter(|| unsafe {
                    tb64v512enc(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            });
        }

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
    bench_turbo_style_decode_checked,
    bench_turbo_style_decode_unchecked,
    bench_turbo_style_encode
);
criterion_main!(benches);
