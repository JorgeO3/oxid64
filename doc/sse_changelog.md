# SSE/SSSE3 Experiment Changelog

Last updated: 2026-02-27
Owner: `simd/ssse3*` optimization work

## Purpose

Track SIMD experiments so we do not repeat low-ROI changes.
Each entry records:

- exact change
- benchmark metrics (before/after)
- outcome (`success`, `noise`, `regression`)
- most likely cause
- decision (`keep`, `revert`, `park`)

## Canonical Commands

Use these commands for comparable numbers:

```bash
taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/Rust Port (SSSE3, C-Style Strict)/1048576" --exact --noplot
taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/Rust Port (SSSE3, C-Style)/1048576" --exact --noplot
taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/Rust Port (SSSE3, C-Style Strict Range)/1048576" --exact --noplot
taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/TurboBase64 C (SSE, B64CHECK)/1048576" --exact --noplot
taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/TurboBase64 C (SSE, NB64CHECK)/1048576" --exact --noplot
```

Assembly and profiling:

```bash
cargo asm --release --bench base64_bench 1889 > /tmp/asm_cstyle.s
cargo asm --release --bench base64_bench 1890 > /tmp/asm_strict.s
perf stat -r 3 -e cycles,instructions,branches,branch-misses,cache-misses -- taskset -c 0 target/release/deps/base64_bench-* --bench "Base64 Decoding/Rust Port (SSSE3, C-Style Strict)/1048576" --exact --noplot
perf record -F 999 -- taskset -c 0 target/release/deps/base64_bench-* --bench "Base64 Decoding/Rust Port (SSSE3, C-Style Strict)/1048576" --exact --noplot
perf report --stdio --no-children
```

## Baseline Snapshot (2026-02-27)

Sequential (not parallel) runs:

- Rust SSSE3 C-Style Strict: `~7.89 GiB/s` (`[7.8823, 7.9024]`)
- Rust SSSE3 C-Style: `~10.47 GiB/s` (`[10.419, 10.503]`)
- Rust SSSE3 C-Style Strict Range: `~5.21 GiB/s` (`[5.1874, 5.2193]`)
- TurboBase64 C SSE B64CHECK: `~7.49 GiB/s` (`[7.4816, 7.5019]`)
- TurboBase64 C SSE NB64CHECK: `~12.19 GiB/s` (`[12.133, 12.224]`)

Current strict assembly/perf shape:

- `decode_base64_ssse3_cstyle_strict` (`cargo asm` symbol 1890)
- function-level count: `pmovmskb=3`, `pshufb=65`
- no XMM stack spills seen in hot loop
- `perf report`: ~90%+ cycles inside strict decode kernel
- `perf annotate`: hottest ops are `pshufb`, `pmaddubsw`, `pmaddwd`, and `movdqu` stores

## Experiment Log

### SSE-2026-02-27-012

- Change: expanded `sse_hash_resynth` search-space and added poisoned-mapping objective (kernel-free wave).
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- Search-space extensions:
  - index tiers: `maskless`, `masked_0f` (1 extra op budget)
  - mix ops: `avg_epu8`, `add_epi8`, `xor_si128`, `min_epu8`, `max_epu8`, `sub_epi8`
  - shifts: `k in {1,2,3,4}`
  - gates/budgets:
    - `PASS_MASKLESS(shared)`: `map_conflicts=0`, `valid_msb_vio=0`, `ascii_msb_vio=0`, `impossible=0`, `extra_ops<=1`, `const_live_delta<=0`
    - `PASS_MASKED(shared)`: `map_conflicts=0`, `impossible=0`, `extra_ops<=1`, `const_live_delta<=0`
    - `PASS_MASKLESS(poison)`: `map_conflicts=0`, `valid_msb_vio=0`, `ascii_msb_vio=0`, `poison_impossible=0`
    - `PASS_MASKED(poison)`: `map_conflicts=0`, `poison_impossible=0`
- Run:
  - `cargo run --release --bin sse_hash_resynth -- 48 3000`
  - log: `/tmp/current_resynth_wave_poison_48x3000.txt`
  - `cargo run --release --bin sse_hash_resynth -- 96 6000`
  - log: `/tmp/current_resynth_wave_poison_96x6000.txt`
- Result:
  - `no PASSING MASKLESS shared-hash`
  - `no PASSING MASKED shared-hash`
  - `no PASSING MASKLESS poisoned-mapping`
  - `no PASSING MASKED poisoned-mapping`
  - In larger wave (`96x6000`) best candidates still stall at:
    - shared: `impossible=3`
    - poison: `poison_impossible=3`
- Best observed (still failing):
  - shared top (masked avg shift=2): `map_conflicts=0`, `impossible=3`
  - poison top (maskless xor shift=3): `map_conflicts=0`, `poison_impossible=3`
- Outcome: `regression-risk avoided / blocked in current family`
- Most likely cause:
  - Even with richer mix/index tiers, this hash family cannot eliminate the last conflicting buckets (`impossible` and `poison_impossible` remain > 0), so no strict-by-construction candidate is implementable under current constraints.
- Decision: `pivot`
  - stop iterating this exact family unless a substantially richer partition model is introduced (e.g., two-LUT perturbation or different hash construction).

### SSE-2026-02-27-013

- Change: expanded `sse_hash_resynth` with `Two-LUT perturbation` families and reran release waves.
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- New model axes:
  - hash families:
    - `one_lut`
    - `two_lut_add` (`core + lut1[lo]`)
    - `two_lut_avg` (`avg(core, lut1[lo])`)
  - existing axes kept:
    - `index_mode in {maskless, masked_0f}`
    - `mix in {avg, add, xor, min, max, sub}`
    - `k in {1,2,3,4}`
- Budgets:
  - `extra_ops` and `const_live_delta` now computed at `HashSpec` level (family + index mode).
  - PASS gates unchanged (`shared` and `poison`) with budget checks.
- Runs:
  - smoke: `cargo run --release --bin sse_hash_resynth -- 24 2000`
    - log: `/tmp/current_resynth_two_lut_smoke.txt`
  - wave: `cargo run --release --bin sse_hash_resynth -- 48 3000`
    - log: `/tmp/current_resynth_two_lut_48x3000.txt`
