use oxid64::engine::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use std::hint::black_box;
use std::time::Instant;

fn encoded_len(source_len: usize) -> usize {
    ((source_len + 2) / 3) * 4
}

fn xorshift_fill(buf: &mut [u8]) {
    let mut x = 0x1234_5678_9abc_def0_u64;
    for b in buf {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = x as u8;
    }
}

fn parse_arg(args: &[String], index: usize, default: usize) -> usize {
    args.get(index)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn checksum_edge(out: &[u8], len: usize, checksum: &mut u64) {
    if len != 0 {
        *checksum ^= out[0] as u64;
        *checksum ^= out[len - 1] as u64;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let decoded_size = parse_arg(&args, 1, 1024 * 1024);
    let iterations = parse_arg(&args, 2, 40_000);
    let warmup_runs = parse_arg(&args, 3, 256);

    let mut source = vec![0u8; decoded_size];
    xorshift_fill(&mut source);

    let mut encoded_buf = vec![0u8; encoded_len(decoded_size) + 64];
    let encoded_len = encode_base64_fast(&source, &mut encoded_buf);
    let encoded = &encoded_buf[..encoded_len];

    let expected_len = decoded_len_strict(encoded).expect("generated input must be valid");
    let mut output = vec![0u8; expected_len + 64];
    let mut checksum = 0u64;

    for _ in 0..warmup_runs {
        let n = decode_base64_fast(black_box(encoded), black_box(&mut output)).unwrap_or(0);
        if n != expected_len {
            eprintln!("decode failed during warmup: produced {n} bytes, expected {expected_len}");
            std::process::exit(1);
        }
        checksum_edge(&output, n, &mut checksum);
    }

    let start = Instant::now();
    for _ in 0..iterations {
        let n = decode_base64_fast(black_box(encoded), black_box(&mut output)).unwrap_or(0);
        if n != expected_len {
            eprintln!(
                "decode failed during measurement: produced {n} bytes, expected {expected_len}"
            );
            std::process::exit(1);
        }
        checksum_edge(&output, n, &mut checksum);
    }
    let elapsed = start.elapsed();

    let bytes_total = expected_len as f64 * iterations as f64;
    let gib_per_s = bytes_total / elapsed.as_secs_f64() / (1024.0 * 1024.0 * 1024.0);

    println!(
        "decoded_size={expected_len} encoded_size={} iterations={iterations}",
        encoded.len()
    );
    println!(
        "elapsed={:.3}s throughput={:.3} GiB/s",
        elapsed.as_secs_f64(),
        gib_per_s
    );
    println!("checksum={checksum}");
}
