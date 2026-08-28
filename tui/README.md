# tui/

> **Resident terminal UI. Long-lived, interactive, rich
> output.** For the cases where the user sits in front of a
> terminal and drives the agent through a session.

`tui/` is a Rust workspace member (added to the root
`Cargo.toml` `members` when the slice lands). It is the
**REPL of Garive** — a continuous, streaming, keyboard-driven
view into the agent runtime.

## When to Use `tui/`

- Multi-turn conversation with the agent.
- Live streaming output (token-by-token chat, in-progress
  tool calls, status lines).
- Rich terminal rendering: syntax highlighting, markdown,
  diff previews, progress bars, multi-pane layouts.
- Keyboard-driven shortcuts: copy, paste, edit, regenerate,
  cancel, navigate history.
- Resumes across reconnects — open the same session in
  multiple terminals and pick up where you left off.

## When NOT to Use `tui/`

The user (or CI / Makefile / shell script) wants a
**non-interactive one-shot**. Use `cli/` instead.

## What Lives Here

A Rust binary that embeds a TUI framework (`ratatui`,
`crossterm`, or similar — decided when the slice lands) and
renders the agent runtime state.

| Pane (planned) | Purpose |
|--------------|---------|
| Chat | Streaming conversation |
| Tool calls | Inline or side-pane list with parameters + output |
| Memory / Knowledge | Read-only browse + search |
| Logs | Filtered structured logs from the agent runtime |
| Status | Active tool, current task, token usage, elapsed time |

## Conventions

- **One TUI process per session.** Multiple sessions →
  multiple processes, not one process holding many.
- **Resident but interruptible.** Cancel / abort shortcuts
  must work mid-stream.
- **Rich terminal, fall back to plain.** Detect the TTY's
  capability (`$COLORTERM`, `stty size`); degrade gracefully
  to plain text when the env can't render.
- **No interactive prompts that block the runtime.** Tool
  approval is a Y/N keystroke, not a `read -p` loop.
- **Streaming is mandatory.** Output is rendered as it
  arrives, not buffered to end-of-task.

## Dependencies

- Talks to `engine/` crates via normal Cargo path deps.
- Talks to `runtime/gateway/` over the wire schema in
  `spec/proto/` when running against a remote agent.

## Build

```
cargo run -p tui
```

`just tui` is a thin wrapper.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