- Result:
  - `no PASSING MASKLESS shared-hash`
  - `no PASSING MASKED shared-hash`
  - `no PASSING MASKLESS poisoned-mapping`
  - `no PASSING MASKED poisoned-mapping`
  - best frontier improved from previous family:
    - shared objective reached `impossible=2` (was stuck at `3`)
    - poison objective reached `poison_impossible=2` (was stuck at `3`)
- Best observed candidates (still failing):
  - shared top:
    - `family=two_lut_avg`, `mode=maskless`, `mix=avg`, `k=2`
    - `map_conflicts=0`, `valid_msb_vio=0`, `ascii_msb_vio=0`, `impossible=2`, `poison_impossible=2`
  - poison top:
    - same frontier also appears with `poison_impossible=2`
- Outcome: `near miss / still blocked`
- Most likely cause:
  - one extra LUT increases expressivity but still cannot resolve the final 2 conflicting buckets under current single-index bucket model and strict sign constraints.
- Decision: `pivot again`
  - next family must add a different partition primitive (e.g., predicate-selected LUT or context-aware split), not more random search over the same bucket mechanism.

### SSE-2026-02-27-014

- Change: expanded re-synthesis with all discussed high-ROI families + parallel execution by `spec`.
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- Added families:
  - `one_lut`
  - `two_lut_add`
  - `two_lut_avg`
  - `two_lut_xor`
  - `predsel_bit5`
  - `predsel_bit6`
  - `pos_aware`
  - `lo_hi_add`
  - `dual_index_bit5`
  - `dual_index_bit6`
- Added model/runtime features:
  - parallel search across specs (`threads` arg, default = `available_parallelism`)
  - shared objective now uses `check_buckets` (supports dual-index families)
  - `PASS_EXTENDED` tiers:
    - shared: `map_conflict=0`, `impossible=0`, `valid_msb_vio=0`, `ascii_msb_vio=0`, `extra_ops<=2`, `const_live_delta<=1`
    - poison: `map_conflict=0`, `poison_impossible=0`, `valid_msb_vio=0`, `ascii_msb_vio=0`, `extra_ops<=2`, `const_live_delta<=1`
- Run:
  - `cargo run --release --bin sse_hash_resynth -- 48 3000 $(nproc)`
  - log: `/tmp/current_resynth_all_families_48x3000.txt`
- Result:
  - `PASSING MASKLESS shared-hash`: **FOUND (1)**
  - `PASSING MASKED shared-hash`: none
  - `PASSING EXTENDED shared-hash`: **FOUND (1)**
  - `PASSING poisoned-mapping` (maskless/masked/extended): none
- Best shared candidate (top-1):
  - `family=predsel_bit6`, `mode=maskless`, `mix=avg_epu8`, `shift=2`
  - `extra_ops=1`, `const_live_delta=0`
  - `map_conflicts=0`, `valid_msb_vio=0`, `ascii_msb_vio=0`, `impossible=0`
  - `poison_impossible=3` (poison objective still not solved)
  - tables:
    - `primary=[01 01 01 01 01 01 01 01 00 00 21 19 00 01 00 17]`
    - `perturb=[00 00 00 00 00 00 00 00 00 00 00 02 00 00 00 00]`
- Outcome: `success` for shared-hash feasibility under expanded families.
- Most likely cause:
  - added predicate-split partition (`predsel_bit6`) unlocked the last `shared` conflicts that were stuck at `impossible>0`.
- Decision: `next`
  - Do a minimal kernel spike for this single PASSING shared candidate (keep loop shape, replace check-hash path only) and validate asm/perf.

### SSE-2026-02-27-001

- Change: strict path pairwise check reduction (`check_mask_bits_pair`), `m01|m23` in DS64.
- Scope: `src/simd/ssse3_cstyle.rs` (`process_ds64_strict`).
- Before: `~7.90 GiB/s`
- After: `~7.93 GiB/s`
- Outcome: `noise` (marginal, not statistically meaningful).
- Most likely cause: LLVM already generated a very similar loop shape; no meaningful asm shift.
- Decision: `keep` (no regression, but no real win).

### SSE-2026-02-27-002

- Change: force 1 movemask per DS64 (quad OR of checks).
- Scope: strict DS64 check fold experiment.
- Before: `~7.9 GiB/s`
- After: `~6.9-7.1 GiB/s` (observed around `~7.03 GiB/s` in controlled runs).
- Outcome: `regression` (large).
- Most likely cause: increased XMM live-set triggered RA fallout (more constant reload pressure/worse scheduling).
- Decision: `revert` (rolled back).

### SSE-2026-02-27-003

- Change: RA-first refactor in strict path:
  - late-load second pair (`iv0/iv1`)
  - compute shifted in caller
  - strict uses `map_and_pack_with_shifted`.
- Scope: `src/simd/ssse3_cstyle.rs`.
- Before: strict in `~7.88-7.93 GiB/s` band.
- After: strict remains in `~7.82-7.93 GiB/s` band; non-strict remains `~10.3-10.5 GiB/s`.
- Outcome: `noise` (no clear speedup, no structural regression).
- Most likely cause: main bottleneck still shuffle/madd/store throughput, not reduction glue.
- Decision: `keep` (stable, cleaner strict structure).

### SSE-2026-02-27-004

- Change: strict range-check variant (`C-Style Strict Range`).
- Scope: `decode_base64_ssse3_cstyle_strict_range`.
- Reference compare: strict table-check path.
- Strict table-check: `~7.89 GiB/s`
- Strict range-check: `~5.21 GiB/s`
- Outcome: `regression` for throughput target.
- Most likely cause: compare-based validity path added instruction pressure without reducing dominant decode cost.
- Decision: `park` (keep for research/reference, not default).

### SSE-2026-02-27-005

- Change: inline asm micro-kernel trial (historical).
- Context: older SSSE3 decoder optimization track.
- Before/After (historical note): `~7.6 -> ~8.0 GiB/s` (~`+5.3%`).
- Outcome: `success` in that branch, but not retained as current default.
- Most likely cause: tighter register/control of hot sequence.
- Decision: `park` (use only if intrinsics path plateaus and asm maintenance is acceptable).

### SSE-2026-02-27-006

