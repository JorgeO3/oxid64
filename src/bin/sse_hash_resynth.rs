#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use std::cmp::Ordering;
use std::env;
use std::thread;

const BASE_DELTA_ASSO: [u8; 16] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x00,
    0x0f,
];
const BASE_DELTA_PERTURB: [u8; 16] = [0u8; 16];
const BASE_DELTA_HI: [u8; 16] = [0u8; 16];
const BASE_DELTA_POS: [[u8; 16]; 4] = [[0u8; 16]; 4];

#[derive(Clone, Copy, Debug)]
enum MixOp {
    Avg,
    Add,
    Xor,
    Min,
    Max,
    Sub,
}

impl MixOp {
    fn name(self) -> &'static str {
        match self {
            MixOp::Avg => "avg_epu8",
            MixOp::Add => "add_epi8",
            MixOp::Xor => "xor_si128",
            MixOp::Min => "min_epu8",
            MixOp::Max => "max_epu8",
            MixOp::Sub => "sub_epi8",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HashFamily {
    OneLut,
    TwoLutAdd,
    TwoLutAvg,
    TwoLutXor,
    PredSelBit5,
    PredSelBit6,
    PosAware,
    LoHiAdd,
    DualIndexBit5,
    DualIndexBit6,
}

impl HashFamily {
    fn name(self) -> &'static str {
        match self {
            HashFamily::OneLut => "one_lut",
            HashFamily::TwoLutAdd => "two_lut_add",
            HashFamily::TwoLutAvg => "two_lut_avg",
            HashFamily::TwoLutXor => "two_lut_xor",
            HashFamily::PredSelBit5 => "predsel_bit5",
            HashFamily::PredSelBit6 => "predsel_bit6",
            HashFamily::PosAware => "pos_aware",
            HashFamily::LoHiAdd => "lo_hi_add",
            HashFamily::DualIndexBit5 => "dual_index_bit5",
            HashFamily::DualIndexBit6 => "dual_index_bit6",
        }
    }

    fn extra_ops(self) -> u8 {
        match self {
            HashFamily::OneLut => 0,
            HashFamily::TwoLutAdd => 1,
            HashFamily::TwoLutAvg => 1,
            HashFamily::TwoLutXor => 1,
            HashFamily::PredSelBit5 => 1,
            HashFamily::PredSelBit6 => 1,
            HashFamily::PosAware => 0,
            HashFamily::LoHiAdd => 1,
            HashFamily::DualIndexBit5 => 1,
            HashFamily::DualIndexBit6 => 1,
        }
    }

    fn const_live_delta(self) -> i8 {
        match self {
            HashFamily::OneLut => 0,
            // Adds one more constant table (second hash LUT) vs one_lut.
            HashFamily::TwoLutAdd => 1,
            HashFamily::TwoLutAvg => 1,
            HashFamily::TwoLutXor => 1,
            HashFamily::PredSelBit5 => 1,
            HashFamily::PredSelBit6 => 1,
            HashFamily::PosAware => 3, // 4 per-position tables instead of one.
            HashFamily::LoHiAdd => 1,
            HashFamily::DualIndexBit5 => 0,
            HashFamily::DualIndexBit6 => 0,
        }
    }

    fn uses_perturb(self) -> bool {
        matches!(
            self,
            HashFamily::TwoLutAdd
                | HashFamily::TwoLutAvg
                | HashFamily::TwoLutXor
                | HashFamily::PredSelBit5
                | HashFamily::PredSelBit6
        )
    }

    fn uses_hi(self) -> bool {
        matches!(self, HashFamily::LoHiAdd)
    }

    fn uses_pos(self) -> bool {
        matches!(self, HashFamily::PosAware)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndexMode {
    Maskless,
    Masked0f,
}

impl IndexMode {
    fn name(self) -> &'static str {
        match self {
            IndexMode::Maskless => "maskless",
            IndexMode::Masked0f => "masked_0f",
        }
    }

    fn extra_ops(self) -> u8 {
        match self {
            IndexMode::Maskless => 0,
            IndexMode::Masked0f => 1, // one pand for idx sanitization
        }
    }

    fn const_live_delta(self) -> i8 {
        match self {
            // shared-hash drops check_asso: -1 live constant vs strict baseline.
            IndexMode::Maskless => -1,
            // shared-hash drops check_asso but masked index needs mask0f: net 0.
            IndexMode::Masked0f => 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct HashSpec {
    shift: u8,
    mix: MixOp,
    index_mode: IndexMode,
    family: HashFamily,
}

impl HashSpec {
    fn extra_ops(self) -> u8 {
        self.index_mode.extra_ops() + self.family.extra_ops()
    }

    fn const_live_delta(self) -> i8 {
        self.index_mode.const_live_delta() + self.family.const_live_delta()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Score {
    map_conflict_buckets: u32,
    valid_msb_violations: u32,
    ascii_msb_violations: u32,
    impossible_buckets: u32,
    mixed_buckets: u32,
    feasible_width_sum: u32,
    poison_impossible_buckets: u32,
}

impl Score {
    fn better_than(self, other: Self) -> bool {
        self.cmp_key().cmp(&other.cmp_key()) == Ordering::Less
    }

    fn cmp_key(self) -> (u32, u32, u32, u32, u32, i32) {
        (
            self.map_conflict_buckets,
            self.valid_msb_violations,
            self.ascii_msb_violations,
            self.impossible_buckets,
            self.mixed_buckets,
            -(self.feasible_width_sum as i32),
        )
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    spec: HashSpec,
    tables: HashTables,
    score: Score,
}

impl Candidate {
    fn pass_maskless(&self) -> bool {
        self.spec.index_mode == IndexMode::Maskless
            && self.score.map_conflict_buckets == 0
            && self.score.valid_msb_violations == 0
            && self.score.ascii_msb_violations == 0
            && self.score.impossible_buckets == 0
            && self.spec.extra_ops() <= 1
            && self.spec.const_live_delta() <= 0
    }

    fn pass_masked(&self) -> bool {
        self.spec.index_mode == IndexMode::Masked0f
            && self.score.map_conflict_buckets == 0
            && self.score.impossible_buckets == 0
            && self.spec.extra_ops() <= 1
            && self.spec.const_live_delta() <= 0
    }

    fn pass_poison_maskless(&self) -> bool {
        self.spec.index_mode == IndexMode::Maskless
            && self.score.map_conflict_buckets == 0
            && self.score.valid_msb_violations == 0
            && self.score.ascii_msb_violations == 0
            && self.score.poison_impossible_buckets == 0
            && self.spec.extra_ops() <= 1
            && self.spec.const_live_delta() <= 0
    }

    fn pass_poison_masked(&self) -> bool {
        self.spec.index_mode == IndexMode::Masked0f
            && self.score.map_conflict_buckets == 0
            && self.score.poison_impossible_buckets == 0
            && self.spec.extra_ops() <= 1
            && self.spec.const_live_delta() <= 0
    }

    fn pass_shared_extended(&self) -> bool {
        self.score.map_conflict_buckets == 0
            && self.score.impossible_buckets == 0
            && self.score.valid_msb_violations == 0
            && self.score.ascii_msb_violations == 0
            && self.spec.extra_ops() <= 2
            && self.spec.const_live_delta() <= 1
    }

    fn pass_poison_extended(&self) -> bool {
        self.score.map_conflict_buckets == 0
            && self.score.poison_impossible_buckets == 0
            && self.score.valid_msb_violations == 0
            && self.score.ascii_msb_violations == 0
            && self.spec.extra_ops() <= 2
            && self.spec.const_live_delta() <= 1
    }
}

#[derive(Clone, Copy, Debug)]
struct HashTables {
    primary: [u8; 16],
    perturb: [u8; 16],
    hi: [u8; 16],
    pos: [[u8; 16]; 4],
}

type BucketBits = [[u64; 4]; 16];

#[derive(Clone)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let s = if seed == 0 {
            0xA5A5_0123_89AB_CDEF
        } else {
            seed
        };
        Self { state: s }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_u8(&mut self) -> u8 {
        (self.next_u64() & 0xff) as u8
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        (self.next_u64() as usize) % upper
    }
}

#[inline]
fn avg_epu8(a: u8, b: u8) -> u8 {
    (((a as u16) + (b as u16) + 1) >> 1) as u8
}

#[inline]
fn is_base64_valid_strict_simd(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'
    )
}

#[inline]
fn base64_sextet(byte: u8) -> Option<u8> {
    Some(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    })
}

#[inline]
fn hash_mixed_raw(
    byte: u8,
    next_low_k: u8,
    pos_in_dword: u8,
    tables: &HashTables,
    spec: HashSpec,
) -> u8 {
    debug_assert!(pos_in_dword < 4);
    let k = spec.shift;
    debug_assert!(k >= 1 && k <= 7);

    let lo = (byte & 0x0f) as usize;
    let hi = (byte >> 4) as usize;
    let shifted = if pos_in_dword == 3 {
        byte >> k
    } else {
        let mask = ((1u16 << k) - 1) as u8;
        (byte >> k) | ((next_low_k & mask) << (8 - k))
    };

    let base = match spec.family {
        HashFamily::PosAware => tables.pos[pos_in_dword as usize][lo],
        HashFamily::LoHiAdd => tables.primary[lo].wrapping_add(tables.hi[hi]),
        HashFamily::PredSelBit5 => {
            if (byte & 0x20) != 0 {
                tables.perturb[lo]
            } else {
                tables.primary[lo]
            }
        }
        HashFamily::PredSelBit6 => {
            if (byte & 0x40) != 0 {
                tables.perturb[lo]
            } else {
                tables.primary[lo]
            }
        }
        _ => tables.primary[lo],
    };

    let core = match spec.mix {
        MixOp::Avg => avg_epu8(base, shifted),
        MixOp::Add => base.wrapping_add(shifted),
        MixOp::Xor => base ^ shifted,
        MixOp::Min => base.min(shifted),
        MixOp::Max => base.max(shifted),
        MixOp::Sub => base.wrapping_sub(shifted),
    };

    match spec.family {
        HashFamily::OneLut => core,
        HashFamily::TwoLutAdd => core.wrapping_add(tables.perturb[lo]),
        HashFamily::TwoLutAvg => avg_epu8(core, tables.perturb[lo]),
        HashFamily::TwoLutXor => core ^ tables.perturb[lo],
        HashFamily::PredSelBit5
        | HashFamily::PredSelBit6
        | HashFamily::PosAware
        | HashFamily::LoHiAdd
        | HashFamily::DualIndexBit5
        | HashFamily::DualIndexBit6 => core,
    }
}

#[inline]
fn hash_indexes(mixed: u8, byte: u8, spec: HashSpec) -> (usize, usize) {
    let base_idx = match spec.index_mode {
        IndexMode::Maskless => (mixed & 0x0f) as usize,
        IndexMode::Masked0f => (mixed & 0x0f) as usize,
    };
    let check_idx = match spec.family {
        HashFamily::DualIndexBit5 => base_idx ^ ((((byte >> 5) & 1) as usize) << 3),
        HashFamily::DualIndexBit6 => base_idx ^ ((((byte >> 6) & 1) as usize) << 3),
        _ => base_idx,
    };
    (base_idx, check_idx)
}

#[inline]
fn set_bucket_bit(buckets: &mut BucketBits, bucket_idx: usize, byte: u8) {
    let word = (byte >> 6) as usize;
    let bit = (byte & 63) as u32;
    buckets[bucket_idx][word] |= 1u64 << bit;
}

fn build_buckets_and_msb_violations(
    tables: &HashTables,
    spec: HashSpec,
    valid_lut: &[bool; 256],
) -> (BucketBits, BucketBits, u32, u32) {
    let mut map_buckets = [[0u64; 4]; 16];
    let mut check_buckets = [[0u64; 4]; 16];
    let mut valid_msb_violations = 0u32;
    let mut ascii_msb_violations = 0u32;
    let contexts = 1u16 << spec.shift;
    for byte in u8::MIN..=u8::MAX {
        let is_ascii_nonneg = byte < 128;
        let is_valid = valid_lut[byte as usize];
        for pos in 0..4u8 {
            if pos == 3 {
                let mixed = hash_mixed_raw(byte, 0, pos, tables, spec);
                if is_valid && (mixed & 0x80) != 0 {
                    valid_msb_violations += 1;
                }
                if is_ascii_nonneg && (mixed & 0x80) != 0 {
                    ascii_msb_violations += 1;
                }
                let (h_map, h_check) = hash_indexes(mixed, byte, spec);
                set_bucket_bit(&mut map_buckets, h_map, byte);
                set_bucket_bit(&mut check_buckets, h_check, byte);
            } else {
                for next_low in 0..contexts {
                    let mixed = hash_mixed_raw(byte, next_low as u8, pos, tables, spec);
                    if is_valid && (mixed & 0x80) != 0 {
                        valid_msb_violations += 1;
                    }
                    if is_ascii_nonneg && (mixed & 0x80) != 0 {
                        ascii_msb_violations += 1;
                    }
                    let (h_map, h_check) = hash_indexes(mixed, byte, spec);
                    set_bucket_bit(&mut map_buckets, h_map, byte);
                    set_bucket_bit(&mut check_buckets, h_check, byte);
                }
            }
        }
    }
    (
        map_buckets,
        check_buckets,
        valid_msb_violations,
        ascii_msb_violations,
    )
}

#[inline]
fn bucket_fixed_delta_conflict(
    bucket: &[u64; 4],
    valid_lut: &[bool; 256],
    required_delta_lut: &[i8; 256],
) -> Result<Option<i8>, ()> {
    let mut delta_seen: Option<i8> = None;
    for (word_idx, word) in bucket.iter().enumerate() {
        let mut bits = *word;
        while bits != 0 {
            let bit = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let byte_idx = word_idx * 64 + bit;
            if !valid_lut[byte_idx] {
                continue;
            }
            let d = required_delta_lut[byte_idx];
            if let Some(prev) = delta_seen {
                if prev != d {
                    return Err(());
                }
            } else {
                delta_seen = Some(d);
            }
        }
    }
    Ok(delta_seen)
}

#[inline]
fn bucket_poison_feasible_delta_80(
    bucket: &[u64; 4],
    valid_lut: &[bool; 256],
    fixed_delta: Option<i8>,
) -> Option<u8> {
    // Robust poison criterion used here:
    // invalid lane must satisfy ((mapped | input) & 0x80) != 0.
    // For input >= 128 this is always true; constraints come from ASCII invalids only.
    if let Some(d) = fixed_delta {
        let du = d as u8;
        for (word_idx, word) in bucket.iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let byte = (word_idx * 64 + bit) as u8;
                if valid_lut[byte as usize] || (byte & 0x80) != 0 {
                    continue;
                }
                if byte.wrapping_add(du) < 128 {
                    return None;
                }
            }
        }
        return Some(du);
    }

    // Free bucket: choose any delta that poisons all ASCII invalid bytes.
    let mut min_ascii_invalid = u8::MAX;
    let mut max_ascii_invalid = u8::MIN;
    let mut any_ascii_invalid = false;
    for (word_idx, word) in bucket.iter().enumerate() {
        let mut bits = *word;
        while bits != 0 {
            let bit = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let byte = (word_idx * 64 + bit) as u8;
            if valid_lut[byte as usize] || (byte & 0x80) != 0 {
                continue;
            }
            any_ascii_invalid = true;
            min_ascii_invalid = min_ascii_invalid.min(byte);
            max_ascii_invalid = max_ascii_invalid.max(byte);
        }
    }
    if !any_ascii_invalid {
        return Some(0);
    }

    // For each ASCII invalid byte b, valid d interval is [128-b, 255-b].
    // Intersection non-empty iff max_b - min_b <= 127.
    if max_ascii_invalid.wrapping_sub(min_ascii_invalid) > 127 {
        return None;
    }

    Some(128u8.wrapping_sub(min_ascii_invalid))
}

fn evaluate_table(
    tables: &HashTables,
    spec: HashSpec,
    valid_lut: &[bool; 256],
    required_delta_lut: &[i8; 256],
) -> Score {
    let (map_buckets, check_buckets, valid_msb_violations, ascii_msb_violations) =
        build_buckets_and_msb_violations(tables, spec, valid_lut);
    let mut score = Score {
        valid_msb_violations,
        ascii_msb_violations,
        ..Score::default()
    };

    let mut fixed_deltas: [Option<i8>; 16] = [None; 16];
    let mut delta_conflict: [bool; 16] = [false; 16];

    // Gate A: mapping feasibility per bucket (valid bytes in one bucket must share one delta).
    for (h, bucket) in map_buckets.iter().enumerate() {
        match bucket_fixed_delta_conflict(bucket, valid_lut, required_delta_lut) {
            Ok(d) => {
                fixed_deltas[h] = d;
            }
            Err(()) => {
                score.map_conflict_buckets += 1;
                delta_conflict[h] = true;
            }
        }
    }

    for bucket in &check_buckets {
        let mut seen_any = false;
        let mut has_valid = false;
        let mut has_invalid = false;

        // Feasible t interval under sat_add sign constraints:
        // valid byte b:   t >= -b_i8
        // invalid byte b: t <= -b_i8 - 1
        let mut lo = -128i16;
        let mut hi = 127i16;

        for (word_idx, word) in bucket.iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                seen_any = true;
                let byte_idx = word_idx * 64 + bit;
                let b = byte_idx as u8;
                let bi = (b as i8) as i16;
                if valid_lut[byte_idx] {
                    has_valid = true;
                    let lower = -bi;
                    if lower > lo {
                        lo = lower;
                    }
                } else {
                    has_invalid = true;
                    let upper = -bi - 1;
                    if upper < hi {
                        hi = upper;
                    }
                }
            }
        }

        if !seen_any {
            continue;
        }
        if has_valid && has_invalid {
            score.mixed_buckets += 1;
        }
        if lo <= hi {
            score.feasible_width_sum += (hi - lo + 1) as u32;
        } else {
            score.impossible_buckets += 1;
        }
    }

