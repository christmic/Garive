# docs/architecture/core/

> **Core Agent design.** Documents in this directory describe
> the Core Agent — the engine that drives a user message
> through to an answer. The Core lives in `engine/core/` (Rust
> primary) and `engine-kt/core/` (Kotlin mirror); both follow
> the same design.

## What's Here

| Doc | What it covers | Status |
|-----|----------------|--------|
| `loop.md` | Two-layer driver (`agent_loop` + `agent_turn`) and the **derive → invoke → judge → run** iteration. Ledger as single source of truth. Governance as queried port. Suspended / Resume. | draft — loop skeleton settled |
| `ledger.md` | *(forthcoming)* Append-only event log, derive path, surface shapes, summarisation, replay semantics. |
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
│   ├── ledger.md          event log + derive (forthcoming)
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

`loop.md` is the **only settled design** today. The other
docs above are placeholders — when they land, they get the
same `## Meta` block, the same Context / Options / Decision
/ Consequences / Open Questions / Known Limitations skeleton,
and the same cross-link discipline.

## See also

- `AGENTS.md` — repo-wide rules (the constitution).
- `docs/README.md` — doc hierarchy (docs → spec → constitution → code).
- `.agents/ddd.md` — domain-driven design pipeline.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: drafting — one design landed (`loop.md`); siblings pending.