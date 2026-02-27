#![allow(clippy::cast_sign_loss)]

use std::collections::BTreeSet;

const DELTA_ASSO: [u8; 16] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00,
    0x0f,
];

#[derive(Clone, Copy)]
enum PredKind {
    Bit(u8),
    Xor(u8, u8),
    Or(u8, u8),
    And(u8, u8),
}

#[derive(Clone, Copy)]
struct PredSpec {
    name: &'static str,
    kind: PredKind,
}

#[derive(Clone)]
struct BucketInfo {
    valid: BTreeSet<u8>,
    invalid: BTreeSet<u8>,
}

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
fn pred_bit(byte: u8, spec: PredSpec) -> u8 {
    match spec.kind {
        PredKind::Bit(mask) => ((byte & mask) != 0) as u8,
        PredKind::Xor(a, b) => (((byte & a) != 0) ^ ((byte & b) != 0)) as u8,
        PredKind::Or(a, b) => (((byte & a) != 0) || ((byte & b) != 0)) as u8,
        PredKind::And(a, b) => (((byte & a) != 0) && ((byte & b) != 0)) as u8,
    }
}

fn buckets_for_pred(spec: PredSpec) -> [BucketInfo; 32] {
    let mut buckets: [BucketInfo; 32] = std::array::from_fn(|_| BucketInfo {
        valid: BTreeSet::new(),
        invalid: BTreeSet::new(),
    });

    for byte in u8::MIN..=u8::MAX {
        let pred = pred_bit(byte, spec);
        let is_valid = expected_sextet(byte).is_some();

        for pos in 0..4u8 {
            if pos == 3 {
                let h = delta_hash_nibble(byte, 0, pos);
                let idx = ((h << 1) | pred) as usize;
                if is_valid {
                    buckets[idx].valid.insert(byte);
                } else {
                    buckets[idx].invalid.insert(byte);
                }
            } else {
                for next_low3 in 0..8u8 {
                    let h = delta_hash_nibble(byte, next_low3, pos);
                    let idx = ((h << 1) | pred) as usize;
                    if is_valid {
                        buckets[idx].valid.insert(byte);
                    } else {
                        buckets[idx].invalid.insert(byte);
                    }
                }
            }
        }
    }

    buckets
}

fn option1_mixed_buckets(buckets: &[BucketInfo; 32]) -> Vec<usize> {
    let mut mixed = Vec::new();
    for (idx, b) in buckets.iter().enumerate() {
        if !b.valid.is_empty() && !b.invalid.is_empty() {
            mixed.push(idx);
        }
    }
    mixed
}

