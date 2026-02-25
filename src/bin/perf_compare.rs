use oxid64::scalar::{decode_base64_fast, decoded_len_strict, encode_base64_fast};
use std::hint::black_box;
use std::time::Instant;

unsafe extern "C" {
    fn tb64xenc(in_data: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64enc(in_data: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64xdec(in_data: *const u8, inlen: usize, out: *mut u8) -> usize;
    fn tb64dec(in_data: *const u8, inlen: usize, out: *mut u8) -> usize;
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

fn parse_mode(args: &[String]) -> &str {
    args.get(1).map(String::as_str).unwrap_or("rust")
}

#[inline(always)]
fn checksum_edge(out: &[u8], n: usize, checksum: &mut u64) {
    if n != 0 {
        *checksum ^= out[0] as u64;
        *checksum ^= out[n - 1] as u64;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = parse_mode(&args);
    let input_size = parse_arg(&args, 2, 1024 * 1024);
    let iterations = parse_arg(&args, 3, 20_000);

    match mode {
        "rust" | "cfast" | "cauto" => {
            let mut input = vec![0u8; input_size];
            xorshift_fill(&mut input);

            let mut output = vec![0u8; encoded_len(input_size) + 64];
            let mut checksum = 0u64;

            for _ in 0..256 {
                let n = match mode {
                    "rust" => encode_base64_fast(black_box(&input), black_box(&mut output)),
                    "cfast" => unsafe {
                        tb64xenc(
                            black_box(input.as_ptr()),
                            black_box(input.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    "cauto" => unsafe {
                        tb64enc(
                            black_box(input.as_ptr()),
                            black_box(input.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    _ => unreachable!(),
                };
                checksum_edge(&output, n, &mut checksum);
            }

            let start = Instant::now();
            for _ in 0..iterations {
                let n = match mode {
                    "rust" => encode_base64_fast(black_box(&input), black_box(&mut output)),
                    "cfast" => unsafe {
                        tb64xenc(
                            black_box(input.as_ptr()),
                            black_box(input.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    "cauto" => unsafe {
                        tb64enc(
                            black_box(input.as_ptr()),
                            black_box(input.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    _ => unreachable!(),
                };
                checksum_edge(&output, n, &mut checksum);
            }
            let elapsed = start.elapsed();

            let bytes_total = input_size as f64 * iterations as f64;
            let gib_per_s = bytes_total / elapsed.as_secs_f64() / (1024.0 * 1024.0 * 1024.0);

            println!("mode={mode} input_size={input_size} iterations={iterations}");
            println!(
                "elapsed={:.3}s throughput={:.3} GiB/s",
                elapsed.as_secs_f64(),
                gib_per_s
            );
            println!("checksum={checksum}");
        }
        "rustdec" | "cfastdec" | "cautodec" => {
            // Prepare valid base64 input once from pseudo-random source bytes.
            let mut source = vec![0u8; input_size];
            xorshift_fill(&mut source);

            let mut encoded_buf = vec![0u8; encoded_len(input_size) + 64];
            let encoded_len = encode_base64_fast(&source, &mut encoded_buf);
            let encoded = &encoded_buf[..encoded_len];

            let expected_len =
                decoded_len_strict(encoded).expect("internal encoded input must be valid");
            let mut output = vec![0u8; expected_len + 64];
            let mut checksum = 0u64;

            for _ in 0..256 {
                let n = match mode {
                    "rustdec" => {
                        decode_base64_fast(black_box(encoded), black_box(&mut output)).unwrap_or(0)
                    }
                    "cfastdec" => unsafe {
                        tb64xdec(
                            black_box(encoded.as_ptr()),
                            black_box(encoded.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    "cautodec" => unsafe {
                        tb64dec(
                            black_box(encoded.as_ptr()),
                            black_box(encoded.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    _ => unreachable!(),
                };
                if n != expected_len {
                    eprintln!("decode failed: produced {n} bytes, expected {expected_len}");
                    std::process::exit(1);
                }
                checksum_edge(&output, n, &mut checksum);
            }

            let start = Instant::now();
            for _ in 0..iterations {
                let n = match mode {
                    "rustdec" => {
                        decode_base64_fast(black_box(encoded), black_box(&mut output)).unwrap_or(0)
                    }
                    "cfastdec" => unsafe {
                        tb64xdec(
                            black_box(encoded.as_ptr()),
                            black_box(encoded.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    "cautodec" => unsafe {
                        tb64dec(
                            black_box(encoded.as_ptr()),
                            black_box(encoded.len()),
                            black_box(output.as_mut_ptr()),
                        )
                    },
                    _ => unreachable!(),
                };
                if n != expected_len {
                    eprintln!("decode failed: produced {n} bytes, expected {expected_len}");
                    std::process::exit(1);
                }
                checksum_edge(&output, n, &mut checksum);
            }
            let elapsed = start.elapsed();

            let bytes_total = expected_len as f64 * iterations as f64;
            let gib_per_s = bytes_total / elapsed.as_secs_f64() / (1024.0 * 1024.0 * 1024.0);

            println!(
                "mode={mode} decoded_size={expected_len} encoded_size={} iterations={iterations}",
                encoded.len()
            );
            println!(
                "elapsed={:.3}s throughput={:.3} GiB/s",
                elapsed.as_secs_f64(),
                gib_per_s
            );
            println!("checksum={checksum}");
        }
        _ => {
            eprintln!("mode must be one of: rust, cfast, cauto, rustdec, cfastdec, cautodec");
            std::process::exit(2);
        }
    }
}
