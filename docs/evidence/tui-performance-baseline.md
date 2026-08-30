# TUI interactive latency baseline

> Recorded: 2026-08-30. This is reproducible local evidence, not a claim about
> every terminal, operating system, or CPU.

## Environment

| Field | Value |
|---|---|
| Machine | Apple M1 Pro, arm64 |
| OS | Darwin 25.6.0 |
| Rust | `rustc 1.98.0 (88d9e12ae 2026-08-18)` |
| Profile | Cargo debug test profile |
| Command | `cargo test -p garive-tui --test performance -- --nocapture` |

## Workload and result

The executable test renders a `120×40` Ratatui buffer containing a bounded
200-cell mixed User/Agent/activity timeline. It discards ten warm-up frames,
then sorts 100 measured frames. The editor sample inserts 100 CJK graphemes
through the production undo/bounds path.

| Measurement | Result | Blocking gate |
|---|---:|---:|
| Render p50 | 3,329 µs | — |
| Render p95 | 3,545 µs | < 50,000 µs |
| Editor insert p95 | 58 µs | < 4,000 µs |

The render gate is deliberately wider than a 60 Hz frame on debug builds so
shared CI noise does not create a false product failure. The recorded p95 is
below 4 ms on the reference machine. The test performs no network or file I/O
inside rendering; Runtime/PTY latency is covered separately by
`production_runtime.rs` and `live_h1.rs`.

Re-record this document only after reading the emitted `TUI_BASELINE` line and
reviewing any workload, dependency, or hardware change. Release-profile and
other-platform numbers remain external compatibility evidence.

## Release outer-process first frame

The release gate launches 60 independent shipping `garive-tui` processes under
a real `expect` PTY: three runs of 20 samples. Every process connects to the
production `LiveHost` backed by a fresh file SQLite database. Time begins before
process spawn and ends after terminal negotiation and the first interactive
`GARIVE` frame. Each process then takes the normal confirmed quit path.

```sh
cargo build --release -p garive-tui --bin garive-tui \
  --example visual_demo_host --example release_process_baseline
cargo run --release -p garive-tui --example release_process_baseline
```

Pinned evidence: [`tui-release-first-frame-2026-08-30.json`](tui-release-first-frame-2026-08-30.json),
Garive `54ae160b697147a00e7e1fc128cb3accdc19a18c`.

| Run | p50 | p95 | p99 | max | Gate |
|---:|---:|---:|---:|---:|---:|
| 1 | 26.338 ms | 28.883 ms | 28.883 ms | 356.375 ms | p95 < 150 ms |
| 2 | 25.778 ms | 26.503 ms | 26.503 ms | 27.081 ms | p95 < 150 ms |
| 3 | 25.869 ms | 27.068 ms | 27.068 ms | 27.198 ms | p95 < 150 ms |

All three p95 values pass. The first run's unsmoothed maximum is retained; the
gate is percentile-based and no outlier was deleted. This closes the first-frame
metric on the pinned macOS reference only. Idle CPU, the exact
10-Session/5,000-cell peak-RSS workload, and other native platforms remain open.
