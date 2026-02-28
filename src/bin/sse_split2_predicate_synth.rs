#![allow(clippy::cast_sign_loss)]

use std::cmp::Ordering;
use std::collections::BTreeSet;

const DELTA_ASSO: [u8; 16] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x0f,
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

#[derive(Clone)]
struct CandidateResult {
    name: String,
    mixed_count: usize,
    impossible_count: usize,
    mixed_idxs: Vec<usize>,
    impossible_idxs: Vec<usize>,
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

fn buckets_for_pair(a: PredSpec, b: PredSpec) -> [BucketInfo; 64] {
    let mut buckets: [BucketInfo; 64] = std::array::from_fn(|_| BucketInfo {
        valid: BTreeSet::new(),
        invalid: BTreeSet::new(),
    });

    for byte in u8::MIN..=u8::MAX {
        let p0 = pred_bit(byte, a);
        let p1 = pred_bit(byte, b);
        let pred2 = p0 | (p1 << 1);
        let is_valid = expected_sextet(byte).is_some();

        for pos in 0..4u8 {
            if pos == 3 {
                let h = delta_hash_nibble(byte, 0, pos);
                let idx = ((h << 2) | pred2) as usize;
                if is_valid {
                    buckets[idx].valid.insert(byte);
                } else {
                    buckets[idx].invalid.insert(byte);
                }
            } else {
                for next_low3 in 0..8u8 {
                    let h = delta_hash_nibble(byte, next_low3, pos);
                    let idx = ((h << 2) | pred2) as usize;
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

fn option1_mixed_buckets(buckets: &[BucketInfo; 64]) -> Vec<usize> {
    let mut out = Vec::new();
    for (idx, b) in buckets.iter().enumerate() {
        if !b.valid.is_empty() && !b.invalid.is_empty() {
            out.push(idx);
        }
    }
    out
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

fn option2_impossible_buckets(buckets: &[BucketInfo; 64]) -> Vec<usize> {
    let mut out = Vec::new();
    for (idx, b) in buckets.iter().enumerate() {
        if feasible_t_values(&b.valid, &b.invalid).is_empty() {
            out.push(idx);
        }
    }
    out
}

fn evaluate_pair(a: PredSpec, b: PredSpec) -> CandidateResult {
    let name = format!("{} | {}", a.name, b.name);
    let buckets = buckets_for_pair(a, b);
    let mixed = option1_mixed_buckets(&buckets);
    let impossible = option2_impossible_buckets(&buckets);
    CandidateResult {
        name,
        mixed_count: mixed.len(),
        impossible_count: impossible.len(),
        mixed_idxs: mixed,
        impossible_idxs: impossible,
    }
}

fn cmp_candidate(lhs: &CandidateResult, rhs: &CandidateResult) -> Ordering {
    lhs.mixed_count
        .cmp(&rhs.mixed_count)
        .then(lhs.impossible_count.cmp(&rhs.impossible_count))
        .then(lhs.name.cmp(&rhs.name))
}

fn main() {
    let predicates = [
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

    let mut results = Vec::new();
    for i in 0..predicates.len() {
        for j in (i + 1)..predicates.len() {
            results.push(evaluate_pair(predicates[i], predicates[j]));
        }
    }

    results.sort_by(cmp_candidate);

    println!("2-bit split synthesis (idx=(delta_hash<<2)|pred2)\n");
    println!("Option1: pure bucket-validity lookup");
    println!("Option2: split sat-add tables (4x16 entries)\n");
    println!("Tested predicate pairs: {}\n", results.len());

    let feasible_opt1 = results.iter().filter(|r| r.mixed_count == 0).count();
    let feasible_opt2 = results.iter().filter(|r| r.impossible_count == 0).count();

    println!("Feasible counts:");
    println!("  Option1 feasible pairs: {feasible_opt1}");
    println!("  Option2 feasible pairs: {feasible_opt2}\n");

    println!("Top 8 candidates by (mixed_count, impossible_count):");
    for r in results.iter().take(8) {
        println!(
            "  {} => opt1_mixed={} opt2_impossible={}",
            r.name, r.mixed_count, r.impossible_count
        );
    }

    if let Some(best) = results.first() {
        println!("\nBest candidate details:");
        println!("  {}", best.name);
        println!(
            "  opt1_mixed={} idx={:?}",
            best.mixed_count, best.mixed_idxs
        );
        println!(
            "  opt2_impossible={} idx={:?}",
            best.impossible_count, best.impossible_idxs
        );
    }
}