fn feasible_t_values(valid: &BTreeSet<u8>, invalid: &BTreeSet<u8>) -> Vec<i8> {
    let mut out = Vec::new();

    for t in i8::MIN..=i8::MAX {
        let mut ok = true;
        for &byte in valid {
            let y = sat_add_i8(t, byte as i8);
            if y < 0 {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        for &byte in invalid {
            let y = sat_add_i8(t, byte as i8);
            if y >= 0 {
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

fn option2_impossible_buckets(buckets: &[BucketInfo; 32]) -> Vec<usize> {
    let mut impossible = Vec::new();
    for (idx, b) in buckets.iter().enumerate() {
        let feasible = feasible_t_values(&b.valid, &b.invalid);
        if feasible.is_empty() {
            impossible.push(idx);
        }
    }
    impossible
}

fn option2_example_tables(buckets: &[BucketInfo; 32]) -> Option<([i8; 16], [i8; 16])> {
    let mut t0 = [0i8; 16];
    let mut t1 = [0i8; 16];

    for h in 0..16usize {
        let b0 = &buckets[h * 2];
        let b1 = &buckets[h * 2 + 1];
        let f0 = feasible_t_values(&b0.valid, &b0.invalid);
        let f1 = feasible_t_values(&b1.valid, &b1.invalid);
        if f0.is_empty() || f1.is_empty() {
            return None;
        }
        t0[h] = f0[0];
        t1[h] = f1[0];
    }
    Some((t0, t1))
}

fn print_bucket_detail(spec: PredSpec, buckets: &[BucketInfo; 32]) {
    println!("Detailed bucket composition for {}", spec.name);
    for (idx, b) in buckets.iter().enumerate() {
        if !b.valid.is_empty() || !b.invalid.is_empty() {
            println!(
                "  idx {:2}: valid={} invalid={}",
                idx,
                b.valid.len(),
                b.invalid.len()
            );
        }
    }
}

fn main() {
    let preds = [
        PredSpec {
            name: "bit7",
            kind: PredKind::Bit(0x80),
        },
        PredSpec {
            name: "bit6",
            kind: PredKind::Bit(0x40),
        },
        PredSpec {
            name: "bit5",
            kind: PredKind::Bit(0x20),
        },
        PredSpec {
            name: "bit4",
            kind: PredKind::Bit(0x10),
        },
        PredSpec {
            name: "bit3",
            kind: PredKind::Bit(0x08),
        },
        PredSpec {
            name: "bit6_xor_bit5",
            kind: PredKind::Xor(0x40, 0x20),
        },
        PredSpec {
            name: "bit6_or_bit5",
            kind: PredKind::Or(0x40, 0x20),
        },
        PredSpec {
            name: "bit6_and_bit5",
            kind: PredKind::And(0x40, 0x20),
        },
        PredSpec {
            name: "bit5_xor_bit4",
            kind: PredKind::Xor(0x20, 0x10),
        },
    ];

    println!("Split-predicate synthesis on current delta_hash (idx=(hash<<1)|pred)\n");
    println!("Option1: pure bucket-validity lookup (MSB invalid flag)");
    println!("Option2: sat-add split tables (two 16-entry tables by pred)\n");

    let mut best_opt1 = (usize::MAX, "");
    let mut best_opt2 = (usize::MAX, "");

    for spec in preds {
        let buckets = buckets_for_pred(spec);
        let mixed = option1_mixed_buckets(&buckets);
        let impossible = option2_impossible_buckets(&buckets);
        let opt1_ok = mixed.is_empty();
        let opt2_ok = impossible.is_empty();

        println!(
            "{}: opt1_mixed={} opt1={} | opt2_impossible={} opt2={}",
            spec.name,
            mixed.len(),
            if opt1_ok { "FEASIBLE" } else { "NOT_FEASIBLE" },
            impossible.len(),
            if opt2_ok { "FEASIBLE" } else { "NOT_FEASIBLE" }
        );

        if !opt1_ok {
            println!("  opt1 mixed idx: {:?}", mixed);
        }
        if !opt2_ok {
            println!("  opt2 impossible idx: {:?}", impossible);
        } else if let Some((t0, t1)) = option2_example_tables(&buckets) {
            print!("  opt2 example t0=[");
            for (i, v) in t0.iter().enumerate() {
                if i != 0 {
                    print!(", ");
                }
                print!("{v}");
            }
            println!("]");
            print!("  opt2 example t1=[");
            for (i, v) in t1.iter().enumerate() {
                if i != 0 {
                    print!(", ");
                }
                print!("{v}");
            }
            println!("]");
        }

        if mixed.len() < best_opt1.0 {
            best_opt1 = (mixed.len(), spec.name);
        }
        if impossible.len() < best_opt2.0 {
            best_opt2 = (impossible.len(), spec.name);
        }
    }

    println!("\nBest by Option1 mixed-bucket count: {} ({})", best_opt1.1, best_opt1.0);
    println!(
        "Best by Option2 impossible-bucket count: {} ({})",
        best_opt2.1, best_opt2.0
    );

    // Print one detailed profile for the current best Option2 candidate for easier follow-up.
    if let Some(best) = [
        PredSpec {
            name: "bit7",
            kind: PredKind::Bit(0x80),
        },
        PredSpec {
            name: "bit6",
            kind: PredKind::Bit(0x40),
        },
        PredSpec {
            name: "bit5",
            kind: PredKind::Bit(0x20),
        },
        PredSpec {
            name: "bit4",
            kind: PredKind::Bit(0x10),
        },
        PredSpec {
            name: "bit3",
            kind: PredKind::Bit(0x08),
        },
        PredSpec {
            name: "bit6_xor_bit5",
            kind: PredKind::Xor(0x40, 0x20),
        },
        PredSpec {
            name: "bit6_or_bit5",
            kind: PredKind::Or(0x40, 0x20),
        },
        PredSpec {
            name: "bit6_and_bit5",
            kind: PredKind::And(0x40, 0x20),
        },
        PredSpec {
            name: "bit5_xor_bit4",
            kind: PredKind::Xor(0x20, 0x10),
        },
    ]
    .into_iter()
    .find(|p| p.name == best_opt2.1)
    {
        println!();
        let buckets = buckets_for_pred(best);
        print_bucket_detail(best, &buckets);
    }
}

