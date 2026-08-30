# TUI Linux native execution evidence

> Recorded: 2026-08-30. This closes native Linux arm64 linking, execution,
> production Runtime/SQLite, and xterm-compatible PTY automation for the named
> candidate. It does not close Linux x86_64 native execution, tmux, a physical
> terminal emulator, or `TERM=dumb` behavior.

## Candidate and environment

| Field | Value |
|---|---|
| Garive revision | `b3543685` |
| Rust | `rustc 1.98.0 (88d9e12ae 2026-08-18)` |
| Rust host | `aarch64-unknown-linux-gnu` |
| Linux kernel | `7.1.3-200.fc44.aarch64` |
| Userland | Debian 12 `rust:1.98-bookworm` container |
| VM/container | Podman 6.1.0, AppleHV arm64, `linux/arm64` |
| PTY driver | Expect 5.45.4, `TERM=xterm-256color` |

The container and Rust toolchain used the VM's native arm64 architecture; no
CPU emulation was involved. The repository was mounted read-only and Cargo
outputs were isolated in a container volume.

## Passing gates

The complete TUI package linked and ran natively:

```text
cargo test -p garive-tui
```

Result: exit `0`; 19 test binaries; 75 passed, 0 failed. This includes five
live shipping-binary PTY cases (`20.30s`), private persistence, reducer/editor,
snapshots, performance, crash recovery, and the production HTTP Host plus
SQLite round trip (`83.21s`). The production test performs completion,
suspension/continuation, cancellation, background Session completion, session
switching, CJK persistence, clean exit, restart, and screen-reader replay.

The candidate also passed strict native linting:

```text
cargo clippy -p garive-tui --all-targets -- \
  -D warnings -D clippy::undocumented_unsafe_blocks
```

Result: exit `0`; `Finished dev profile` in `3.47s`.

The focused Runtime SQLite adapter suite passed 6/6 on the same native Linux
environment. The full TUI run then exercised that adapter through the shipping
binary and production Host.

## Defect found and closed

The first native run exposed a real concurrent-read defect. Ledger replay read
`ledger_facts` and `ledger_sessions` in separate SQLite snapshots, so a commit
between the queries could make a valid database appear corrupt as
`InvalidStoredValue("session projection")`. Revision `f3a0ada9` now wraps the
two reads in one deferred read transaction and reuses an existing write
transaction during commit. The Linux production Runtime/PTTY test subsequently
passed twice, followed by the candidate-bound full-suite pass above.

The run also made the Expect harness's platform-dependent UTF-8 channel setup
explicit. The fixture now selects UTF-8 before spawning and configures the
spawn channel directly; the durable ledger assertion proves the exact
`耐久 tui` payload survives the Linux PTY round trip.

## Remaining platform gates

- execute the same suite on native Linux x86_64 hardware or VM;
- run the shipping executable in named physical terminal emulators and tmux;
- verify `TERM=dumb` refusal/linear fallback and signal restoration there;
- capture candidate-bound native Linux screenshots if Linux visual admission
  is requested.

Linux x86_64 still has complete source-level all-target check and strict-Clippy
evidence in
[`tui-linux-cross-build-2026-08-30.md`](tui-linux-cross-build-2026-08-30.md),
but that result must not be described as native execution.
