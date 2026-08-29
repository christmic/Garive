# CLI

One-shot, pipe-friendly Rust H1 client. It accepts an explicit loopback Host,
creates a Session, starts one Turn with stable per-command idempotency keys and
prints only the committed completion text.

```text
cargo run -p garive-cli -- http://127.0.0.1:4317/ my-agent "hello"
```

Exit codes are `0 completed`, `2 client/transport`, `3 suspended`, `4 stopped`
and `5 failed`. The full `ask/run/review` and JSON output surface remains
future work; resident Session UX belongs in `tui/`.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: live H1 one-shot client
