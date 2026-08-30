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