    // Poisoned-mapping feasibility (mask 0x80) with robust invalid criterion:
    // invalid -> ((mapped | input) & 0x80) != 0.
    for (h, bucket) in map_buckets.iter().enumerate() {
        if delta_conflict[h] {
            score.poison_impossible_buckets += 1;
            continue;
        }

        if bucket_poison_feasible_delta_80(bucket, valid_lut, fixed_deltas[h]).is_none() {
            score.poison_impossible_buckets += 1;
        }
    }

    score
}

fn local_search(
    spec: HashSpec,
    restarts: usize,
    iters: usize,
    valid_lut: &[bool; 256],
    required_delta_lut: &[i8; 256],
) -> Candidate {
    let mut rng = XorShift64::new(
        0xD00D_BAAD_F00D_CAFEu64
            ^ (spec.shift as u64)
            ^ ((spec.mix as u8 as u64) << 16)
            ^ ((spec.index_mode as u8 as u64) << 20)
            ^ ((spec.family as u8 as u64) << 22)
            ^ ((restarts as u64) << 24)
            ^ ((iters as u64) << 32),
    );

    let baseline_tables = HashTables {
        primary: BASE_DELTA_ASSO,
        perturb: BASE_DELTA_PERTURB,
        hi: BASE_DELTA_HI,
        pos: BASE_DELTA_POS,
    };
    let baseline_score = evaluate_table(&baseline_tables, spec, valid_lut, required_delta_lut);
    let mut best_tables = baseline_tables;
    let mut best_score = baseline_score;

    for restart in 0..restarts {
        let mut tables = if restart == 0 {
            baseline_tables
        } else {
            let mut primary = [0u8; 16];
            for v in &mut primary {
                *v = rng.next_u8();
            }
            let mut perturb = [0u8; 16];
            if spec.family.uses_perturb() {
                for v in &mut perturb {
                    *v = rng.next_u8();
                }
            }
            let mut hi = [0u8; 16];
            if spec.family.uses_hi() {
                for v in &mut hi {
                    *v = rng.next_u8();
                }
            }
            let mut pos = [[0u8; 16]; 4];
            if spec.family.uses_pos() {
                for table in &mut pos {
                    for v in table {
                        *v = rng.next_u8();
                    }
                }
            }
            HashTables {
                primary,
                perturb,
                hi,
                pos,
            }
        };

        let mut current_score = evaluate_table(&tables, spec, valid_lut, required_delta_lut);
        if current_score.better_than(best_score) {
            best_score = current_score;
            best_tables = tables;
        }

        for _ in 0..iters {
            let idx = rng.next_usize(16);
            let choose = rng.next_usize(4);
            let mut target = 0u8; // 0 primary, 1 perturb, 2 hi, 3 pos
            if spec.family.uses_pos() && choose == 3 {
                target = 3;
            } else if spec.family.uses_hi() && choose == 2 {
                target = 2;
            } else if spec.family.uses_perturb() && choose == 1 {
                target = 1;
            }
            let pos_lane = rng.next_usize(4);
            let old = match target {
                1 => tables.perturb[idx],
                2 => tables.hi[idx],
                3 => tables.pos[pos_lane][idx],
                _ => tables.primary[idx],
            };
            let mut new = rng.next_u8();
            if new == old {
                new ^= 0x5a;
            }

            match target {
                1 => tables.perturb[idx] = new,
                2 => tables.hi[idx] = new,
                3 => tables.pos[pos_lane][idx] = new,
                _ => tables.primary[idx] = new,
            }
            let trial_score = evaluate_table(&tables, spec, valid_lut, required_delta_lut);

            if trial_score.better_than(current_score) {
                current_score = trial_score;
                if trial_score.better_than(best_score) {
                    best_score = trial_score;
                    best_tables = tables;
                }
            } else {
                match target {
                    1 => tables.perturb[idx] = old,
                    2 => tables.hi[idx] = old,
                    3 => tables.pos[pos_lane][idx] = old,
                    _ => tables.primary[idx] = old,
                }
            }
        }
    }

    Candidate {
        spec,
        tables: best_tables,
        score: best_score,
    }
}