- Change: added experimental SSE4.1 strict variants in separate file:
  - `Ssse3CStyleStrictSse41PtestMaskDecoder`
  - `Ssse3CStyleStrictSse41PtestNoMaskDecoder`
- Scope:
  - `src/simd/ssse3_cstyle_experiments.rs`
  - `benches/base64_bench.rs` (new benchmark labels)
  - `tests/sse_decode_tests.rs` (equivalence checks)
- Benchmark labels:
  - `Rust Port (SSSE3+SSE4.1, C-Style Strict PTEST Mask)`
  - `Rust Port (SSSE3+SSE4.1, C-Style Strict PTEST NoMask)`
- Before (strict baseline, pinned): `~7.86 GiB/s` (`[7.8171, 7.8943]`)
- After (SSE4.1 PTEST Mask, pinned): `~7.76 GiB/s` (`[7.7270, 7.7744]`)
- After (SSE4.1 PTEST NoMask, pinned): `~7.61 GiB/s` (`[7.5860, 7.6309]`)
- Outcome: `regression`.
- Most likely cause:
  - `PTEST Mask` compiles to `ptest xmm, [rip+const]` in the hot loop (extra loop-time constant memory operand).
  - `PTEST NoMask` did not lower to a true `ptest` path in the hot loop; codegen stayed close to `pmovmskb` and added extra scalar glue.
  - Dominant hot mix (`pshufb`, `pmadd*`, stores) stayed effectively unchanged.
- Decision: `park` (keep as documented negative result; do not use as default).

### SSE-2026-02-27-007

- Change: added strict-check table synthesis probe for the `shared-hash` idea.
- Scope:
  - `src/bin/sse_strict_table_synth.rs`
- Goal:
  - Verify if a single `check_values2[16]` (indexed by current `delta_hash` nibble) can classify strict-valid vs invalid bytes via:
    - `sign(sat_add_i8(check_values2[h], input_byte)) == invalid_byte`
- Before:
  - Hypothesis: reuse `delta_hash` and remove `check_asso` path could be feasible and high-ROI.
- After (synthesis run):
  - Command: `cargo run --bin sse_strict_table_synth`
  - Result: `NOT FEASIBLE` under current hash model.
  - Impossible buckets: `[3, 4, 6, 8]`
  - Feasible buckets exist for others, but global 16-entry solution does not exist.
- Outcome: `regression-risk avoided` (algorithmic dead-end detected before kernel rewrite).
- Most likely cause:
  - Current `delta_hash` bucket partition mixes valid/invalid bytes in conflicting ways for a single signed-saturating-add threshold table.
- Decision: `park` this exact shared-hash-single-table variant and move to richer designs (poisoned mapping / multi-stage validation) only.

### SSE-2026-02-27-008

- Change: added poisoned-mapping synthesis probe with fixed `DELTA_ASSO` and synthesized `DELTA_VALUES`.
- Scope:
  - `src/bin/sse_poisoned_map_synth.rs`
- Goal:
  - Check if strict validation can be fused into mapping by keeping exact valid sextets and forcing poison bits on invalid bytes.
  - Tested poison masks:
    - `0x80` (sign bit)
    - `0xC0` (high two bits)
- Before:
  - Hypothesis: algorithmic strict-path win by removing explicit check shuffle pipeline.
- After (synthesis run):
  - Command: `cargo run --bin sse_poisoned_map_synth`
  - Current table baseline:
    - mask `0x80`: `valid_bad=0/64`, `invalid_unpoisoned=90/192`
    - mask `0xC0`: `valid_bad=0/64`, `invalid_unpoisoned=58/192`
  - Synth result with current hash partition:
    - mask `0x80`: `NOT FEASIBLE`
      - unsatisfied buckets: `3,4,6,7,8,10,12`
    - mask `0xC0`: `NOT FEASIBLE`
      - unsatisfied buckets: `3,4,6,7,8,10`
- Outcome: `regression-risk avoided` (infeasible design detected before kernel rewrite).
- Most likely cause:
  - With current `delta_hash` partition, valid and invalid bytes collide in buckets with incompatible constraints, so no single-byte delta per bucket can satisfy both exact sextet mapping and guaranteed poisoning.
- Decision: `park` this fixed-hash poisoned-mapping variant. Any future poisoned approach likely needs a different hash partition and/or multi-stage mapping.

### SSE-2026-02-27-009

- Change: added split-predicate synthesis probe (`idx=(delta_hash<<1)|pred`) for both:
  - Option1: bucket-validity lookup
  - Option2: split sat-add tables (`check0[16]`, `check1[16]`)
- Scope:
  - `src/bin/sse_split_predicate_synth.rs`
- Goal:
  - Test whether adding one cheap predicate bit is enough to break current bucket conflicts.
- Predicates tested:
  - `bit7`, `bit6`, `bit5`, `bit4`, `bit3`
  - `bit6_xor_bit5`, `bit6_or_bit5`, `bit6_and_bit5`
  - `bit5_xor_bit4`
- After (synthesis run):
  - Command: `cargo run --bin sse_split_predicate_synth`
  - Option1 (`bucket-validity`): `NOT FEASIBLE` for all predicates
    - mixed bucket count range: `7..11`
  - Option2 (`split sat-add`): `NOT FEASIBLE` for all predicates
    - impossible bucket count: `4` for every predicate tested
  - Best candidate by conflict count (`bit7`) still fails:
    - Option1 mixed buckets: `[6, 8, 10, 12, 14, 16, 20]`
    - Option2 impossible buckets: `[6, 8, 12, 16]`
- Outcome: `regression-risk avoided` (one-bit split proven insufficient before kernel rewrite).
- Most likely cause:
  - Current hash partition + one extra predicate bit still leaves unresolved mixed/conflicting buckets.
- Decision: `park` one-bit split design; next feasible direction is richer partition (>=2 predicate bits and/or re-synthesized hash) or multi-stage fallback for conflict buckets.

### SSE-2026-02-27-010

- Change: added 2-bit split synthesis probe (`idx=(delta_hash<<2)|pred2`) over pairs of cheap predicates.
- Scope:
  - `src/bin/sse_split2_predicate_synth.rs`
