use oxid64::simd::scalar::encode_base64_fast;
use oxid64::simd::ssse3::Ssse3Decoder;
use oxid64::simd::Base64Decoder;
use std::hint::black_box;
use std::time::Instant;

unsafe extern "C" {
    fn tb64v128enc(in_: *const u8, inlen: usize, out: *mut u8) -> usize;
}

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
    let engine = args.get(1).map(|s| s.as_str()).unwrap_or("scalar");
    let input_size = parse_arg(&args, 2, 1024 * 1024);
    let iterations = parse_arg(&args, 3, 20_000);

    let mut input = vec![0u8; input_size];
    xorshift_fill(&mut input);

    let mut output = vec![0u8; encoded_len(input_size) + 64];
    let mut checksum = 0u64;

    let sse_decoder = Ssse3Decoder::new();

    let start = Instant::now();
    match engine {
        "scalar" => {
            for _ in 0..iterations {
                let n = encode_base64_fast(black_box(&input), black_box(&mut output));
                checksum ^= output[0] as u64;
                checksum ^= output[n - 1] as u64;
            }
        }
        "ssse3" => {
            for _ in 0..iterations {
                let out = sse_decoder.encode(black_box(&input));
                let out = black_box(out);
                checksum ^= out[0] as u64;
                checksum ^= out[out.len() - 1] as u64;
            }
        }
        "c_sse" => {
            for _ in 0..iterations {
                let n = unsafe {
                    tb64v128enc(
                        black_box(input.as_ptr()),
                        black_box(input.len()),
                        black_box(output.as_mut_ptr()),
                    )
                };
                checksum ^= output[0] as u64;
                checksum ^= output[n - 1] as u64;
            }
        }
        _ => panic!("Unknown engine"),
    }
    let elapsed = start.elapsed();

    let bytes_total = input_size as f64 * iterations as f64;
    let gib_per_s = bytes_total / elapsed.as_secs_f64() / (1024.0 * 1024.0 * 1024.0);

    println!("engine={engine} input_size={input_size} iterations={iterations}");
    println!(
        "elapsed={:.3}s throughput={:.3} GiB/s",
        elapsed.as_secs_f64(),
        gib_per_s
    );
    println!("checksum={checksum}");
}