fn format_table_hex(table: &[u8; 16]) -> String {
    table
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_tables_hex(spec: HashSpec, tables: &HashTables) -> String {
    let mut parts = vec![format!("primary=[{}]", format_table_hex(&tables.primary))];
    if spec.family.uses_perturb() {
        parts.push(format!("perturb=[{}]", format_table_hex(&tables.perturb)));
    }
    if spec.family.uses_hi() {
        parts.push(format!("hi=[{}]", format_table_hex(&tables.hi)));
    }
    if spec.family.uses_pos() {
        parts.push(format!("pos0=[{}]", format_table_hex(&tables.pos[0])));
        parts.push(format!("pos1=[{}]", format_table_hex(&tables.pos[1])));
        parts.push(format!("pos2=[{}]", format_table_hex(&tables.pos[2])));
        parts.push(format!("pos3=[{}]", format_table_hex(&tables.pos[3])));
    }
    parts.join(" ")
}

fn derive_delta_values(
    tables: &HashTables,
    spec: HashSpec,
    valid_lut: &[bool; 256],
    required_delta_lut: &[i8; 256],
) -> Option<[i8; 16]> {
    let (buckets, _, _, _) = build_buckets_and_msb_violations(tables, spec, valid_lut);
    let mut out = [0i8; 16];

    for (h, bucket_bits) in buckets.iter().enumerate() {
        let mut chosen: Option<i8> = None;
        for (word_idx, word) in bucket_bits.iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let byte_idx = word_idx * 64 + bit;
                if !valid_lut[byte_idx] {
                    continue;
                }
                let d = required_delta_lut[byte_idx];
                if let Some(prev) = chosen {
                    if prev != d {
                        return None;
                    }
                } else {
                    chosen = Some(d);
                }
            }
        }
        out[h] = chosen.unwrap_or(0);
    }
    Some(out)
}

