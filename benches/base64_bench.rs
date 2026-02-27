#![allow(warnings)]
use base64_turbo::STANDARD as BASE64_TURBO_STANDARD;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxid64::simd::avx2::Avx2Decoder;
use oxid64::simd::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use oxid64::simd::ssse3::{DecodeOpts, Ssse3Decoder};

unsafe extern "C" {
    fn tb64sdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64senc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xenc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

pub fn bench_base64_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base64 Decoding");

    for size in [1024, 1024 * 1024].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        let mut input = vec![0u8; *size];
        for (i, b) in input.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let mut encoded = vec![0u8; ((size + 2) / 3) * 4 + 64];
        let encoded_len = encode_base64_fast(&input, &mut encoded);
        encoded.truncate(encoded_len);
        let decoded_len = decoded_len_strict(&encoded).unwrap();

        let mut output = vec![0u8; decoded_len + 64];

        group.bench_with_input(
            BenchmarkId::new("Rust Port (Safe Scalar)", size),
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
            BenchmarkId::new("Rust Port (SSSE3)", size),
            &encoded,
            |b, i| {
                let decoder = Ssse3Decoder::with_opts(DecodeOpts { strict: false });
                b.iter(|| {
                    let _ = decoder
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Rust Port (SSSE3 Strict)", size),
            &encoded,
            |b, i| {
                let decoder = Ssse3Decoder::new();
                b.iter(|| {
                    let _ = decoder
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("base64-turbo (decode_into)", size),
            &encoded,
            |b, i| {
                b.iter(|| {
                    let _ = BASE64_TURBO_STANDARD
                        .decode_into(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("base64-turbo (unsafe decode_scalar)", size),
            &encoded,
            |b, i| {
                b.iter(|| unsafe {
                    let _ = BASE64_TURBO_STANDARD
                        .decode_scalar(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if std::arch::is_x86_feature_detected!("sse4.1") {
            group.bench_with_input(
                BenchmarkId::new("base64-turbo (unsafe decode_sse4)", size),
                &encoded,
                |b, i| {
                    b.iter(|| unsafe {
                        let _ = BASE64_TURBO_STANDARD
                            .decode_sse4(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                    });
                },
            );
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if std::arch::is_x86_feature_detected!("avx2") {
            group.bench_with_input(
                BenchmarkId::new("base64-turbo (unsafe decode_avx2)", size),
                &encoded,
                |b, i| {
                    b.iter(|| unsafe {
                        let _ = BASE64_TURBO_STANDARD
                            .decode_avx2(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                    });
                },
            );
        }

        group.bench_with_input(
            BenchmarkId::new("TurboBase64 C (Fast Scalar)", size),
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
            BenchmarkId::new("TurboBase64 C (Extreme Fast Scalar)", size),
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
            BenchmarkId::new("TurboBase64 C (SSE)", size),
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
            BenchmarkId::new("TurboBase64 C (SSE, B64CHECK)", size),
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
            BenchmarkId::new("TurboBase64 C (SSE, NB64CHECK)", size),
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
    }
    group.finish();
}

pub fn bench_base64_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base64 Encoding");
    let sse_encoder = Ssse3Decoder::new();

    for size in [1024, 1024 * 1024].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        let mut input = vec![0u8; *size];
        for (i, b) in input.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let out_len = ((*size + 2) / 3) * 4 + 64;
        let mut output = vec![0u8; out_len];

        group.bench_with_input(
            BenchmarkId::new("Rust Port (Safe Scalar)", size),
            &input,
            |b, i| {
                b.iter(|| {
                    let _ = encode_base64_fast(
                        black_box(i.as_slice()),
                        black_box(output.as_mut_slice()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Rust Port (SSSE3)", size),
            &input,
            |b, i| {
                b.iter(|| {
                    let _ = Ssse3Decoder::encode_to_slice(
                        black_box(i.as_slice()),
                        black_box(output.as_mut_slice()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("TurboBase64 C (Fast Scalar)", size),
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
            BenchmarkId::new("TurboBase64 C (Extreme Fast Scalar)", size),
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

        group.bench_with_input(
            BenchmarkId::new("TurboBase64 C (SSE)", size),
            &input,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v128enc(
                        black_box(i.as_ptr()),
                        black_box(i.len()),
                        black_box(output.as_mut_ptr()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Rust Port (AVX2)", size),
            &input,
            |b, i| {
                b.iter(|| {
                    let _ = Avx2Decoder::encode_to_slice(
                        black_box(i.as_slice()),
                        black_box(output.as_mut_slice()),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("TurboBase64 C (AVX2)", size),
            &input,
            |b, i| {
                b.iter(|| unsafe {
                    tb64v256enc(
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

criterion_group!(benches, bench_base64_decode, bench_base64_encode);
criterion_main!(benches);
