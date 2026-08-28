# CLI

One-shot, pipe-friendly Rust Host client. The admitted first slice accepts one
text argument, runs the shared Host v1 fake scenario through
`garive_runtime::FakeHost`, prints ordered deltas and exits only after the
fixture terminal.

```text
cargo run -p garive-cli -- hello
```

The full `ask/run/review`, JSON output and stable exit-code surface is future
work and is not claimed by this shell. It remains non-interactive; resident
session UX belongs in `tui/`.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable fake-host shell
