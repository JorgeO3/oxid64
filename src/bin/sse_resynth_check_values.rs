#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use std::env;

#[derive(Clone, Copy, Debug)]
enum MixOp {
    Avg,
    Add,
    Xor,
}

impl MixOp {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "avg" | "avg_epu8" => Some(Self::Avg),
            "add" | "add_epi8" => Some(Self::Add),
            "xor" | "xor_si128" => Some(Self::Xor),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct HashSpec {
    shift: u8,
    mix: MixOp,
}

#[inline]
fn avg_epu8(a: u8, b: u8) -> u8 {
    (((a as u16) + (b as u16) + 1) >> 1) as u8
}

#[inline]
fn sat_add_i8(a: i8, b: i8) -> i8 {
    let sum = (a as i16) + (b as i16);
    if sum > i8::MAX as i16 {
        i8::MAX
    } else if sum < i8::MIN as i16 {
        i8::MIN
    } else {
        sum as i8
    }
}

#[inline]
fn is_base64_valid_strict_simd(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'
    )
}

#[inline]
fn hash_nibble(byte: u8, next_low_k: u8, pos_in_dword: u8, table: &[u8; 16], spec: HashSpec) -> u8 {
    debug_assert!(pos_in_dword < 4);
    let k = spec.shift;

    let base = table[(byte & 0x0f) as usize];
    let shifted = if pos_in_dword == 3 {
        byte >> k
    } else {
        let mask = ((1u16 << k) - 1) as u8;
        (byte >> k) | ((next_low_k & mask) << (8 - k))
    };

    let mixed = match spec.mix {
        MixOp::Avg => avg_epu8(base, shifted),
        MixOp::Add => base.wrapping_add(shifted),
        MixOp::Xor => base ^ shifted,
    };
    mixed & 0x0f
}

fn parse_table(arg: &str) -> Result<[u8; 16], String> {
    let parts: Vec<&str> = arg
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() != 16 {
        return Err(format!(
            "expected exactly 16 table bytes, got {}",
            parts.len()
        ));
    }

    let mut out = [0u8; 16];
    for (i, p) in parts.iter().enumerate() {
        let p = p.trim_start_matches("0x").trim_start_matches("0X");
        out[i] = u8::from_str_radix(p, 16)
            .map_err(|e| format!("invalid hex byte at index {i}: {p} ({e})"))?;
    }
    Ok(out)
}

fn build_buckets(table: &[u8; 16], spec: HashSpec) -> [[bool; 256]; 16] {
    let mut buckets = [[false; 256]; 16];

    for byte in u8::MIN..=u8::MAX {
        for pos in 0..4u8 {
            if pos == 3 {
                let h = hash_nibble(byte, 0, pos, table, spec);
                buckets[h as usize][byte as usize] = true;
            } else {
                let contexts = 1u16 << spec.shift;
                for next_low in 0..contexts {
                    let h = hash_nibble(byte, next_low as u8, pos, table, spec);
                    buckets[h as usize][byte as usize] = true;
                }
            }
        }
    }

    buckets
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!(
            "Usage: cargo run --bin sse_resynth_check_values -- <mix:avg|add|xor> <shift:2..5> \"<16 hex bytes>\""
        );
        eprintln!(
            "Example: cargo run --bin sse_resynth_check_values -- add 4 \"4a 7a 6a 8a 3a 7a 9a 4a 7a 2a f9 5b cd ad 1d bb\""
        );
        std::process::exit(2);
    }

    let mix = match MixOp::parse(&args[1]) {
        Some(v) => v,
        None => {
            eprintln!("invalid mix '{}', use avg|add|xor", args[1]);
            std::process::exit(2);
        }
    };

    let shift = match args[2].parse::<u8>() {
        Ok(v) if (1..=7).contains(&v) => v,
        _ => {
            eprintln!("invalid shift '{}', expected 1..=7", args[2]);
            std::process::exit(2);
        }
    };

    let table = match parse_table(&args[3]) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("table parse error: {e}");
            std::process::exit(2);
        }
    };

    let spec = HashSpec { shift, mix };
    let buckets = build_buckets(&table, spec);

    let mut check_values = [0i8; 16];
    let mut impossible = Vec::new();

    println!("Synthesize check_values for sat-add sign model");
    println!("spec: mix={:?}, shift={}", spec.mix, spec.shift);
    println!();

    for (h, bucket) in buckets.iter().enumerate() {
        let mut seen_any = false;
        let mut valid_count = 0u32;
        let mut invalid_count = 0u32;

        let mut lo = -128i16;
        let mut hi = 127i16;

        for byte in 0u16..=255u16 {
            if !bucket[byte as usize] {
                continue;
            }
            seen_any = true;
            let b = byte as u8;
            let bi = (b as i8) as i16;

            if is_base64_valid_strict_simd(b) {
                valid_count += 1;
                let lower = -bi;
                if lower > lo {
                    lo = lower;
                }
            } else {
                invalid_count += 1;
                let upper = -bi - 1;
                if upper < hi {
                    hi = upper;
                }
            }
        }

        if !seen_any {
            println!("bucket {h:2}: empty");
            check_values[h] = 0;
            continue;
        }

        if lo > hi {
            println!(
                "bucket {h:2}: valid={} invalid={} FEASIBLE=NO",
                valid_count, invalid_count
            );
            impossible.push(h);
            continue;
        }

        let chosen = ((lo + hi) / 2).clamp(-128, 127) as i8;
        check_values[h] = chosen;

        // Quick sanity for this chosen t.
        let mut violations = 0u32;
        for byte in 0u16..=255u16 {
            if !bucket[byte as usize] {
                continue;
            }
            let b = byte as u8;
            let y = sat_add_i8(chosen, b as i8);
            let valid = is_base64_valid_strict_simd(b);
            if valid {
                if y < 0 {
                    violations += 1;
                }
            } else if y >= 0 {
                violations += 1;
            }
        }

        println!(
            "bucket {h:2}: valid={} invalid={} range=[{lo}, {hi}] chosen={} violations={}",
            valid_count, invalid_count, chosen, violations
        );
    }

    println!();
    if impossible.is_empty() {
        println!("RESULT: FEASIBLE");
        println!("check_values (i8): {:?}", check_values);
        print!("check_values (hex): [");
        for (i, v) in check_values.iter().enumerate() {
            if i != 0 {
                print!(", ");
            }
            print!("0x{:02x}", *v as u8);
        }
        println!("]");
    } else {
        println!("RESULT: NOT FEASIBLE; impossible buckets={impossible:?}");
    }
}
