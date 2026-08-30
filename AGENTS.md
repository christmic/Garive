# AGENTS.md

> **Garive is the next-generation Agent project** — a multi-language
> platform built around an Agent that grows beyond a coding
> assistant. It contains a Core Agent, multi-channel capability,
> an Agent Gateway Platform, and multi-platform Agent Apps.

This file is the canonical Agent entry point for the Garive repo
(read by OpenAI Codex, Cursor, Claude Code via `CLAUDE.md`, and
similar tools). Project-level Agent resources (engineering rules,
git workflow, conventions, multi-language sync, testing pyramid,
doc style) live under `.agents/` and are `@`-referenced from
this file. **`.agents/` is the code constitution** — every file
there is a rule that applies repo-wide; tier-specific overrides
live in `<tier>/AGENTS.md`.

## The R&D Pipeline

Garive's documentation is layered. Each layer has a job; don't
mix them.

```
  stage 1           stage 2          stage 3          stage 4          stage 5
  ┌──────┐         ┌────────┐        ┌─────────┐      ┌──────────┐     ┌──────┐
  │ docs │ ──────→ │ spec   │ ─────→ │ .agents │ ───→ │ <tier>/  │ ─→  │ code │
  └──────┘         └────────┘        │     +   │      │ AGENTS.md │     └──────┘
  natural lang.    normative         │ tier    │      │ tier-
  human-edited     contract          │ AGENTS  │      │ specific
  thinking         what we will      └─────────┘      │ overrides
  designing        implement                          └──────────┘
  comparing
```

| Stage | Lives in | Job | Edited by |
|-------|----------|-----|-----------|
| **1. Design** | `docs/` | Think. Compare options. Record trade-offs. Output may be wrong; iteration is cheap. | humans |
| **2. Spec** | `spec/` | Decide. Name the contract. Invariants, types, acceptance. **Only spec what we are about to implement** (Living Specification rule). | humans |
| **3. Constitution** | `.agents/` | Rules every tier obeys. Engineering, git, conventions, multi-language, testing. | humans + agents |
| **4. Tier rules** | `<tier>/AGENTS.md` | Tier-specific overrides of the constitution. | humans + agents |
| **5. Code** | the codebase | The implementation. Conforms to 3 + 4; checked by the testing pyramid in `.agents/testing.md`. | agents (human-supervised) |

The layer a question belongs to:

- "Should this slice live in Rust or Kotlin?" → `docs/`
  (design).
- "What does this slice's contract say?" → `spec/`.
- "How should I name a Rust function?" → `.agents/conventions.md`.
- "What's the build rule for `engine/llm/`?" → `engine/AGENTS.md`.

If a question doesn't fit any layer, the layer is wrong —
fix the layer, not the answer.

## What Garive Is

Not just a Coding Agent. Garive is an Agent that **grows**:

- **Core capabilities** — agent loop, tools, safety, memory,
  knowledge.
- **Research frontiers** — self-drive, value discovery, feedback
  loops.

### Components

- **Core Agent** — one bounded reasoning execution, production-first in Rust.
  Kotlin experimentally re-implements admitted portable slices (currently
  D0, C0-C5, and C5b-A) from accepted specs and shared semantic fixtures; it is neither a
  product Runtime nor a second source of truth.
- **Multi-channel capability** — chat surfaces beyond the coding IDE.
- **Protocol adapters** — provider-independent implementations of admitted LLM
  wire protocols, with no Garive model or environment dependency.
- **Providers** — deployment and model composition between the neutral model
  contract and a protocol adapter.
- **Runtime** — product Sessions, durable turns, execution, storage, recovery,
  configuration, and the host boundary used by every client.
- **Agent Gateway Platform** — an optional Go service edge, admitted when
  deployment evidence requires separate scaling or isolation.
- **Multi-platform Agent Apps** — Tauri/TypeScript desktop and KMP/native
  mobile clients after the Runtime boundary stabilizes.

### Tech Stack

| Part | Language | Build tool | Workspace |
|------|----------|------------|-----------|
| `engine/`, `runtime/replica`, `cli`, `tui`, `bench` | Rust | Cargo workspace (root `Cargo.toml` members) | main |
| `desktop/` backend | Rust (Tauri) | cargo (workspace member) | main |
| `desktop/` frontend | TypeScript / React | pnpm (Tauri CLI orchestrates) | independent |
| `mobile/` | Kotlin (KMP) | Gradle | independent |
| `experiments/engine-kt/` | Experimental Kotlin Engine + verification adapters | Gradle | independent |
| `runtime/gateway/` | Go (deferred) | go build / go mod | independent |
| `spec/proto/` | — | buf / protoc codegen → Rust + Kotlin | single source |

## Repository Rules (apply repo-wide)

@.agents/engineering-rules.md
@.agents/dependency-versions.md
@.agents/git-workflow.md
@.agents/conventions.md
@.agents/multi-language.md
@.agents/testing.md
@.agents/doc-style.md
@.agents/ddd.md
@.agents/architecture.md