fn derive_check_values(tables: &HashTables, spec: HashSpec, valid_lut: &[bool; 256]) -> Option<[i8; 16]> {
    let (_, buckets, _, _) = build_buckets_and_msb_violations(tables, spec, valid_lut);
    let mut out = [0i8; 16];

    for (h, bucket_bits) in buckets.iter().enumerate() {
        let mut seen_any = false;
        let mut lo = -128i16;
        let mut hi = 127i16;

        for (word_idx, word) in bucket_bits.iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                seen_any = true;
                let byte_idx = word_idx * 64 + bit;
                let b = byte_idx as u8;
                let bi = (b as i8) as i16;
                if valid_lut[byte_idx] {
                    let lower = -bi;
                    if lower > lo {
                        lo = lower;
                    }
                } else {
                    let upper = -bi - 1;
                    if upper < hi {
                        hi = upper;
                    }
                }
            }
        }

        if !seen_any {
            out[h] = 0;
            continue;
        }
        if lo > hi {
            return None;
        }
        out[h] = lo as i8;
    }

    Some(out)
}

fn derive_poison_delta_values_80(
    tables: &HashTables,
    spec: HashSpec,
    valid_lut: &[bool; 256],
    required_delta_lut: &[i8; 256],
) -> Option<[u8; 16]> {
    let (buckets, _, _, _) = build_buckets_and_msb_violations(tables, spec, valid_lut);
    let mut out = [0u8; 16];
    for (h, bucket) in buckets.iter().enumerate() {
        let fixed = bucket_fixed_delta_conflict(bucket, valid_lut, required_delta_lut).ok()?;
        let d = bucket_poison_feasible_delta_80(bucket, valid_lut, fixed)?;
        out[h] = d;
    }
    Some(out)
}

