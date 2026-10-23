# Wave 20 checked CPU zip-map reliability and timing

Host: AMD Ryzen 7 9800X3D 8-Core Processor, Linux x86_64, 16 available
threads, rustc 1.98.1. Release `calibrate_zip` measured the full
`execute_zip_map` call for `left[i] * right[i] + 7`, including planning,
allocation, workers, and joins. Each independent process warmed both modes
twice, then took seven calibration and five holdout paired samples with
alternating measurement order. Times below are nanoseconds; each range is
minimum / median / maximum. The CSV files in this directory retain every raw
sample and the machine fingerprint.

| Elements | Workers | Process | Calibration sequential | Calibration parallel | Holdout sequential | Holdout parallel | Auto with this validated profile |
| ---: | ---: | :--- | :--- | :--- | :--- | :--- | :--- |
| 8,192 | 2 | A | 4,190 / 4,320 / 5,481 | 18,870 / 20,180 / 35,641 | 4,230 / 4,240 / 4,950 | 19,011 / 21,860 / 22,640 | Sequential |
| 65,536 | 2 | A | 33,171 / 34,670 / 43,041 | 45,870 / 59,461 / 68,481 | 33,121 / 33,171 / 51,121 | 41,511 / 54,371 / 58,161 | Sequential |
| 1,048,576 | 2 | A | 566,348 / 659,809 / 882,542 | 389,065 / 406,545 / 505,527 | 559,198 / 574,817 / 777,070 | 384,396 / 436,186 / 570,687 | Sequential |
| 1,048,576 | 2 | B | 577,198 / 630,318 / 781,080 | 438,466 / 471,447 / 481,857 | 668,989 / 683,729 / 868,072 | 433,846 / 479,636 / 527,417 | Sequential |
| 1,048,576 | 4 | A | 563,928 / 593,698 / 862,712 | 276,473 / 311,875 / 339,375 | 524,227 / 610,199 / 659,939 | 300,584 / 316,824 / 329,364 | Parallel |
| 1,048,576 | 4 | B | 552,717 / 592,118 / 802,181 | 312,074 / 322,125 / 469,246 | 546,158 / 561,447 / 657,949 | 274,353 / 302,754 / 318,074 | Parallel |

The profile gate spans all observed samples and adds 10% to the upper bound.
Its sequential / parallel intervals are respectively `[559198,970797]` /
`[384396,627756]` ns for the first 1,048,576 element two-worker process and
`[577198,954880]` / `[433846,580159]` ns for its replica. Both overlap, so
the apparent median gain is **not** an Auto speedup. For four workers the
intervals are `[524227,948984]` / `[276473,373313]` ns and
`[546158,882400]` / `[274353,516171]` ns. Both independently pass the
strict parallel-upper-below-sequential-lower rule. These are conservative
empirical envelopes, not confidence intervals or a claim about other sizes,
hosts, operations, contention, or future runs. Without an accepted, current
profile for the exact zip-map operation, size, worker count, and host, `Auto`
still selects sequential CPU.

Correctness was checked separately with `cargo test --offline -p
cgx2-execution-policy`: fixed seed `0x5eedead`, 96 zip and scalar-map cases,
zero/single and uneven boundaries, overflow indices, mismatch lengths,
infeasible workers, requested cancellation, injected worker panic, and injected
output/chunk allocation failures. Errors expose no partial output. Replay the
seeded corpus or the injected failures with:

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo test --offline -p cgx2-execution-policy --test zip_map bounded_seeded_corpus_matches_every_backend -- --nocapture
RUSTUP_TOOLCHAIN=1.98.1 cargo test --offline -p cgx2-execution-policy --lib injected_failures_never_return_partial_output -- --nocapture
RUSTUP_TOOLCHAIN=1.98.1 cargo test --offline -p cgx2-execution-policy --lib cancellation_after_launch_wins_over_worker_failures -- --nocapture
```

Reproduce each CSV with the owned benchmark command, replacing `SIZE`,
`WORKERS`, and paths. The `-replica` CSV files are separate processes.

```sh
RUSTUP_TOOLCHAIN=1.98.1 cargo run --release --offline -p cgx2-execution-policy --bin calibrate_zip -- SIZE WORKERS crates/cgx2-execution-policy/measurements/RAW.csv /tmp/cgx2-zip-profile.txt
```