- Goal:
  - Test whether 64 logical buckets (4-way split per hash nibble) are enough to make:
    - Option1 feasible (pure bucket-validity lookup)
    - Option2 feasible (split sat-add tables, 4x16 entries)
- Predicate pool:
  - `bit7`, `bit6`, `bit5`, `bit4`, `bit3`
  - `bit6_xor_bit5`, `bit6_or_bit5`, `bit6_and_bit5`
  - `bit5_xor_bit4`
- After (synthesis run):
  - Command: `cargo run --bin sse_split2_predicate_synth`
  - Tested pairs: `36`
  - Option1 feasible pairs: `0`
  - Option2 feasible pairs: `0`
  - Best candidate (`bit4 | bit3`) still fails:
    - Option1 mixed buckets: `[14, 19, 20, 27, 28, 35, 42]`
    - Option2 impossible buckets: `[14, 19, 27, 35]`
- Outcome: `regression-risk avoided` (2-bit cheap split still insufficient under current hash model).
- Most likely cause:
  - Conflicts are structural to the current hash partition; adding cheap predicate bits without changing hash cannot fully separate valid/invalid classes.
- Decision: `park` fixed-hash split strategy up to 2 bits. Next direction should be hash re-synthesis and/or conflict-bucket fallback path.

### SSE-2026-02-27-011

- Change: added experimental strict variant `SSSE3+SSE4.1, C-Style Strict Arith Check`.
- Scope:
  - `src/simd/ssse3_cstyle_experiments.rs`
  - `benches/base64_bench.rs`
  - `tests/sse_decode_tests.rs`
- Design:
  - Keep current Turbo-style decode map+pack (`delta_asso/delta_values` + `pmadd*` + `cpv`).
  - Replace strict check path (`check_asso/check_values` shuffles) with arithmetic SIMD classifier:
    - valid set: `[A-Za-z0-9+/]`
    - invalid mask built via `subs_epu8 + cmpeq + andnot`
    - reduction via `movemask` (pair-wise per DS64).
- Validation:
  - `cargo check` ✅
  - `cargo test --test sse_decode_tests` ✅ (3/3)
- Bench metrics:
  - Initial run (v1 arith, psubusb-heavy):
    - `~4.37 GiB/s` (`[4.3477, 4.3885]`) regression vs strict base.
  - Revised run (v2 arith, lower-constant checker):
    - `~5.08 GiB/s` (`[5.0153, 5.1147]`).
  - Revised run (v3 arith, removed 2x DS64 outer unroll for strict-arith only):
    - `~5.12 GiB/s` (`[5.1129, 5.1251]`) vs v2: no statistically significant change.
  - Same-build strict baseline:
    - `~7.85 GiB/s` (`[7.8373, 7.8527]`).
- ASM notes:
  - `arith v2`: `pshufb=39`, `pmaddubsw=13`, `pmaddwd=13`, `movdqu=28`, `pmovmskb=3`
  - but many extra classifier ops (`pminub/pcmpeqb/paddb`) and frequent loop-body `.LCPI` constant reloads.
  - `strict base`: `pshufb=65`, `pmadd*=13/13`, `movdqu=28`, lower extra-op pressure and cleaner constant residency.
- Outcome: `regression` (material).
- Most likely cause:
  - Although check-side shuffles were reduced, arithmetic classifier added too many extra ops and increased register pressure.
  - LLVM emitted repeated constant reloads in the hot loop for arith variant, offsetting shuffle savings.
- Decision: `park` (documented negative result; not candidate for default).

### SSE-2026-02-27-012

- Change: new isolated file + prototype `Hybrid Buckets` strict checker.
- Scope:
  - `src/simd/ssse3_cstyle_experiments_hybrid.rs` (copy-based work, original experiments file untouched)
  - `src/simd/mod.rs`
  - `benches/base64_bench.rs`
  - `tests/sse_decode_tests.rs`
- Design:
  - Keep current decode map+pack path (`delta_asso/delta_values` + `pmadd*`).
  - Reuse `delta_hash` from mapping.
  - Fast path: shared-hash signed-saturating table check (`fast_check_values`) for feasible buckets.
  - Fallback path: original strict check (`check_asso/check_values`) only on conflict buckets.
  - Conflict buckets (from synthesis): `[3, 4, 6, 8]`.
- Validation:
  - `cargo check` ✅
  - `cargo test --test sse_decode_tests` ✅ (3/3)
- Bench metrics:
  - `Rust Port (SSSE3+SSE4.1, C-Style Strict Hybrid Buckets)/1048576`:
    - `~3.50 GiB/s` (`[3.4972, 3.5119]`) in repeated runs.
  - Baseline compare:
    - strict base is around `~7.84 GiB/s`.
- Outcome: `regression` (large).
- Most likely cause:
  - Conflict buckets are too frequent for valid base64 in this hash model.
  - Measured valid tuple conflict-rate with current hash/bucket model: `~51.56%` (`825/1600`).
  - At 16-byte vectors this makes fallback effectively always-on, so hybrid pays:
    - fast check cost
    - conflict detection cost
    - plus full slow check cost almost every iteration.
- Decision: `park` (discard as performance candidate; keep as negative evidence).

### SSE-2026-02-27-013

- Change: added hash re-synthesis tooling for richer same-budget hash families (no kernel changes yet).
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- Design:
  - Search family: `h = mix(asso[low_nibble], shifted(byte,k)) & 0x0f`
  - `mix ∈ {avg_epu8, add_epi8, xor_si128}`
  - `k ∈ {2,3,4,5}`
  - Optimized objective for single-table/shared-check feasibility under sat-add sign model:
    - minimize `impossible_buckets`
    - then minimize `mixed_buckets`
    - then maximize feasible threshold interval width.
- Validation:
  - `cargo run --bin sse_hash_resynth` ✅
- Results (restarts=16, iters/restart=1200):
  - Found fully-feasible candidates (`impossible_buckets == 0`) for multiple specs.
  - Top candidates:
    1. `mix=avg_epu8, shift=3, impossible=0, mixed=5`
       - table: `[90 ac 4c 8b 4c 6b 0b 8c cb 2a 08 51 cc ed 6c f1]`
    2. `mix=add_epi8, shift=4, impossible=0, mixed=6`
       - table: `[4a 7a 6a 8a 3a 7a 9a 4a 7a 2a f9 5b cd ad 1d bb]`
    3. `mix=add_epi8, shift=2, impossible=0, mixed=6`
       - table: `[3d 26 b6 a6 65 05 a5 7d 0c 9c 78 ff f6 e6 b6 96]`
  - `RESULT: found 7 fully-feasible candidate(s)`.
