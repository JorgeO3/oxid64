#![allow(clippy::cast_possible_truncation)]

use oxid64::engine::scalar::encode_base64_fast;
use std::fs;
use std::path::PathBuf;

const DELTA_ASSO: [u8; 16] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x0f,
];

// Same table used by the hybrid experiment kernel.
// bucket index is low nibble of delta_hash.
const CONFLICT_BUCKETS: [u8; 16] = [0, 0, 0, 1, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0];

#[derive(Debug, Clone)]
struct Dataset {
    name: String,
    encoded: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct Stats {
    encoded_len: usize,
    safe_len: usize,
    vec_len: usize,
    amb_bytes: usize,
    blocks16_total: usize,
    blocks16_amb: usize,
    blocks64_total: usize,
    blocks64_amb: usize,
}

impl Stats {
    fn amb_byte_rate(self) -> f64 {
        if self.vec_len == 0 {
            0.0
        } else {
            self.amb_bytes as f64 / self.vec_len as f64
        }
    }

    fn amb_block16_rate(self) -> f64 {
        if self.blocks16_total == 0 {
            0.0
        } else {
            self.blocks16_amb as f64 / self.blocks16_total as f64
        }
    }

    fn amb_block64_rate(self) -> f64 {
        if self.blocks64_total == 0 {
            0.0
        } else {
            self.blocks64_amb as f64 / self.blocks64_total as f64
        }
    }
}

#[inline]
fn avg_epu8(a: u8, b: u8) -> u8 {
    (((a as u16) + (b as u16) + 1) >> 1) as u8
}

#[inline]
fn delta_hash_nibble(byte: u8, next_low3: u8, pos_in_dword: usize) -> u8 {
    debug_assert!(pos_in_dword < 4);
    let asso = DELTA_ASSO[(byte & 0x0f) as usize];
    let shifted = if pos_in_dword == 3 {
        byte >> 3
    } else {
        (byte >> 3) | ((next_low3 & 0x07) << 5)
    };
    avg_epu8(asso, shifted) & 0x0f
}

#[inline]
fn is_ambiguous_byte(encoded: &[u8], idx: usize) -> bool {
    let byte = encoded[idx];
    let pos_in_dword = idx & 3;
    let next_low3 = if pos_in_dword == 3 || idx + 1 >= encoded.len() {
        0
    } else {
        encoded[idx + 1] & 0x07
    };
    let h = delta_hash_nibble(byte, next_low3, pos_in_dword) as usize;
    CONFLICT_BUCKETS[h] != 0
}

fn analyze_encoded(encoded: &[u8]) -> Stats {
    // Mirrors SIMD decode strategy: keep last 4 chars for scalar tail safety.
    let safe_len = encoded.len().saturating_sub(4);
    let vec_len = (safe_len / 16) * 16;
    let mut amb_bytes = 0usize;
    let mut blocks16_amb = 0usize;
    let mut blocks64_amb = 0usize;

    for block_start in (0..vec_len).step_by(16) {
        let mut any16 = false;
        for i in block_start..block_start + 16 {
            if is_ambiguous_byte(encoded, i) {
                amb_bytes += 1;
                any16 = true;
            }
        }
        if any16 {
            blocks16_amb += 1;
        }
    }

    let blocks64_total = vec_len / 64;
    for block_start in (0..(blocks64_total * 64)).step_by(64) {
        let mut any64 = false;
        for i in block_start..block_start + 64 {
            if is_ambiguous_byte(encoded, i) {
                any64 = true;
                break;
            }
        }
        if any64 {
            blocks64_amb += 1;
        }
    }

    Stats {
        encoded_len: encoded.len(),
        safe_len,
        vec_len,
        amb_bytes,
        blocks16_total: vec_len / 16,
        blocks16_amb,
        blocks64_total,
        blocks64_amb,
    }
}

fn bench_like_encoded(size: usize) -> Vec<u8> {
    let mut input = vec![0u8; size];
    for (i, b) in input.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    let mut encoded = vec![0u8; ((size + 2) / 3) * 4 + 64];
    let n = encode_base64_fast(&input, &mut encoded);
    encoded.truncate(n);
    encoded
}

fn parse_sizes(arg: &str) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for tok in arg.split(',') {
        let s = tok.trim();
        if s.is_empty() {
            continue;
        }
        let v = s
            .parse::<usize>()
            .map_err(|e| format!("invalid size '{s}': {e}"))?;
        out.push(v);
    }
    if out.is_empty() {
        return Err("empty --sizes list".to_string());
    }
    Ok(out)
}

fn print_stats(name: &str, st: Stats) {
    println!("dataset: {name}");
    println!("  encoded_len          : {}", st.encoded_len);
    println!("  safe_len             : {}", st.safe_len);
    println!("  vectorized_len       : {}", st.vec_len);
    println!("  ambiguous_bytes      : {}", st.amb_bytes);
    println!(
        "  ambiguous_byte_rate  : {:.4}%",
        st.amb_byte_rate() * 100.0
    );
    println!(
        "  16B ambiguous blocks : {}/{} ({:.4}%)",
        st.blocks16_amb,
        st.blocks16_total,
        st.amb_block16_rate() * 100.0
    );
    println!(
        "  64B ambiguous blocks : {}/{} ({:.4}%)",
        st.blocks64_amb,
        st.blocks64_total,
        st.amb_block64_rate() * 100.0
    );
    println!();
}

fn main() -> Result<(), String> {
    let mut sizes = vec![1024usize, 1024 * 1024];
    let mut file: Option<PathBuf> = None;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--sizes" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --sizes".to_string())?;
                sizes = parse_sizes(v)?;
                i += 2;
            }
            "--file" => {
                let p = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --file".to_string())?;
                file = Some(PathBuf::from(p));
                i += 2;
            }
            "-h" | "--help" => {
                println!("Usage:");
                println!("  cargo run --bin sse_ambiguity_rate");
                println!("  cargo run --bin sse_ambiguity_rate -- --sizes 1024,1048576");
                println!("  cargo run --bin sse_ambiguity_rate -- --file /path/to/base64.txt");
                return Ok(());
            }
            other => {
                return Err(format!("unknown arg: {other}"));
            }
        }
    }

    println!("SSE ambiguity-rate probe");
    println!("  conflict buckets: {:?}", CONFLICT_BUCKETS);
    println!("  model: current delta_hash nibble + hybrid conflict table");
    println!();

    let mut datasets = Vec::<Dataset>::new();
    for s in sizes {
        let encoded = bench_like_encoded(s);
        datasets.push(Dataset {
            name: format!("bench_like(size={s})"),
            encoded,
        });
    }

    if let Some(path) = file {
        let data =
            fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        datasets.push(Dataset {
            name: format!("file({})", path.display()),
            encoded: data,
        });
    }

    for ds in &datasets {
        let st = analyze_encoded(&ds.encoded);
        print_stats(&ds.name, st);
    }

    println!("Interpretation:");
    println!("  - lower ambiguous rates => better ROI potential for two-tier strict fallback.");
    println!("  - if 64B ambiguous rate is near 100%, two-tier fallback is unlikely to help.");

    Ok(())
}
