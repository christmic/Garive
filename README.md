# Garive

Garive is a from-scratch Agent platform informed by Sylvander's lessons, not a
source-level fork. Its governing rule is: Agent decides, Runtime persists and
executes, adapters translate external protocols, and clients own presentation
only.

## Implemented foundation

- Rust implements the production-first Agent C0-C6 semantics. The experimental
  Kotlin Engine checks admitted portable slices against the same specs and
  fixtures without becoming a second production implementation.
- Rust SQLite implements the durable host slice. The Kotlin experiment uses
  PostgreSQL to validate portability with real database tests.
- Rust/Kotlin OpenAI Responses and Anthropic Messages adapters share reviewed
  official-shape fixtures and strict terminal/retry contracts.
- Rust/Kotlin compatible Providers map neutral requests, outcomes and streams
  without endpoint, credential, environment or transport ownership.
- Host API v1 has generated Rust, Kotlin and KMP bindings plus semantic
  round-trips.
- CLI, TUI, Web, Tauri Desktop, Android and iOS provide executable fake-host
  shells; Android APK verification still requires a local SDK.

## Repository map

- `engine/`: portable Rust Agent, LLM, ledger and capability contracts.
- `adapters/`: Rust provider wire adapters.
- `providers/`: Rust neutral/protocol deployment composition; vendor profiles
  remain separate.
- `runtime/replica/`: Rust composition/storage boundary with SQLite.
- `experiments/engine-kt/`: experimental Kotlin Engine mirror and verification
  adapters; never a product Runtime or second source of truth.
- `spec/`: accepted behavior, wire schemas and cross-language fixtures.
- `docs/architecture/`: design research and the system map.
- `cli/`, `tui/`, `web/`, `desktop/`, `mobile/`: thin product surfaces over the
  Host boundary.

C7 measured compression, live network hosts, vendor profiles, production
credentials/deployment and the Go gateway remain explicitly gated.
See `spec/design/core-agent-plan.md` and
`spec/design/agent-platform-delivery.md` for the work graph and evidence rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable Agent platform foundation
