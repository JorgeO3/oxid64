#![allow(warnings)]
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxid64::simd::Base64Decoder;
use oxid64::simd::avx2::Avx2Decoder;
use oxid64::simd::scalar::{decode_base64_fast, encode_base64_fast};
use oxid64::simd::sse42::Sse42Decoder;

unsafe extern "C" {
    fn tb64sdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64senc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xenc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

pub fn bench_base64_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base64 Decoding");
    let sse_decoder = Sse42Decoder;

    for size in [1024, 1024 * 1024].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        let mut input = vec![0u8; *size];
        for (i, b) in input.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let mut encoded = vec![0u8; ((size + 2) / 3) * 4 + 64];
        let encoded_len = encode_base64_fast(&input, &mut encoded);
        encoded.truncate(encoded_len);

        let mut output = vec![0u8; *size + 64];

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
                b.iter(|| {
                    let _ = sse_decoder.decode(black_box(i.as_slice()));
                });
            },
        );

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
    }
    group.finish();
}

pub fn bench_base64_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base64 Encoding");
    let sse_encoder = Sse42Decoder;

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
                    let _ = Sse42Decoder::encode_to_slice(
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
