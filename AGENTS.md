# AGENTS.md

> **Garive is the next-generation Agent project** — a multi-language
> platform built around an Agent that grows beyond a coding
> assistant. It contains a Core Agent, multi-channel capability, an
> Agent Gateway Platform, and multi-platform Agent Apps.

This file is the canonical Agent entry point for the Garive repo
(read by OpenAI Codex, Cursor, Claude Code via `CLAUDE.md`, and
similar tools). Project-level Agent resources (engineering rules,
git workflow, conventions) live under `.agents/` and are
`@`-referenced from this file.

## What Garive Is

Not just a Coding Agent. Garive is an Agent that **grows**:

- **Core capabilities** — agent loop, tools, safety, memory,
  knowledge.
- **Research frontiers** — self-drive, value discovery, feedback
  loops.

### Components

- **Core Agent** — the agent runtime. Written in **Rust** as the
  primary language, with a **Kotlin** mirror (`experiments/engine-kt/`)
  that tracks the Rust tree semantically. Shared wire protocol and
  semantics; Kotlin ships in lock-step with Rust.
- **Multi-channel capability** — chat surfaces beyond the coding IDE.
- **Agent Gateway Platform** — high-throughput, stable gateway
  written in **Go**.
- **Multi-platform Agent Apps** — desktop apps in **Swift**
  (macOS); web apps in **TypeScript**.

### Tech Stack

| Part | Language | Build tool | Workspace |
|------|----------|------------|-----------|
| `engine/`, `runtime/replica`, `cli`, `tui`, `bench` | Rust | Cargo workspace (root `Cargo.toml` members) | main |
| `desktop/` backend | Rust (Tauri) | cargo (workspace member) | main |
| `desktop/` frontend | TypeScript / React | pnpm (Tauri CLI orchestrates) | independent |
| `mobile/` | Kotlin (KMP) | Gradle | independent |
| `experiments/engine-kt/` | Kotlin | Gradle | independent |
| `runtime/gateway/` | Go | go build / go mod | independent |
| `spec/proto/` | — | buf / protoc codegen → Rust + Kotlin | single source |

## Repository Rules (apply repo-wide)

@.agents/engineering-rules.md
@.agents/git-workflow.md
@.agents/conventions.md
@.agents/multi-language.md
@.agents/ddd.md
@.agents/architecture.md