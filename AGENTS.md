# Garive — Agent Engineering Rules

> Repository-local implementation policy. Applies to every tracked
> source file in this repository. Project-level Agent resources
> (rules, skills, agent definitions) live under `.agents/` and are
> `@`-referenced from this file.

## Project Overview

**Garive is the next-generation Agent project** — a multi-language
platform built around an Agent that grows beyond a coding assistant.

### Components

- **Core Agent** — the agent runtime: loop, tools, safety, memory,
  knowledge. Written in **Rust** as the primary language, with a
  **Kotlin** mirror for cross-language isomorphic design (shared
  wire protocol and semantics; Kotlin ships in lock-step with Rust).
- **Multi-channel capability** — chat surfaces beyond the coding IDE.
- **Agent Gateway Platform** — high-throughput, stable gateway
  written in **Go**.
- **Multi-platform Agent Apps** — desktop apps in **Swift** (macOS);
  web apps in **TypeScript**.

### Positioning

Not just a Coding Agent. Garive is an Agent that **grows**:

- Core capabilities: loop, tools, safety, memory, knowledge.
- Research frontiers: self-drive, value discovery, feedback loops.

### Tech Stack

| Part | Language | Build tool | Workspace |
|------|----------|------------|-----------|
| `engine/`, `runtime/replica`, `cli`, `tui`, `bench` | Rust | Cargo workspace (root `Cargo.toml` members) | main |
| `desktop/` backend | Rust (Tauri) | cargo (workspace member) | main |
| `desktop/` frontend | TypeScript / React | pnpm (Tauri CLI orchestrates) | independent |
| `mobile/` | Kotlin (KMP) | Gradle | independent |
| `experiments/kotlin/` | Kotlin | Gradle | independent |
| `runtime/gateway/` | Go | go build / go mod | independent |
| `spec/proto/` | — | buf / protoc codegen → Rust + Kotlin | single source |

@.agents/engineering-rules.md
@.agents/git-workflow.md
@.agents/memory-flush.md
@.agents/conventions.md
@.agents/architecture.md