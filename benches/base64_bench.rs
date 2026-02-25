#![allow(warnings)]
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxid64::simd::scalar::{decode_base64_fast, encode_base64_fast};
// use oxid64::scalar_unsafe::decode_extreme_unsafe;

unsafe extern "C" {
    fn tb64sdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

pub fn bench_base64(c: &mut Criterion) {
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

        let mut output = vec![0u8; *size + 64]; // Need padding for overlapping writes

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

        // group.bench_with_input(
        //     BenchmarkId::new("Rust Port (Extreme Unsafe)", size),
        //     &encoded,
        //     |b, i| {
        //         b.iter(|| unsafe {
        //             let _ = decode_extreme_unsafe(
        //                 black_box(i.as_slice()),
        //                 black_box(output.as_mut_slice()),
        //             );
        //         });
        //     },
        // );

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
    }
    group.finish();
}

criterion_group!(benches, bench_base64);
criterion_main!(benches);
