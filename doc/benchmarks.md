# Benchmark Results (Turbo-Style)

This document summarizes the latest `criterion` run comparing `oxid64` against Turbo-Base64 and Lemire's fastbase64, following the Turbo-Base64 style methodology.

## Methodology

- Single-thread benchmark (`cargo bench --bench base64_bench`).
- Sizes: `10,000` and `1,000,000` bytes.
- Throughput unit: `GiB/s` (criterion output).
- Decode split into:
1. Checked (strict/validating)
2. No-check (non-strict)
- Encode split into one table with all available variants.

Run reference date: 2026-03-02.

## Decode (Checked) Throughput

| Implementation | 10,000 | 1,000,000 |
|---|---:|---:|
| oxid64 scalar strict | 5.0237 | 4.9418 |
| oxid64 ssse3 strict | 12.014 | 12.346 |
| oxid64 avx2 strict | 22.277 | 22.736 |
| oxid64 avx512vbmi strict | 5.0091 | 4.9690 |
| tb64s scalar | 3.4279 | 3.3890 |
| tb64x scalar | 5.3792 | 5.2910 |
| tb64v128 check | 11.439 | 11.476 |
| tb64v256 check | 21.633 | 17.282 |
| fastbase64 avx2 validating | 20.073 | 20.201 |

### Decode (Checked) Win Matrix
Reference: `oxid64 avx2 strict`

| Rival | 10,000 | 1,000,000 |
|---|---|---|
| tb64s scalar | ✅ | ✅ |
| tb64x scalar | ✅ | ✅ |
| tb64v128 check | ✅ | ✅ |
| tb64v256 check | ✅ | ✅ |
| fastbase64 avx2 validating | ✅ | ✅ |

## Decode (No Check) Throughput

| Implementation | 10,000 | 1,000,000 |
|---|---:|---:|
| oxid64 ssse3 non-strict | 16.225 | 16.807 |
| oxid64 avx2 non-strict | 24.281 | 24.873 |
| tb64v128 no-check | 18.771 | 18.975 |
| tb64v256 no-check | 33.353 | 27.474 |

### Decode (No Check) Win Matrix
Reference: `oxid64 avx2 non-strict`

| Rival | 10,000 | 1,000,000 |
|---|---|---|
| tb64v128 no-check | ✅ | ✅ |
| tb64v256 no-check | ❌ | ❌ |

## Encode Throughput

| Implementation | 10,000 | 1,000,000 |
|---|---:|---:|
| oxid64 scalar | 6.8635 | 6.5591 |
| oxid64 ssse3 | 6.8498 | 6.9324 |
| oxid64 avx2 | 25.235 | 23.167 |
| oxid64 avx512vbmi | 6.8346 | 6.5949 |
| tb64s scalar | 3.2108 | 3.1309 |
| tb64x scalar | 6.4698 | 6.4745 |
| tb64v128 | 14.285 | 14.417 |
| tb64v256 | 22.662 | 21.644 |
| fastbase64 avx2 | 27.828 | 23.072 |

### Encode Win Matrix
Reference: `oxid64 avx2`

| Rival | 10,000 | 1,000,000 |
|---|---|---|
| tb64s scalar | ✅ | ✅ |
| tb64x scalar | ✅ | ✅ |
| tb64v128 | ✅ | ✅ |
| tb64v256 | ✅ | ✅ |
| fastbase64 avx2 | ❌ | ✅ |

## Notes

- At `1,000,000` bytes, `oxid64 avx2` is the strongest `oxid64` path in checked decode and encode.
- For no-check decode, Turbo `tb64v256` remains faster in this run.
- For encode at `10,000`, `fastbase64 avx2` is still ahead.
