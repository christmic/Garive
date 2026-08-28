# cli/

> **One-shot CLI commands. Single invocation, pipe-friendly,
> script-friendly.** For the cases where the user (or a shell
> script / CI job / Makefile target) wants to ask the agent
> one specific question, see the answer, and exit.

`cli/` is a Rust workspace member (added to the root
`Cargo.toml` `members` when the slice lands). It is the **bash
of Garive** — a non-interactive entry point for everything the
agent can do.

## When to Use `cli/`

- One question, one answer: `garive ask "explain this file"`.
- Pipe-friendly: `cat patch.diff | garive review`.
- Script-friendly: `garive run --task yaml < job.yaml`.
- CI-friendly: deterministic exit codes, structured output
  (`--json`), no TTY assumptions.
- Quick prototyping of new agent capabilities before they
  need a UI surface.

## When NOT to Use `cli/`

The user wants a **resident interactive UI** — multiple turns,
streaming token output, keybindings, inline previews. Use
`tui/` instead.

## What Lives Here

| Subcommand (planned) | Purpose |
|----------------------|---------|
| `garive run` | Run a single task end to one final answer. |
| `garive ask` | One question, no task lifecycle. |
| `garive review` | Inline review of a diff / file. |
| `garive memory {store, recall, ...}` | Memory ops without UI. |
| `garive bench` | (or just delegates to `cargo run -p bench`). |
| `garive config` | Read / write config. |

## Conventions

- **One invocation = one process.** No long-running daemons.
- **stdin / stdout / stderr are streams.** The CLI respects
  pipes; `--json` outputs machine-parseable JSON; default
  output is human-friendly plain text.
- **Exit codes follow Unix conventions.** 0 = success,
  1 = runtime failure, 2 = usage error, 64+ = agent-specific
  (defined in ` `AGENTS.md`).
- **No interactive prompts.** If input is missing, fail with
  exit code 2 and a clear error message.
- **Flags are stable.** `garive run --task <file> --json` is
  the contract; breaking it requires a major version bump.

## Dependencies

- Talks to `engine/` crates via normal Cargo path deps.
- Talks to `runtime/gateway/` over the wire schema in
  `spec/proto/` when the agent isn't running locally.

## Build

```
cargo run -p cli -- run --task job.yaml
```

`just cli` is a thin wrapper.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