fn format_table_u8_hex(table: &[u8; 16]) -> String {
    table
        .iter()
        .map(|b| format!("0x{b:02x}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_table_i8_dec(table: &[i8; 16]) -> String {
    table
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let restarts = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(16);
    let iters = args
        .get(2)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1200);

    let valid_lut = {
        let mut v = [false; 256];
        for b in u8::MIN..=u8::MAX {
            v[b as usize] = is_base64_valid_strict_simd(b);
        }
        v
    };
    let required_delta_lut = {
        let mut d = [0i8; 256];
        for b in u8::MIN..=u8::MAX {
            if let Some(sextet) = base64_sextet(b) {
                d[b as usize] = sextet as i8 - b as i8;
            }
        }
        d
    };

    let mixes = [
        MixOp::Avg,
        MixOp::Add,
        MixOp::Xor,
        MixOp::Min,
        MixOp::Max,
        MixOp::Sub,
    ];
    let shifts = [1u8, 2, 3, 4];
    let index_modes = [IndexMode::Maskless, IndexMode::Masked0f];
    let families = [
        HashFamily::OneLut,
        HashFamily::TwoLutAdd,
        HashFamily::TwoLutAvg,
        HashFamily::TwoLutXor,
        HashFamily::PredSelBit5,
        HashFamily::PredSelBit6,
        HashFamily::PosAware,
        HashFamily::LoHiAdd,
        HashFamily::DualIndexBit5,
        HashFamily::DualIndexBit6,
    ];
    let mut specs = Vec::new();
    for family in families {
        for index_mode in index_modes {
            for mix in mixes {
                for shift in shifts {
                    specs.push(HashSpec {
                        shift,
                        mix,
                        index_mode,
                        family,
                    });
                }
            }
        }
    }

    println!("Hash re-synthesis (same SIMD budget family)");
    println!("Model: raw = mix(lut0[low_nibble], shifted(byte,k)); optional perturb LUT1; idx tier = maskless|masked_0f");
    println!("mix in {{avg_epu8, add_epi8, xor_si128, min_epu8, max_epu8, sub_epi8}}, families expanded (one/two-lut, predicate-select, pos-aware, lo+hi, dual-index), k in {{1,2,3,4}}");
    println!("Objective: shared-hash strict candidate that is implementable in hot loop");
    println!("PASS_MASKLESS: map_conflict=0, valid_msb_vio=0, ascii_msb_vio=0, impossible=0, extra_ops<=1, const_live_delta<=0");
    println!("PASS_MASKED: map_conflict=0, impossible=0, extra_ops<=1, const_live_delta<=0");
    println!("PASS_EXTENDED: map_conflict=0, impossible/poison_impossible=0, valid_msb_vio=0, ascii_msb_vio=0, extra_ops<=2, const_live_delta<=1");

    let threads = args
        .get(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        });
    println!("restarts={restarts}, iters/restart={iters}, threads={threads}");
    println!();

    let baseline_tables = HashTables {
        primary: BASE_DELTA_ASSO,
        perturb: BASE_DELTA_PERTURB,
        hi: BASE_DELTA_HI,
        pos: BASE_DELTA_POS,
    };

    let results: Vec<(HashSpec, Score, Candidate)> = if threads <= 1 || specs.len() < 2 {
        specs
            .into_iter()
            .map(|spec| {
                let baseline_score =
                    evaluate_table(&baseline_tables, spec, &valid_lut, &required_delta_lut);
                let best = local_search(spec, restarts, iters, &valid_lut, &required_delta_lut);
                (spec, baseline_score, best)
            })
            .collect()
    } else {
        let chunk_size = specs.len().div_ceil(threads);
        let mut out = Vec::new();
        thread::scope(|scope| {
            let mut handles = Vec::new();
            for chunk in specs.chunks(chunk_size) {
                let local_specs = chunk.to_vec();
                let valid = valid_lut;
                let required_delta = required_delta_lut;
                let baseline = baseline_tables;
                handles.push(scope.spawn(move || {
                    let mut local = Vec::with_capacity(local_specs.len());
                    for spec in local_specs {
                        let baseline_score =
                            evaluate_table(&baseline, spec, &valid, &required_delta);
                        let best = local_search(spec, restarts, iters, &valid, &required_delta);
                        local.push((spec, baseline_score, best));
                    }
                    local
                }));
            }
            for h in handles {
                out.extend(h.join().expect("worker thread panicked"));
            }
        });
        out
    };

    let mut best_all: Vec<Candidate> = Vec::with_capacity(results.len());
    for (spec, baseline_score, best) in &results {
        best_all.push(best.clone());
        println!(
            "spec family={} mode={} mix={} shift={} (extra_ops={}, const_live_delta={}) | baseline: map_conflicts={} valid_msb_vio={} ascii_msb_vio={} impossible={} poison_impossible={} mixed={} width={} | best: map_conflicts={} valid_msb_vio={} ascii_msb_vio={} impossible={} poison_impossible={} mixed={} width={}",
            spec.family.name(),
            spec.index_mode.name(),
            spec.mix.name(),
            spec.shift,
            spec.extra_ops(),
            spec.const_live_delta(),
            baseline_score.map_conflict_buckets,
            baseline_score.valid_msb_violations,
            baseline_score.ascii_msb_violations,
            baseline_score.impossible_buckets,
            baseline_score.poison_impossible_buckets,
            baseline_score.mixed_buckets,
            baseline_score.feasible_width_sum,
            best.score.map_conflict_buckets,
            best.score.valid_msb_violations,
            best.score.ascii_msb_violations,
            best.score.impossible_buckets,
            best.score.poison_impossible_buckets,
            best.score.mixed_buckets,
            best.score.feasible_width_sum
        );
        println!("  best tables: {}", format_tables_hex(best.spec, &best.tables));
    }

    best_all.sort_by(|a, b| a.score.cmp_key().cmp(&b.score.cmp_key()));
    println!();
    println!("Top candidates:");
    for (i, c) in best_all.iter().take(5).enumerate() {
        println!(
            "  {}. family={} mode={} mix={} shift={} (extra_ops={}, const_live_delta={}) => map_conflicts={} valid_msb_vio={} ascii_msb_vio={} impossible={} poison_impossible={} mixed={} width={} tables={}",
            i + 1,
            c.spec.family.name(),
            c.spec.index_mode.name(),
            c.spec.mix.name(),
            c.spec.shift,
            c.spec.extra_ops(),
            c.spec.const_live_delta(),
            c.score.map_conflict_buckets,
            c.score.valid_msb_violations,
            c.score.ascii_msb_violations,
            c.score.impossible_buckets,
            c.score.poison_impossible_buckets,
            c.score.mixed_buckets,
            c.score.feasible_width_sum,
            format_tables_hex(c.spec, &c.tables)
        );
    }

    let passing_maskless: Vec<&Candidate> = best_all
        .iter()
        .filter(|c| c.pass_maskless())
        .collect();
    let passing_masked: Vec<&Candidate> = best_all
        .iter()
        .filter(|c| c.pass_masked())
        .collect();
    println!();
    if passing_maskless.is_empty() {
        println!(
            "RESULT: no PASSING MASKLESS shared-hash candidate found."
        );
    } else {
        println!(
            "RESULT: found {} PASSING MASKLESS shared-hash candidate(s).",
            passing_maskless.len()
        );
        println!("PASSING MASKLESS candidates (with derived tables):");
        for (i, c) in passing_maskless.iter().enumerate() {
            let delta_values =
                derive_delta_values(&c.tables, c.spec, &valid_lut, &required_delta_lut)
                .expect("passing candidate must derive delta_values");
            let check_values = derive_check_values(&c.tables, c.spec, &valid_lut)
                .expect("passing candidate must derive check_values");
            println!(
                "  {}. family={} mode={} mix={} shift={} tables={}",
                i + 1,
                c.spec.family.name(),
                c.spec.index_mode.name(),
                c.spec.mix.name(),
                c.spec.shift,
                format_tables_hex(c.spec, &c.tables)
            );
            println!("     DELTA_VALUES(i8): [{}]", format_table_i8_dec(&delta_values));
            println!("     CHECK_VALUES(i8): [{}]", format_table_i8_dec(&check_values));
        }
    }

    println!();
    if passing_masked.is_empty() {
        println!("RESULT: no PASSING MASKED shared-hash candidate found (masked_0f tier).");
    } else {
        println!(
            "RESULT: found {} PASSING MASKED shared-hash candidate(s).",
            passing_masked.len()
        );
        println!("Top MASKED candidates:");
        for (i, c) in passing_masked.iter().take(5).enumerate() {
            println!(
                "  {}. family={} mode={} mix={} shift={} tables={} score: map_conflicts={} impossible={} mixed={} width={} valid_msb_vio={} ascii_msb_vio={}",
                i + 1,
                c.spec.family.name(),
                c.spec.index_mode.name(),
                c.spec.mix.name(),
                c.spec.shift,
                format_tables_hex(c.spec, &c.tables),
                c.score.map_conflict_buckets,
                c.score.impossible_buckets,
                c.score.mixed_buckets,
                c.score.feasible_width_sum,
                c.score.valid_msb_violations,
                c.score.ascii_msb_violations
            );
        }
    }

    let passing_shared_extended: Vec<&Candidate> =
        best_all.iter().filter(|c| c.pass_shared_extended()).collect();
    if passing_shared_extended.is_empty() {
        println!("RESULT: no PASSING EXTENDED shared-hash candidate found.");
    } else {
        println!(
            "RESULT: found {} PASSING EXTENDED shared-hash candidate(s).",
            passing_shared_extended.len()
        );
    }

    let mut by_poison = best_all.clone();
    by_poison.sort_by(|a, b| {
        (
            a.score.map_conflict_buckets,
            a.score.poison_impossible_buckets,
            a.score.valid_msb_violations,
            a.score.ascii_msb_violations,
            a.score.mixed_buckets,
            -(a.score.feasible_width_sum as i32),
        )
            .cmp(&(
                b.score.map_conflict_buckets,
                b.score.poison_impossible_buckets,
                b.score.valid_msb_violations,
                b.score.ascii_msb_violations,
                b.score.mixed_buckets,
                -(b.score.feasible_width_sum as i32),
            ))
    });

    println!();
    println!("Top candidates (poison objective, mask=0x80 robust):");
    for (i, c) in by_poison.iter().take(5).enumerate() {
        println!(
            "  {}. family={} mode={} mix={} shift={} => map_conflicts={} poison_impossible={} valid_msb_vio={} ascii_msb_vio={} mixed={} width={} tables={}",
            i + 1,
            c.spec.family.name(),
            c.spec.index_mode.name(),
            c.spec.mix.name(),
            c.spec.shift,
            c.score.map_conflict_buckets,
            c.score.poison_impossible_buckets,
            c.score.valid_msb_violations,
            c.score.ascii_msb_violations,
            c.score.mixed_buckets,
            c.score.feasible_width_sum,
            format_tables_hex(c.spec, &c.tables)
        );
    }

    let passing_poison_maskless: Vec<&Candidate> = by_poison
        .iter()
        .filter(|c| c.pass_poison_maskless())
        .collect();
    let passing_poison_masked: Vec<&Candidate> = by_poison
        .iter()
        .filter(|c| c.pass_poison_masked())
        .collect();

    println!();
    if passing_poison_maskless.is_empty() {
        println!("RESULT: no PASSING MASKLESS poisoned-mapping candidate found.");
    } else {
        println!(
            "RESULT: found {} PASSING MASKLESS poisoned-mapping candidate(s).",
            passing_poison_maskless.len()
        );
        for (i, c) in passing_poison_maskless.iter().enumerate() {
            let delta_values = derive_poison_delta_values_80(
                &c.tables,
                c.spec,
                &valid_lut,
                &required_delta_lut,
            )
                .expect("poison passing candidate must derive delta table");
            println!(
                "  {}. family={} mode={} mix={} shift={} tables={}",
                i + 1,
                c.spec.family.name(),
                c.spec.index_mode.name(),
                c.spec.mix.name(),
                c.spec.shift,
                format_tables_hex(c.spec, &c.tables)
            );
            println!("     DELTA_VALUES_POISON(u8): [{}]", format_table_u8_hex(&delta_values));
        }
    }

    if passing_poison_masked.is_empty() {
        println!("RESULT: no PASSING MASKED poisoned-mapping candidate found.");
    } else {
        println!(
            "RESULT: found {} PASSING MASKED poisoned-mapping candidate(s).",
            passing_poison_masked.len()
        );
        for (i, c) in passing_poison_masked.iter().take(5).enumerate() {
            let delta_values = derive_poison_delta_values_80(
                &c.tables,
                c.spec,
                &valid_lut,
                &required_delta_lut,
            )
                .expect("poison passing candidate must derive delta table");
            println!(
                "  {}. family={} mode={} mix={} shift={} tables={}",
                i + 1,
                c.spec.family.name(),
                c.spec.index_mode.name(),
                c.spec.mix.name(),
                c.spec.shift,
                format_tables_hex(c.spec, &c.tables)
            );
            println!("     DELTA_VALUES_POISON(u8): [{}]", format_table_u8_hex(&delta_values));
        }
    }

    let passing_poison_extended: Vec<&Candidate> =
        by_poison.iter().filter(|c| c.pass_poison_extended()).collect();
    if passing_poison_extended.is_empty() {
        println!("RESULT: no PASSING EXTENDED poisoned-mapping candidate found.");
    } else {
        println!(
            "RESULT: found {} PASSING EXTENDED poisoned-mapping candidate(s).",
            passing_poison_extended.len()
        );
        for (i, c) in passing_poison_extended.iter().take(5).enumerate() {
            let delta_values = derive_poison_delta_values_80(
                &c.tables,
                c.spec,
                &valid_lut,
                &required_delta_lut,
            )
                .expect("extended poison candidate must derive delta table");
            println!(
                "  {}. family={} mode={} mix={} shift={} (extra_ops={}, const_live_delta={}) tables={}",
                i + 1,
                c.spec.family.name(),
                c.spec.index_mode.name(),
                c.spec.mix.name(),
                c.spec.shift,
                c.spec.extra_ops(),
                c.spec.const_live_delta(),
                format_tables_hex(c.spec, &c.tables)
            );
            println!("     DELTA_VALUES_POISON(u8): [{}]", format_table_u8_hex(&delta_values));
        }
    }
}
