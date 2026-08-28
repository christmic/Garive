# docs/architecture/core/

> **Core Agent design.** Documents in this directory describe
> the Core Agent — the engine that drives a user message
> through to an answer. The Core lives in `engine/core/` (Rust
> primary) and `engine-kt/core/` (Kotlin mirror); both follow
> the same design.

## What's Here

| Doc | What it covers | Status |
|-----|----------------|--------|
| `loop.md` | Two-layer driver (`agent_loop` + `agent_turn`) and the **derive → invoke → judge → run** iteration. The turn state. Derive as incremental + stateful. Three-pass `assemble` (tier / evict / format). Per-tool policy profiles. Summary entry schema. Boundary invariants. Three-mechanism recap. | **draft (possible mechanism)** — shape settled as a candidate; specifics (thresholds, eviction triggers) are not committed |
| `ledger.md` | Append-only round log. Per-turn segments. Entry kinds catalog (user.message, assistant.message, intent, tool_result, verdict, effects, summary.v1, rewrite_directive, approval_request, ...). SQLite persistence. API surface. Multi-turn segments. | **draft (possible mechanism)** — kinds catalog + schema are candidates |
| `governance.md` | *(forthcoming)* Policy for `governance.judge(intent) → verdict`. Allow / deny / rewrite / AskUser decision tree. Where policy lives, how it gets updated. |
| `scheduler.md` | *(forthcoming)* Turn scheduling across a single process; interaction with multi-agent runs. |
| `multiagent.md` | *(forthcoming)* Multi-agent coordination, sessions, fan-out / fan-in. |
| `governance.md` | *(forthcoming)* Policy for `governance.judge(intent) → verdict`. Allow / deny / rewrite / AskUser decision tree. Where policy lives, how it gets updated. |
| `scheduler.md` | *(forthcoming)* Turn scheduling across a single process; interaction with multi-agent runs. |
| `multiagent.md` | *(forthcoming)* Multi-agent coordination, sessions, fan-out / fan-in. |

## Layering

`core/` is one layer of `docs/architecture/`. Higher layers
(when they exist) consume `core/` as input. Lower layers (when
they exist) elaborate `core/` into specific surfaces.

```
architecture/
├── system-overview.md     high-level system architecture (forthcoming)
├── core/                  Core Agent — this directory
│   ├── loop.md            agent_loop / agent_turn / iteration
│   ├── ledger.md          append-only round log + entry kinds
│   └── ...
├── infra/                 runtime, gateway, replica (forthcoming)
├── client/                 mobile, desktop, macos-native (forthcoming)
└── cross-cutting/          multi-language, conformance, etc. (forthcoming)
```

`core/` documents are **deliberative**. When a slice here
settles (loop is the closest), the relevant excerpt moves to
`spec/design/<slice>.md` and `core/<file>.md` is updated with a
"Status: superseded by …" pointer.

## Status

`loop.md` and `ledger.md` are both **draft (possible
mechanism)** — shape is settled as a candidate; specifics
(payload encodings, schema indexes, threshold numbers, eviction
triggers) land with the slice. `governance.md`,
`scheduler.md`, and `multiagent.md` are placeholders — when
they land, they get the same `## Meta` block, the same
Context / Options / Decision / Consequences / Open
Questions / Known Limitations skeleton, and the same
cross-link discipline.

## See also

- `AGENTS.md` — repo-wide rules (the constitution).
- `docs/README.md` — doc hierarchy (docs → spec → constitution → code).
- `.agents/ddd.md` — domain-driven design pipeline.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: drafting — one design landed (`loop.md`); siblings pending.