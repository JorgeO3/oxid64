#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use std::collections::BTreeSet;

const DELTA_ASSO: [u8; 16] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00,
    0x0f,
];

fn is_base64_valid_strict_simd(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'
    )
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
fn delta_hash_low_nibble(byte: u8, next_low3: u8, pos_in_dword: u8) -> u8 {
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

fn feasible_t_values(bucket_bytes: &BTreeSet<u8>) -> Vec<i8> {
    let mut out = Vec::new();
    for t in i8::MIN..=i8::MAX {
        let mut ok = true;
        for &byte in bucket_bytes {
            let y = sat_add_i8(t, byte as i8);
            let invalid = !is_base64_valid_strict_simd(byte);
            if invalid {
                if y >= 0 {
                    ok = false;
                    break;
                }
            } else if y < 0 {
                ok = false;
                break;
            }
        }
        if ok {
            out.push(t);
        }
    }
    out
}

fn main() {
    // For each nibble hash bucket (index used by pshufb), gather all bytes that can map there
    // across all 4 byte positions in a dword and all possible neighbor low-3 contexts.
    let mut buckets: [BTreeSet<u8>; 16] = std::array::from_fn(|_| BTreeSet::new());

    for byte in u8::MIN..=u8::MAX {
        for pos in 0..4u8 {
            if pos == 3 {
                let h = delta_hash_low_nibble(byte, 0, pos);
                buckets[h as usize].insert(byte);
            } else {
                for next_low3 in 0..8u8 {
                    let h = delta_hash_low_nibble(byte, next_low3, pos);
                    buckets[h as usize].insert(byte);
                }
            }
        }
    }

    println!(
        "Shared-hash strict feasibility for check_values2[16] using current delta_hash nibble"
    );
    println!(
        "Condition checked: sign(sat_add_i8(check_values2[h], input_byte)) == invalid_byte"
    );
    println!("Strict-valid set: [A-Za-z0-9+/], '=' treated invalid in SIMD region");
    println!();

    let mut global_ok = true;
    let mut impossible = Vec::new();

    for (h, bytes) in buckets.iter().enumerate() {
        let valid_count = bytes
            .iter()
            .filter(|&&b| is_base64_valid_strict_simd(b))
            .count();
        let invalid_count = bytes.len() - valid_count;
        let feasible = feasible_t_values(bytes);

        if feasible.is_empty() {
            global_ok = false;
            impossible.push(h);
            println!(
                "bucket {:2}: bytes={} valid={} invalid={} feasible_t=NONE",
                h,
                bytes.len(),
                valid_count,
                invalid_count
            );
        } else {
            println!(
                "bucket {:2}: bytes={} valid={} invalid={} feasible_t={} range=[{}, {}]",
                h,
                bytes.len(),
                valid_count,
                invalid_count,
                feasible.len(),
                feasible.first().unwrap_or(&0),
                feasible.last().unwrap_or(&0)
            );
        }
    }

    println!();
    if global_ok {
        println!("RESULT: FEASIBLE (at least one check_values2[16] exists under this model)");
    } else {
        println!(
            "RESULT: NOT FEASIBLE for single-table shared-hash under this model; impossible buckets: {:?}",
            impossible
        );
    }
}