- Outcome: `success` (tooling goal achieved; new viable partitions discovered).
- Most likely cause:
  - Previous infeasibility was tied to the fixed current hash family.
  - Expanding hash family (same op budget) removed structural bucket conflicts.
- Decision: `keep` and proceed to targeted kernel spike with top candidate (`avg_epu8, shift=3`) in an isolated experiment file.

## Non-Duplication Rule

Before creating a new entry:

1. Search this file for similar idea (`pairwise`, `movemask`, `range`, `inline asm`, `late-load`).
2. If same idea already exists, append a new run to that entry instead of opening a new one.
3. Open a new entry only when the algorithmic idea changes.

Suggested entry ID format:

- `SSE-YYYY-MM-DD-NNN`

## Entry Template

```md
### SSE-YYYY-MM-DD-NNN
- Change:
- Scope:
- Before:
- After:
- Outcome: success | noise | regression
- Most likely cause:
- Decision: keep | revert | park
- Evidence:
  - Bench command:
  - ASM notes:
  - Perf notes:
```

### SSE-2026-02-27-014

- Change: added new strict experimental variant `SSSE3+SSE4.1, C-Style Strict Resynth Add4` using resynth hash candidate #2.
- Scope:
  - `src/simd/ssse3_cstyle_experiments_hybrid.rs`
  - `benches/base64_bench.rs`
  - `tests/sse_decode_tests.rs`
  - `src/bin/sse_resynth_check_values.rs`
- Design:
  - Keep current decode map+pack path (`delta_asso/delta_values` + `pmadd*` + `cpv`).
  - Replace strict-check hash family with candidate #2 from hash re-synthesis:
    - `mix=add_epi8`
    - `shift=4`
    - `check_asso = [4a 7a 6a 8a 3a 7a 9a 4a 7a 2a f9 5b cd ad 1d bb]`
  - New helper check path uses nibble-masked hash before `pshufb` (`hash & 0x0f`) to keep shuffle index semantics aligned with synthesis model.
  - New synthesized `check_values` table (sat-add sign model):
    - `[-97, 20, -128, -11, -127, -37, -45, -53, -61, -69, -77, -85, -93, -29, -65, -61]`
- Validation:
  - `cargo run --bin sse_resynth_check_values -- add 4 "4a 7a 6a 8a 3a 7a 9a 4a 7a 2a f9 5b cd ad 1d bb"` ✅ (`RESULT: FEASIBLE`)
  - `cargo check` ✅
  - `cargo test --test sse_decode_tests` ✅ (3/3)
- Bench metrics (sequential, pinned core):
  - `taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Resynth A3)/1048576" --exact --noplot`
    - `~7.88 GiB/s` (`[7.8598, 7.9036]`)
  - `taskset -c 0 cargo bench --bench base64_bench -- "Base64 Decoding/Rust Port (SSSE3+SSE4.1, C-Style Strict Resynth Add4)/1048576" --exact --noplot`
    - `~6.95 GiB/s` (`[6.9409, 6.9621]`)
  - Delta vs A3: `~ -11.8%` throughput.
- ASM notes (`cargo asm --release --bench base64_bench`):
  - A3 vs Add4 instruction mix:
    - `pshufb`: `65` vs `65`
    - `pmaddubsw/pmaddwd`: `13/13` vs `13/13`
    - `pmovmskb`: `3` vs `3`
    - extra in Add4: `psrld 26` (vs `13`), `pand 13` (vs `0`), `paddb 26` (vs `13`), `movdqa 102` (vs `86`)
- Outcome: `regression`.
- Most likely cause:
  - Add4 check hash requires additional per-vector ops (`>>4`, `+`, `&0x0f`) that are not reused from map path; this increases uops/register pressure without reducing dominant shuffle/pack/store work.
- Decision: `park` Add4 as non-candidate for default strict path.

### SSE-2026-02-27-015

- Change: added automated strict-variant harness for asm+criterion+perf validation.
- Scope:
  - `scripts/sse_strict_harness.sh`
- Design:
  - Variant-key driven runner (`strict`, `resynth_a3`, `resynth_add4`, `hybrid`, `arith`, `ptest_mask`, `ptest_nomask`).
  - For each variant, in series:
    1. `cargo asm` dump for exact symbol (auto-resolves cargo-asm index ambiguity).
    2. instruction counts (`pshufb`, `pmaddubsw`, `pmaddwd`, `pmovmskb`, `movdqu`, `movdqa`).
    3. quick asm gates (`LCPI_refs`, `rsp_refs`, `xmm_stack_refs`).
    4. `cargo bench --bench base64_bench -- "<label>/<size>" --exact --noplot` pinned with `taskset`.
    5. optional `perf stat` on bench binary (`cycles,instructions,branches,branch-misses,cache-misses`).
  - Generates a single markdown report with per-variant logs + summary table.
- Validation:
  - Smoke (asm-only):
    - `scripts/sse_strict_harness.sh --variant strict --skip-bench --skip-perf --out-dir /tmp/sse_harness_test4` ✅
  - Smoke (default variants, no perf):
    - `scripts/sse_strict_harness.sh --skip-perf --out-dir /tmp/sse_harness_smoke` ✅
  - Smoke (bench+perf, no asm):
    - `scripts/sse_strict_harness.sh --variant strict --skip-asm --out-dir /tmp/sse_harness_perf_smoke` ✅
- Outcome: `success` (tooling).
- Most likely impact:
  - Faster high-signal iteration and earlier discard of low-ROI variants via consistent asm/bench/perf evidence.
- Decision: `keep` as default validation harness for strict SSE experiments.

### SSE-2026-02-27-016

- Change: added SSE4.2 strict experimental variant using `PCMPESTRM` range classification.
- Scope:
  - `src/simd/ssse3_cstyle_experiments_hybrid.rs`
  - `benches/base64_bench.rs`
  - `tests/sse_decode_tests.rs`
  - `scripts/sse_strict_harness.sh` (new variant key: `sse42_pcmpestrm`)
