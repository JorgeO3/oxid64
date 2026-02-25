use oxid64::scalar::encode_base64_fast;
use std::hint::black_box;
use std::time::Instant;

fn encoded_len(n: usize) -> usize {
    ((n + 2) / 3) * 4
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
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_size = parse_arg(&args, 1, 1024 * 1024);
    let iterations = parse_arg(&args, 2, 20_000);

    let mut input = vec![0u8; input_size];
    xorshift_fill(&mut input);

    let mut output = vec![0u8; encoded_len(input_size) + 64];
    let mut checksum = 0u64;

    for _ in 0..256 {
        let n = encode_base64_fast(black_box(&input), black_box(&mut output));
        checksum ^= output[0] as u64;
        checksum ^= output[n - 1] as u64;
    }

    let start = Instant::now();
    for _ in 0..iterations {
        let n = encode_base64_fast(black_box(&input), black_box(&mut output));
        checksum ^= output[0] as u64;
        checksum ^= output[n - 1] as u64;
    }
    let elapsed = start.elapsed();

    let bytes_total = input_size as f64 * iterations as f64;
    let gib_per_s = bytes_total / elapsed.as_secs_f64() / (1024.0 * 1024.0 * 1024.0);

    println!("input_size={input_size} iterations={iterations}");
    println!(
        "elapsed={:.3}s throughput={:.3} GiB/s",
        elapsed.as_secs_f64(),
        gib_per_s
    );
    println!("checksum={checksum}");
}
