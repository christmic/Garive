# Garive

Garive is a from-scratch Agent platform informed by Sylvander's lessons, not a
source-level fork. Its governing rule is: Agent decides, Runtime persists and
executes, adapters translate external protocols, and clients own presentation
only.

## Implemented foundation

- Rust and Kotlin independently implement portable Agent C0-C3 semantics from
  accepted specs and shared fixtures.
- Rust SQLite and Kotlin PostgreSQL adapters implement the durable ledger slice
  with real database tests.
- Rust/Kotlin OpenAI Responses and Anthropic Messages adapters share reviewed
  official-shape fixtures and strict terminal/retry contracts.
- Host API v1 has generated Rust, Kotlin and KMP bindings plus semantic
  round-trips.
- CLI, TUI, Web, Tauri Desktop, Android and iOS provide executable fake-host
  shells; Android APK verification still requires a local SDK.

## Repository map

- `engine/`: portable Rust Agent, LLM, ledger and capability contracts.
- `adapters/`: Rust provider wire adapters.
- `runtime/replica/`: Rust composition/storage boundary with SQLite.
- `runtime/server-kt/`: Kotlin Agent server, PostgreSQL/providers and executable
  server composition root.
- `spec/`: accepted behavior, wire schemas and cross-language fixtures.
- `docs/architecture/`: design research and the system map.
- `cli/`, `tui/`, `web/`, `desktop/`, `mobile/`: thin product surfaces over the
  Host boundary.

C4-C7 governed tools, full durable Turn orchestration, live network hosts,
production credentials/deployment and the Go gateway remain explicitly gated.
See `spec/design/core-agent-plan.md` and
`spec/design/agent-platform-delivery.md` for the work graph and evidence rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable Agent platform foundation
