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

Latest pinned evidence: [`tui-release-process-2026-08-30.json`](tui-release-process-2026-08-30.json),
Garive `d0cfc1c01da30d9389907fbb1bb4b61db1eee34b`. The earlier
first-frame-only record remains retained for audit history.

| Run | p50 | p95 | p99 | max | Gate |
|---:|---:|---:|---:|---:|---:|
| 1 | 23.269 ms | 24.971 ms | 24.971 ms | 527.282 ms | p95 < 150 ms |
| 2 | 23.535 ms | 26.098 ms | 26.098 ms | 26.175 ms | p95 < 150 ms |
| 3 | 23.098 ms | 25.113 ms | 25.113 ms | 33.601 ms | p95 < 150 ms |

All three p95 values pass. The first run's unsmoothed maximum is retained; the
gate is percentile-based and no outlier was deleted. This closes the first-frame
metric on the pinned macOS reference only.

## Release idle CPU and bounded-model peak RSS

After the production Host reaches the online empty state, the same outer-process
gate samples each shipping TUI for 60 seconds. Three runs recorded no measurable
CPU-time increase at the Darwin `ps` 10 ms resolution. Therefore each run is
strictly below `10 ms / 60 s = 0.017%` of one logical core, satisfying the
`<0.5%` gate. Empty-state RSS samples are retained in the JSON but do not stand
in for the separate loaded-model workload.

The RSS gate launches three isolated release children under `/usr/bin/time -l`.
Each child constructs the production `AppModel` with exactly 10 Session
summaries and 5,000 bounded mixed Unicode/Markdown timeline cells, renders the
production view at `200×60`, asserts the counts, and remains isolated from the
other benchmark corpora.

```sh
cargo run --release -p garive-tui --example release_memory_baseline
```

Pinned evidence: [`tui-release-memory-2026-08-30.json`](tui-release-memory-2026-08-30.json),
Garive `d0cfc1c01da30d9389907fbb1bb4b61db1eee34b`.

| Run | Peak RSS | Gate |
|---:|---:|---:|
| 1 | 3.938 MiB | < 100 MiB |
| 2 | 4.016 MiB | < 100 MiB |
| 3 | 3.953 MiB | < 100 MiB |

Both metrics are Gates on the pinned macOS reference environment. Other native
platforms and the scheduled 30-minute reconnect-churn gate remain open.