- Design:
  - New decoder: `Ssse3CStyleStrictSse42PcmpestrmDecoder`.
  - New kernel: `decode_base64_ssse3_cstyle_strict_sse42_pcmpestrm`.
  - Decode datapath unchanged (Turbo-style map+pack).
  - Strict check replaced by SSE4.2 range compare:
    - `valid_mask = _mm_cmpestrm(ranges, 10, iv, 16, _SIDD_UBYTE_OPS|_SIDD_CMP_RANGES|_SIDD_UNIT_MASK)`
    - `invalid_mask = ~valid_mask`
    - reduction via `movemask`.
  - Ranges encoded: `[A-Z], [a-z], [0-9], [+,+], [/,/]`.
- Validation:
  - `cargo check` ✅
  - `cargo test --test sse_decode_tests` ✅ (3/3)
- Harness benchmark metrics (pinned, in series):
  - strict base: `~7.84 GiB/s`
  - resynth A3: `~7.83–7.87 GiB/s`
  - sse42 pcmpestrm: `~6.26–6.29 GiB/s`
- ASM notes:
  - `sse42_pcmpestrm` dropped check-side shuffle footprint (`pshufb=39`) and `movdqa` count (`64`),
    but throughput regressed materially.
- Perf notes:
  - `sse42_pcmpestrm`: cycles ~`4.82M`, instructions ~`8.29M`, IPC ~`1.72`
  - strict/resynth A3: cycles ~`4.63–4.67M`, instructions ~`8.62M`, IPC ~`1.85`
  - Net: fewer instructions did not translate to fewer cycles; instruction mix/latency of `PCMPESTRM` path dominates.
- Outcome: `regression`.
- Most likely cause:
  - SSE4.2 text-compare instruction family introduces high-latency / lower-throughput behavior for this hot loop,
    and does not accelerate mapping+pack bottlenecks.
- Decision: `park` (`sse42_pcmpestrm` not candidate for default).

### SSE-2026-02-27-017

- Change: added `Resynth A3 Single` strict variant (single-DS64 loop) and evaluated current best-combo set with harness.
- Scope:
  - `src/simd/ssse3_cstyle_experiments_hybrid.rs`
  - `benches/base64_bench.rs`
  - `tests/sse_decode_tests.rs`
  - `scripts/sse_strict_harness.sh` (new key: `resynth_a3_single`)
- Design:
  - Refactored A3 implementation into shared internal path with const unroll selector:
    - `decode_base64_ssse3_cstyle_strict_sse41_resynth_a3_impl<const DOUBLE_UNROLL: bool>`
  - Existing A3 keeps `DOUBLE_UNROLL=true`.
  - New `A3 Single` uses `DOUBLE_UNROLL=false` to test RA/loop-shape sensitivity (less live pressure, more loop overhead).
- Validation:
  - `cargo check` ✅
  - `cargo test --test sse_decode_tests` ✅ (3/3)
  - Harness run:
    - `scripts/sse_strict_harness.sh --variant strict --variant resynth_a3 --variant resynth_a3_single --variant resynth_add4 --skip-perf --out-dir /tmp/sse_harness_bestcombo`
- Bench metrics (report `/tmp/sse_harness_bestcombo/sse_strict_harness_20260227_011140.md`):
  - strict: `~7.8685 GiB/s`
  - resynth A3: `~7.7777 GiB/s`
  - resynth A3 Single: `~7.5499 GiB/s`
  - resynth Add4: `~6.8845 GiB/s`
- ASM notes:
  - `A3 Single` lowers static op counts (e.g., `pshufb=25`, `movdqu=10`) because loop body is smaller, but loses throughput.
  - No XMM stack spills in tested variants.
- Outcome: `regression` for A3 Single vs strict/A3.
- Most likely cause:
  - Single-DS64 reduced ILP and increased loop/control overhead; reduced static op counts did not translate to better cycles/byte.
- Decision:
  - `keep` strict as current baseline winner.
  - `park` A3 Single and Add4 for default path.
  - keep A3 as secondary reference variant only.

### SSE-2026-02-27-018

- Change: full strict-variant scan with harness to select best current combination.
- Scope:
  - `scripts/sse_strict_harness.sh`
  - `src/simd/ssse3_cstyle_experiments_hybrid.rs` variants already integrated
- Validation command:
  - `scripts/sse_strict_harness.sh --variant strict --variant resynth_a3 --variant resynth_a3_single --variant resynth_add4 --variant sse42_pcmpestrm --variant ptest_mask --variant ptest_nomask --variant arith --variant hybrid --skip-perf --out-dir /tmp/sse_harness_fullscan`
- Report:
  - `/tmp/sse_harness_fullscan/sse_strict_harness_20260227_011604.md`
- Bench ranking (thrpt mid, GiB/s):
  - `strict`: `7.9229`
  - `resynth_a3`: `7.8027`
  - `ptest_mask`: `7.7757`
  - `ptest_nomask`: `7.6951`
  - `resynth_a3_single`: `7.4755`
  - `resynth_add4`: `6.9572`
  - `sse42_pcmpestrm`: `6.3143`
  - `arith`: `5.1275`
  - `hybrid`: `3.4415`
- ASM notes (summary):
  - `strict` remains best despite unchanged op mix vs A3.
  - `resynth_add4` still shows higher `movdqa`/LCPI pressure.
  - `sse42_pcmpestrm` lowers pshufb count but still slower (instruction mix latency cost).
  - `hybrid` flagged `POTENTIAL_RELOAD`.
- Outcome: `success` (selection), with most experiment variants regressing.
- Most likely cause:
  - Current strict baseline already matches best codegen shape for this ISA; many alternatives reduce one subcomponent but worsen total cycles via extra uops, worse scheduling, or reload pressure.
- Decision:
  - `keep` strict as default baseline.
  - `park` `resynth_add4`, `sse42_pcmpestrm`, `arith`, `hybrid`, and `a3_single` for production path.
  - keep `resynth_a3`/`ptest_mask` only as reference experiments.

### SSE-2026-02-27-019

