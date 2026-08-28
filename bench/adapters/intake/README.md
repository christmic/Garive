# bench/adapters/intake/

> **Intake adapter.** Translates a `Case` plus a live
> `EnvSetup` into the **agent's native input format**.

## Why

Different agents consume tasks differently:

| Agent | Input format |
|-------|--------------|
| Markdown-friendly chat agents | a Markdown issue body + workspace pointer |
| IDE-style agents | a structured prompt with file references + diff hunk suggestions |
| JSON-RPC agents | a JSON payload with `task_id`, `repo`, `issue_text`, `base_commit` |
| Voice / streaming agents | TTS-ready text + audio cue metadata |

The driver loop doesn't know which. The intake adapter is the
single point where case data gets shaped into whatever the
agent accepts.

## Contract (prose)

| Method | What it does |
|--------|--------------|
| `translate(case, env_setup) → agent_input` | Produce the agent's native input representation. May include the issue text, repo pointer, base commit, hints about the env, etc. |
| `name()` | Stable identifier for this intake. Recorded in tracking so score history knows which intake produced which agent run. |

## Per-agent Implementations

| Adapter | Agent |
|---------|-------|
| `garive/` | Garive's native agent (canonical intake) |
| `markdown/` | Generic Markdown-chat agents |
| `noop/` | no-op intake (testing the runner itself) |

A new agent → a new sub-directory + a manifest entry.

## Where the Adapter Runs

The intake adapter runs **before** the env is even fully set
up in some cases (the adapter can pre-compute prompt context
from the case alone). In other cases it runs after env setup
(e.g. to include resolved file paths). The driver loop
handles both orderings — pass `case` first, then `env_setup`
once the env signals readiness.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
