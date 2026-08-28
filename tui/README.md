# TUI

Rust terminal Host client. The first executable slice renders the ordered
shared fake-host events and exactly one completion in a terminal frame:

```text
cargo run -p garive-tui
```

This slice proves the Host boundary and terminal state, not a resident
multi-turn interface. Ratatui/crossterm, live incremental transport,
keybindings, approvals and reconnectable Sessions remain separately gated.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable fake-host shell