- Change: added matrix-style sweep harness for strict variants with multi-round ranking and top-k selection.
- Scope:
  - `scripts/sse_strict_harness.sh` (new variant key: `strict_range`)
  - `scripts/sse_strict_sweep.sh` (new)
- Design:
  - New sweep driver runs variants in series and rounds in series.
  - Uses existing harness per run to keep asm/bench parsing consistent.
  - Aggregates per-variant stats across rounds: median/mean/stddev/min/max GiB/s.
  - Produces sorted ranking and top-k block.
  - ASM snapshot captured from round 1 (unless `--skip-asm`).
- Smoke validation:
  - `scripts/sse_strict_sweep.sh --variant strict --variant resynth_a3 --rounds 1 --top-k 2 --skip-asm --out-dir /tmp/sse_sweep_smoke` ✅
  - `scripts/sse_strict_sweep.sh --variant strict --rounds 1 --top-k 1 --out-dir /tmp/sse_sweep_smoke2` ✅
- Full scan (r1) command:
  - `scripts/sse_strict_sweep.sh --rounds 1 --top-k 5 --out-dir /tmp/sse_sweep_full_r1`
- Full scan (r1) results:
  - Report: `/tmp/sse_sweep_full_r1/sse_sweep_20260227_012557.md`
  - Top 5:
    1. `strict` = `7.8931 GiB/s`
    2. `ptest_mask` = `7.8195 GiB/s`
    3. `resynth_a3` = `7.7598 GiB/s`
    4. `ptest_nomask` = `7.7105 GiB/s`
    5. `resynth_a3_single` = `7.5590 GiB/s`
  - Tail:
    - `strict_range` = `5.1288 GiB/s`
    - `arith` = `5.1078 GiB/s`
    - `hybrid` = `3.4570 GiB/s` (`POTENTIAL_RELOAD`)
- Outcome: `success` (automation + stable ranking workflow).
- Most likely cause of ranking pattern:
  - Baseline strict still offers best total cycles/byte on this CPU; alternatives often optimize one dimension but regress due to instruction mix, extra moves, or reload pressure.
- Decision:
  - `keep` sweep harness as default evaluator for future variant search.
  - keep `strict` as current baseline winner until a variant beats it across multi-round median.

### SSE-2026-02-27-020

- Change: ran sweep ranking with 2 rounds on top candidate set to reduce single-run noise.
- Scope:
  - `scripts/sse_strict_sweep.sh`
- Command:
  - `scripts/sse_strict_sweep.sh --variant strict --variant ptest_mask --variant resynth_a3 --variant ptest_nomask --rounds 2 --top-k 4 --out-dir /tmp/sse_sweep_top4_r2`
- Report:
  - `/tmp/sse_sweep_top4_r2/sse_sweep_20260227_012925.md`
- Results (median GiB/s):
  1. `strict` = `7.8939`
  2. `resynth_a3` = `7.8633`
  3. `ptest_mask` = `7.7430`
  4. `ptest_nomask` = `7.6737`
- Outcome: `success` (stable ranking confirms strict baseline remains top).
- Most likely cause:
  - `strict` and `resynth_a3` keep the most favorable total schedule/pressure; `ptest_mask` removes `pmovmskb` but pays extra overhead (`movdqa`/LCPI) that offsets reduction gains.
- Decision:
  - keep `strict` as default candidate.
  - keep `resynth_a3` as closest challenger for future targeted loop-shape work.

### SSE-2026-02-27-021

- Change: added ambiguity-rate probe tool for hybrid/two-tier feasibility on current hash+conflict model.
- Scope:
  - `src/bin/sse_ambiguity_rate.rs`
- Commands run:
  - `cargo check` ✅
  - `cargo run --bin sse_ambiguity_rate` ✅
- Method:
  - Uses current `delta_hash` model and current hybrid `conflict_buckets` table.
  - Reports ambiguity at byte level and block level (16B, 64B) over vectorized region (`len-4`, rounded to 16B).
  - Bench-like input generator matches criterion setup (`input[i] = i % 256`, then base64 encode).
- Results:
  - `bench_like(size=1024)`:
    - ambiguous bytes: `701/1360` (`51.5441%`)
    - ambiguous 16B blocks: `85/85` (`100%`)
    - ambiguous 64B blocks: `21/21` (`100%`)
  - `bench_like(size=1048576)`:
    - ambiguous bytes: `720893/1398096` (`51.5625%`)
    - ambiguous 16B blocks: `87381/87381` (`100%`)
    - ambiguous 64B blocks: `21845/21845` (`100%`)
- Outcome: `success` (diagnostic), `negative` for current two-tier ROI.
- Most likely cause:
  - Conflict-table model marks ~half of bytes as ambiguous and, at this byte density, every 16B/64B block contains at least one ambiguous byte.
  - Therefore block-level fallback would trigger effectively always, eliminating fast-path benefit.
- Decision:
  - `park` two-tier strategy with current conflict table/hash as low-ROI.
  - If revisited, must first reduce ambiguity density via new partition/table synthesis.

### SSE-2026-02-27-022

- Change: upgraded `sse_hash_resynth` objective from "feasible check" to "shared-hash implementable" with hard gates.
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- New score/gates:
  - `map_conflict_buckets` (mapping delta consistency per bucket)
  - `valid_msb_violations` (valid bytes must not set bit7 in raw hash)
  - `ascii_msb_violations` (ASCII bytes 0..127 must not set bit7, correctness guard for maskless check-indexing)
  - existing `impossible_buckets`, `mixed_buckets`, `feasible_width_sum`
- New ranking priority:
  1. `map_conflict_buckets`
  2. `valid_msb_violations`
  3. `ascii_msb_violations`
  4. `impossible_buckets`
  5. `mixed_buckets`
  6. maximize `feasible_width_sum`
- Additional output:
  - per-spec baseline/best now prints all new fields.
  - top candidates include new fields.
  - final PASS condition:
    - `map_conflicts==0 && valid_msb_vio==0 && ascii_msb_vio==0 && impossible==0`
  - for passing candidates, tool derives and prints `DELTA_VALUES` and `CHECK_VALUES`.
- Validation:
  - `cargo check` ✅
  - `cargo run --bin sse_hash_resynth` ✅
