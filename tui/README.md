# TUI

Rust terminal H1 client. The first live slice receives an explicit loopback
Host and renders every newly applied durable event in position order:

```text
cargo run -p garive-tui -- http://127.0.0.1:4317/ my-agent "hello"
```

This is not yet a resident multi-turn interface. Ratatui/crossterm, keybindings,
approvals and reconnectable Session UX remain separately gated.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: live H1 terminal client
