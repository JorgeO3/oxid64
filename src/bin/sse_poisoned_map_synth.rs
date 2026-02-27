#![allow(clippy::cast_sign_loss)]

use std::collections::BTreeSet;

const DELTA_ASSO: [u8; 16] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00,
    0x0f,
];

const CURRENT_DELTA_VALUES: [u8; 16] = [
    0x00, 0x00, 0x00, 0x13, 0x04, 0xbf, 0xbf, 0xb9, 0xb9, 0x00, 0x10, 0xc3, 0xbf, 0xbf, 0xb9,
    0xb9,
];

#[inline]
fn avg_epu8(a: u8, b: u8) -> u8 {
    (((a as u16) + (b as u16) + 1) >> 1) as u8
}

#[inline]
fn expected_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[inline]
fn delta_hash_nibble(byte: u8, next_low3: u8, pos_in_dword: u8) -> u8 {
    debug_assert!(pos_in_dword < 4);
    debug_assert!(next_low3 < 8);
    let asso = DELTA_ASSO[(byte & 0x0f) as usize];
    let shifted = if pos_in_dword == 3 {
        byte >> 3
    } else {
        (byte >> 3) | ((next_low3 & 0x07) << 5)
    };
    avg_epu8(asso, shifted) & 0x0f
}

fn hash_candidates_for_byte(byte: u8) -> BTreeSet<u8> {
    let mut out = BTreeSet::new();
    for pos in 0..4u8 {
        if pos == 3 {
            out.insert(delta_hash_nibble(byte, 0, pos));
        } else {
            for next_low3 in 0..8u8 {
                out.insert(delta_hash_nibble(byte, next_low3, pos));
            }
        }
    }
    out
}

#[derive(Clone)]
struct BucketInfo {
    valid_bytes: BTreeSet<u8>,
    invalid_bytes: BTreeSet<u8>,
}

fn build_buckets() -> [BucketInfo; 16] {
    let mut buckets: [BucketInfo; 16] = std::array::from_fn(|_| BucketInfo {
        valid_bytes: BTreeSet::new(),
        invalid_bytes: BTreeSet::new(),
    });

    for byte in u8::MIN..=u8::MAX {
        let hashes = hash_candidates_for_byte(byte);
        let is_valid = expected_sextet(byte).is_some();
        for h in hashes {
            let bucket = &mut buckets[h as usize];
            if is_valid {
                bucket.valid_bytes.insert(byte);
            } else {
                bucket.invalid_bytes.insert(byte);
            }
        }
    }
    buckets
}

fn evaluate_current_table(buckets: &[BucketInfo; 16], poison_mask: u8) {
    let mut valid_total = 0usize;
    let mut valid_bad = 0usize;
    let mut invalid_total = 0usize;
    let mut invalid_unpoisoned = 0usize;

    for (h, b) in buckets.iter().enumerate() {
        let d = CURRENT_DELTA_VALUES[h];
        for &byte in &b.valid_bytes {
            valid_total += 1;
            let y = byte.wrapping_add(d);
            if let Some(exp) = expected_sextet(byte) {
                if y != exp {
                    valid_bad += 1;
                }
            }
        }
        for &byte in &b.invalid_bytes {
            invalid_total += 1;
            let y = byte.wrapping_add(d);
            if (y & poison_mask) == 0 {
                invalid_unpoisoned += 1;
            }
        }
    }

    println!(
        "Current table check (mask=0x{poison_mask:02X}): valid_bad={valid_bad}/{valid_total}, invalid_unpoisoned={invalid_unpoisoned}/{invalid_total}"
    );
}

fn synthesize_delta_values(
    buckets: &[BucketInfo; 16],
    poison_mask: u8,
) -> Result<[u8; 16], Vec<String>> {
    let mut out = [0u8; 16];
    let mut errors = Vec::new();

    for (h, bucket) in buckets.iter().enumerate() {
        // Step 1: derive fixed delta from valid bytes (if any)
        let mut required_delta: Option<u8> = None;
        for &byte in &bucket.valid_bytes {
            let exp = expected_sextet(byte).expect("valid set only");
            let d = exp.wrapping_sub(byte);
            if let Some(prev) = required_delta {
                if prev != d {
                    errors.push(format!(
                        "bucket {h}: conflicting valid deltas (at least {prev:#04x} vs {d:#04x})"
                    ));
                    required_delta = None;
                    break;
                }
            } else {
                required_delta = Some(d);
            }
        }

        // Step 2: enumerate candidates (free bucket = full domain)
        let mut candidates: Vec<u8> = if let Some(d) = required_delta {
            vec![d]
        } else if bucket.valid_bytes.is_empty() {
            (u8::MIN..=u8::MAX).collect()
        } else {
            Vec::new()
        };

        // Step 3: filter candidates by invalid poison requirement
        candidates.retain(|&d| {
            for &byte in &bucket.invalid_bytes {
                let y = byte.wrapping_add(d);
                if (y & poison_mask) == 0 {
                    return false;
                }
            }
            true
        });

        // Step 4: ensure valid mapping is exact for selected delta
        candidates.retain(|&d| {
            for &byte in &bucket.valid_bytes {
                let exp = expected_sextet(byte).expect("valid set only");
                let y = byte.wrapping_add(d);
                if y != exp {
                    return false;
                }
            }
            true
        });

        if candidates.is_empty() {
            errors.push(format!(
                "bucket {h}: no candidate delta satisfies constraints (valid={}, invalid={})",
                bucket.valid_bytes.len(),
                bucket.invalid_bytes.len()
            ));
            continue;
        }

        out[h] = candidates[0];
    }

    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn main() {
    let buckets = build_buckets();

    println!("Poisoned sextet mapping synthesis (fixed DELTA_ASSO, synthesize DELTA_VALUES)");
    println!("Constraints:");
    println!("1) valid [A-Za-z0-9+/] must decode exactly to sextet 0..63");
    println!("2) invalid bytes must set poison bit(s) in mapped byte");
    println!("3) hash is current delta_hash nibble (same as current kernel)");
    println!();

    for (h, b) in buckets.iter().enumerate() {
        println!(
            "bucket {:2}: valid={} invalid={}",
            h,
            b.valid_bytes.len(),
            b.invalid_bytes.len()
        );
    }
    println!();

    evaluate_current_table(&buckets, 0x80);
    evaluate_current_table(&buckets, 0xC0);
    println!();

    for mask in [0x80u8, 0xC0u8] {
        println!("Synthesis attempt with poison mask 0x{mask:02X}:");
        match synthesize_delta_values(&buckets, mask) {
            Ok(table) => {
                println!("  FEASIBLE");
                print!("  delta_values = [");
                for (i, v) in table.iter().enumerate() {
                    if i != 0 {
                        print!(", ");
                    }
                    print!("0x{v:02x}");
                }
                println!("]");
            }
            Err(errs) => {
                println!("  NOT FEASIBLE");
                for e in errs {
                    println!("  - {e}");
                }
            }
        }
        println!();
    }
}

