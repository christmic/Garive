# TUI interactive latency baseline

> Recorded: 2026-08-30; latest candidate rerun: 2026-09-01. This is reproducible local evidence, not a claim about
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
process spawn and ends after terminal setup and the first interactive `Garive`
frame. Explicit themes do not perform the System-only OSC color probe. Each
process then takes the normal confirmed quit path.

```sh
cargo build --release -p garive-tui --bin garive-tui \
  --example visual_demo_host --example release_process_baseline
cargo run --release -p garive-tui --example release_process_baseline
```

Latest pinned evidence: [`tui-release-process-2026-09-01.json`](tui-release-process-2026-09-01.json),
Garive `06398a03f9a8c186d47ae0d15cc6a604c2cd5328`. Earlier reports remain
retained for audit history.

| Run | p50 | p95 | p99 | max | Gate |
|---:|---:|---:|---:|---:|---:|
| 1 | 28.993 ms | 31.902 ms | 400.507 ms | 400.507 ms | p95 < 150 ms |
| 2 | 31.050 ms | 34.039 ms | 40.929 ms | 40.929 ms | p95 < 150 ms |
| 3 | 33.991 ms | 45.759 ms | 96.995 ms | 96.995 ms | p95 < 150 ms |

All three p95 values pass. Schema v3 uses nearest-rank percentiles and retains
all 60 sorted samples, exposing the first run's isolated 400.507 ms tail rather
than smoothing it away. This closes the first-frame metric on the pinned macOS
reference only.

## Release idle CPU and bounded-model peak RSS

After the production Host reaches the online empty state, the same outer-process
gate samples each shipping TUI for 60 seconds. The three runs consumed 60 ms
CPU time each, or 0.1% of one logical core, satisfying the `<0.5%` gate.
Empty-state RSS peaked between 10,768 and 10,832 KiB; those samples do not stand
in for the separate loaded-model workload.

The RSS gate launches three isolated release children under `/usr/bin/time -l`.
Each child constructs the production `AppModel` with exactly 10 Session
summaries and 5,000 bounded mixed Unicode/Markdown timeline cells, renders the
production view at `200×60`, asserts the counts, and remains isolated from the
other benchmark corpora.

```sh
cargo run --release -p garive-tui --example release_memory_baseline
```

Pinned evidence: [`tui-release-memory-2026-08-31.json`](tui-release-memory-2026-08-31.json),
Garive `d907ea633f024474de21bd3be50b63f5b53f7875`.

| Run | Peak RSS | Gate |
|---:|---:|---:|
| 1 | 3.984 MiB | < 100 MiB |
| 2 | 4.031 MiB | < 100 MiB |
| 3 | 4.031 MiB | < 100 MiB |

The same candidate's three-run in-process distribution is retained in
[`tui-release-in-process-2026-08-31.json`](tui-release-in-process-2026-08-31.json).
Its worst observed p95 was 307 µs key-to-model, 405 µs for the 120×40 render,
600 µs for the 200×60 resize, and at least 776,025 H3 reductions/second at p05.

Both metrics are Gates on the pinned macOS reference environment. Other native
platforms remain open.

## Release reconnect-churn stability

The release gate runs the shipping TUI under an `expect` PTY against production
`LiveHost` and file SQLite for at least 30 minutes. It alternates unique,
numbered committed Turns with `/reconnect`, so a stale redraw cannot satisfy the
progress checks. The executable gate requires at least 1,000 reconnects and 100
committed Turns, peak TUI RSS below 100 MiB, and the late five-minute RSS peak
no more than 20 MiB above the early five-minute peak.

```sh
cargo build --release -p garive-tui --bin garive-tui \
  --example visual_demo_host --example release_churn_baseline
cargo run --release -p garive-tui --example release_churn_baseline
```

Pinned evidence: [`tui-release-churn-2026-08-31.json`](tui-release-churn-2026-08-31.json),
Garive `8b077f128d62bd90b22c63283fec500c6c70714b`.

| Duration | Reconnects | Committed Turns | Early peak | Late/overall/end peak | Result |
|---:|---:|---:|---:|---:|---|
| 1,800.080 s | 1,426 | 143 | 11,680 KiB | 12,784 KiB | PASS |

The late-window increase was 1,104 KiB, within the 20,480 KiB growth gate; the
12,784 KiB overall peak was within the 102,400 KiB absolute gate. This closes
the sustained reconnect/Turn memory-stability gate on the pinned macOS arm64
candidate only.
