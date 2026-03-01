#![allow(warnings)]
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxid64::engine::DecodeOpts;
use oxid64::engine::avx2::Avx2Decoder;
use oxid64::engine::avx512vbmi::Avx512VbmiDecoder;
use oxid64::engine::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use oxid64::engine::ssse3::Ssse3Decoder;

unsafe extern "C" {
    fn tb64sdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_b64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256dec_nb64check(in_: *const u8, inlen: usize, out: *mut u8) -> usize;

    fn tb64senc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xenc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64v256enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

pub fn bench_base64_decode_strict_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base64 Decoding Strict Compare");

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
            BenchmarkId::new("Rust Port (AVX2 Strict)", size),
            &encoded,
            |b, i| {
                let decoder = Avx2Decoder::new();
                b.iter(|| {
                    let _ = decoder
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Rust Port (AVX-512 VBMI Strict)", size),
            &encoded,
            |b, i| {
                let decoder = Avx512VbmiDecoder::new();
                b.iter(|| {
                    let _ = decoder
                        .decode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("TurboBase64 C (AVX2, check)", size),
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
            BenchmarkId::new("TurboBase64 C (AVX2, default)", size),
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

    }
    group.finish();
}

pub fn bench_base64_encode_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("Base64 Encoding Compare");
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
                    let _ = Ssse3Decoder::new()
                        .encode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
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
                    let _ = Avx2Decoder::new()
                        .encode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Rust Port (AVX-512 VBMI)", size),
            &input,
            |b, i| {
                b.iter(|| {
                    let _ = Avx512VbmiDecoder::new()
                        .encode_to_slice(black_box(i.as_slice()), black_box(output.as_mut_slice()));
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

criterion_group!(
    benches,
    bench_base64_decode_strict_compare,
    bench_base64_encode_compare
);
criterion_main!(benches);
