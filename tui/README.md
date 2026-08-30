# Garive TUI

Resident, restart-safe terminal client for the loopback Garive Host. The
shipping binary provides a responsive Ratatui workspace, durable Session
navigation, multiline Unicode editing, safe Markdown, H1 event follow with
bounded reconnect, H2 timeline restore, H3 activity, typed suspension replies,
exact idempotent retry, and private local presentation state.

## Run

Start a configured Runtime Host, then launch:

```text
cargo run -p garive-tui -- --host http://127.0.0.1:4317/
```

Useful launch options:

```text
--session <id>             open a durable Session
--definition <id>          default Agent definition for new Sessions
--theme system|dark|light|mono
--mouse auto|on|off
--screen-reader            linear output without alternate-screen addressing
--reduced-motion
--state-dir <absolute-path>
--no-prompt-history
--ephemeral                disable every local state write
```

`--host` must be an explicit credential-free loopback HTTP URL. Run
`garive-tui --help` for the complete contract.

## Interaction

- `Enter` sends; `Ctrl+J` inserts a newline.
- `Ctrl+N` creates a Session; entering the first prompt also creates one when a
  single Agent definition is installed.
- `Ctrl+S`, `Ctrl+P`, and `Ctrl+R` open Sessions, commands, and prompt history.
- `Esc` requests cancellation while a Turn runs; `Ctrl+Q` confirms exit.
- `PageUp`/`PageDown` scroll; mouse capture is opt-in.
- `/help` lists commands. `/retry` repeats the exact persisted command identity;
  `/copy last` and `/copy session-id` use a bounded OSC 52 request.

The Host/SQLite ledger is the only durable conversation authority. Local files
contain preferences, bounded drafts, prompt history, and exact pending-command
recovery envelopes only. Unix state directories/files are enforced as
`0700`/`0600`; `--ephemeral` requires confirmation before mutations.

## Verify

```text
just tui-unit
just tui-snapshots
just tui-persistence
just tui-runtime-e2e
just tui-pty
just tui
```

The Runtime E2E launches the shipping binary in a real PTY against the
production HTTP Host and SQLite ledger, then covers completion, suspension,
continuation, cancellation, exit, restart, and screen-reader replay.

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: resident product implementation; see `spec/STATUS.md` for evidence