- Result snapshot:
  - best candidates reached `map_conflicts=0` and `valid/ascii_msb_vio=0` for some specs,
    but still had `impossible_buckets=4`.
  - final status:
    - `RESULT: no PASSING shared-hash candidate found (map_conflicts==0, valid_msb_vio==0, ascii_msb_vio==0, impossible==0).`
- Outcome: `success` (better search objective), `negative` (no fully passing candidate in current search family).
- Most likely cause:
  - Current constrained hash family (mix in {avg,add,xor}, shift in {2..5}, 16-entry asso table) is still too limited to satisfy all strict gates simultaneously.
- Decision:
  - keep this scoring as new baseline for future synthesis.
  - next search should expand hash family carefully (without adding hot-loop ops) before new kernel spikes.

### SSE-2026-02-27-023

- Change: optimized `sse_hash_resynth` runtime (same search semantics).
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- Optimizations applied:
  - bucket representation changed from `[[bool;256];16]` to bitsets `[[u64;4];16]` for both map/check buckets.
  - precomputed `required_delta_lut[256]` once in `main` and passed into evaluation/derivation paths.
  - rewrote local-search inner loop to avoid per-iteration `Candidate` cloning/copying; now compares/stores `Score` + tables directly.
  - minor loop cleanup in bucket builder (`contexts` hoisted, cheaper ASCII check).
- Validation:
  - `cargo check --bin sse_hash_resynth` ✅
  - timing probe (warm, release):
    - `/usr/bin/time cargo run --release --bin sse_hash_resynth -- 8 1000 $(nproc)` -> `0:21.31 real`, `731% cpu`.
- Outcome: `success` (runtime reduced with no algorithmic behavior change).
- Most likely cause:
  - lower memory traffic and less branchy membership representation in scoring path + fewer object copies in hot local-search loop.
- Decision:
  - use this optimized synthesizer for large waves (`96 x 6000`) to reduce wall time.

### SSE-2026-02-27-024

- Change: executed full large synthesis wave after runtime optimizations.
- Scope:
  - `src/bin/sse_hash_resynth.rs`
- Command:
  - `cargo run --release --bin sse_hash_resynth -- 96 6000 $(nproc) > /tmp/sse_hash_resynth_96x6000_opt.log 2>&1`
- Result summary:
  - `RESULT: found 1 PASSING MASKLESS shared-hash candidate(s).`
  - `RESULT: no PASSING MASKED shared-hash candidate found (masked_0f tier).`
  - `RESULT: found 1 PASSING EXTENDED shared-hash candidate(s).`
  - `RESULT: no PASSING MASKLESS poisoned-mapping candidate found.`
  - `RESULT: no PASSING MASKED poisoned-mapping candidate found.`
  - `RESULT: no PASSING EXTENDED poisoned-mapping candidate found.`
- Best PASSING shared-hash candidate:
  - `family=predsel_bit6`
  - `mode=maskless`
  - `mix=avg_epu8`
  - `shift=2`
  - `extra_ops=1`
  - `const_live_delta=0`
  - `primary=[01 01 01 01 01 01 01 01 00 00 01 18 00 00 00 20]`
  - `perturb=[00 00 00 00 1f 00 00 00 00 00 00 01 00 00 00 00]`
  - `DELTA_VALUES(i8)=[0, 19, 0, 0, 0, 0, 16, 4, -65, -65, -65, -65, -71, -71, -71, -71]`
  - `CHECK_VALUES(i8)=[-128, -43, -128, -128, -128, -128, -47, -48, -65, -69, -75, -85, -97, -101, -107, -117]`
- Poison objective best (non-passing):
  - best reached `poison_impossible=1` (`predsel_bit6`, `masked_0f`, `sub_epi8`, `shift=3`) but failed MSB gates; no poisoned-pass candidate.
- Outcome: `success` (search completed, stable winner found for shared-hash), `negative` for poisoned mapping family in this wave.
- Most likely cause:
  - expanded families now solve shared-hash feasibility under strict gates, but poisoned mapping remains structurally constrained (still leaves unresolved poison buckets under current budgets/gates).
- Decision:
  - next high-ROI step is a minimal kernel spike with this PASSING shared-hash candidate while preserving strict loop shape.
  - park poisoned mapping until a new family/model reduces `poison_impossible` to zero under gates.

### SSE-2026-02-27-025

- Change: implemented and validated minimal kernel spike for PASSING shared-hash candidate.
- Scope:
  - `src/simd/ssse3_cstyle_experiments_hybrid.rs`
  - `benches/base64_bench.rs`
  - `tests/sse_decode_tests.rs`
- Variant added:
  - decoder: `Ssse3CStyleStrictSse41ResynthSharedBit6Decoder`
  - benchmark label: `Rust Port (SSSE3+SSE4.1, C-Style Strict Resynth Shared Bit6)`
  - hash/check model:
    - family `predsel_bit6`, mode `maskless`, mix `avg`, shift `2`
    - shared-hash check (`check_values` indexed by the same hash used for map)
- Validation:
  - `cargo check --tests --benches` ✅
  - `cargo test --test sse_decode_tests test_sse_decode_specific_lengths -- --exact` ✅
  - `cargo test --test sse_decode_tests test_sse_decode_invalid_chars -- --exact` ✅
- Performance (taskset pinned):
  - baseline strict (`SSSE3, C-Style Strict`, 1MiB): `~7.59 GiB/s` (range `[7.5539, 7.6247]`)
  - shared-bit6 spike (first run): `~6.41 GiB/s` (range `[6.3714, 6.4451]`)
  - shared-bit6 after micro-opt (`blendv` selector): `~6.34 GiB/s` (range `[6.2812, 6.3855]`, regressed vs first run)
- Outcome: `negative` (correctness OK, throughput significantly below baseline).
- Most likely cause:
  - this shared-hash family does not reduce shuffle count in practice; it replaces removed check-hash ops with selector overhead in map path.
  - hot-loop shape now includes extra blend/select work (e.g., `pblendvb`) and additional dependency pressure, so net throughput drops.
- Decision:
  - do not pursue this kernel path further as-is (low ROI).
  - keep implementation as reference experiment; prefer reverting from benchmark rotation if we want cleaner default runs.
