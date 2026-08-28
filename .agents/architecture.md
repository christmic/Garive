# Architecture

## High-level System

Garive's runtime is split across four cooperating tiers:

```
┌─────────────────────────────────────────────────────────────────┐
│                  Agent Apps (clients)                           │
│      Swift (macOS) · TypeScript (web) · other surfaces          │
└─────────────────────────┬───────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                  Agent Gateway (Go)                             │
│     Auth · rate limit · load balance · observability            │
└─────────────────────────┬───────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│            Core Agent (Rust · Kotlin mirror)                    │
│           loop · tools · safety · memory · knowledge            │
└─────────────────────────┬───────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                  Multi-channel Surfaces                         │
│              CLI · IDE · chat · IM · voice (TBD)                │
└─────────────────────────────────────────────────────────────────┘
```

## Core Agent — Cross-language Isomorphic Design

The Core Agent ships in **two source-of-truth mirrors**:

- **Rust** — primary, ship-to-production implementation.
  Performance-sensitive paths live here.
- **Kotlin** — synchronized mirror in `experiments/engine-kt/`. Same
  wire protocol, same semantics, shareable trait / test surface.
  Used by JVM-side services and Android-adjacent surfaces.

Both languages track each other; a protocol-level change updates
both in the same change set, verified by a shared conformance
suite.

## Engine Sub-directories

The Rust workspace (`engine/`) holds the Core Agent crates. Each
sub-directory lands when its slice is scoped.

| Path | Role |
|------|------|
| `core/` | Agent loop, runtime primitives, contracts. |
| `ledger/` | Durable, append-only event log (decisions, actions, outcomes). |
| `llm/` | Language-model abstraction (provider-agnostic). |
| `tools/` | Tool registry and execution surface. |
| `memory/` | Short- and long-term memory layers. |
| `knowledge/` | Knowledge store and retrieval. |
| `skill/` | Skill packaging, loading, execution. |
| `multiagent/` | Coordination primitives for multi-agent runs. |
| `scheduler/` | Task and turn scheduling. |
| `creativity/` | Value discovery, ideation, exploration. |
| `eval/` | Evaluation harness for agent behaviour. |
| `observability/` | Tracing, metrics, structured logs. |
| `config/` | Configuration schema and loaders. |
| `proto/` | Generated protobuf wire types (single source: `spec/proto/`). |

Adding a new engine crate = create the sub-dir + its `Cargo.toml`
+ register it in the root workspace `members`.

## Terminal Surfaces — `cli/` and `tui/`

Two terminal-resident entry points share the same Rust
workspace and the same engine crates. They split the
**use-case surface**, not the **protocol**:

| Crate | Position | Use when |
|-------|----------|----------|
| `cli/` | **One-shot** command. Single invocation, **pipe-friendly**, script-friendly. | One question → one answer. CI / scripts / Makefiles / shell. Non-interactive. Exit codes follow Unix conventions. No TTY required. |
| `tui/` | **Resident** terminal UI. Long-lived, interactive, rich output. | Multi-turn conversation with the agent. Streaming token output, in-progress tool calls, syntax-highlighted previews, keyboard shortcuts, multi-pane layouts. |

**Rule of thumb:** if it can run unattended → `cli/`. If the
user has to be present → `tui/`. The two never overlap;
neither one calls the other.

Both `cli/` and `tui/` are pure frontends over `engine/` —
they do not embed business logic. Engine behaviour (agent
loop, tool execution, memory, knowledge) lives in `engine/`
and is consumed by both surfaces via Cargo path deps.

See `cli/README.md` and `tui/README.md` for per-crate details.

## Runtime Tier

The `runtime/` tier hosts the agent core as a service.

| Path | Language | Role |
|------|----------|------|
| `replica/` | Rust | The replica — the service container that runs an Agent process. |
| `gateway/` | Go | High-throughput gateway (auth, rate limit, load balance, observability, routing). Independent `go.mod`. |

The replica embeds the Rust engine crates and exposes an interface
the gateway talks to. The gateway is **not** part of the Rust
workspace.

## Research Fronts

Beyond the core loop, Garive explores:

- **Self-drive** — the agent initiates work without explicit
  prompts when it detects signal worth acting on.
- **Value discovery** — the agent surfaces opportunities the user
  hasn't yet articulated.
- **Feedback loops** — every action returns a signal that the
  agent integrates into its next decision.

## Build Configuration

| Part | Language | Build tool | Workspace |
|------|----------|------------|-----------|
| `engine/`, `runtime/replica`, `cli`, `tui`, `bench` | Rust | Cargo workspace (root `Cargo.toml` members) | main |
| `desktop/` backend | Rust (Tauri) | cargo (workspace member) | main |
| `desktop/` frontend | TypeScript / React | pnpm (Tauri CLI orchestrates) | independent |
| `mobile/` | Kotlin (KMP) | Gradle | independent |
| `experiments/engine-kt/` | Kotlin | Gradle | independent |
| `runtime/gateway/` | Go | go build / go mod | independent |
| `spec/proto/` | — | buf / protoc codegen → Rust + Kotlin | single source |

Rules of thumb:

- **Main Rust workspace** holds everything that compiles to a Rust
  binary or library and is part of the canonical Rust toolchain.
- **Independent builds** (mobile, engine-kt, gateway,
  desktop frontend) live in their own package-manager trees
  (Gradle, go.mod, pnpm) and integrate via generated artifacts or
  inter-process calls.
- **`spec/proto/`** is the single source of truth for wire schemas.
  Generated Rust and Kotlin bindings ship into the respective
  workspaces; schemas do not drift.
