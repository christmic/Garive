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
- Rust/Kotlin official vendor profiles turn explicit Runtime-supplied endpoint
  and credential values into validated, redacted adapter configuration and
  exact error policies without loading configuration or executing HTTP.
- Host API v1 has generated Rust, Kotlin and KMP bindings plus semantic
  round-trips.
- CLI, TUI and Web consume live H1; Tauri Desktop embeds R1; Android and iOS
  consume the shared live KMP H1 client. Android SDK 36 APK assembly and API 36
  Compose instrumentation are verified alongside KMP and Swift native gates.
- The componentized TUI includes responsive dark/light/monochrome presentation,
  a shared render/hit conversation-position rail with bounded hover previews,
  typed keyboard discovery, terminal-safe external draft editing, and a linear
  screen-reader mode. See the [TUI user guide](docs/manual/tui-user-guide.md).

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

C7 measured compression, live network hosts, hosted vendor capabilities,
production credential resolution/deployment and the Go gateway remain
explicitly gated.
See `spec/design/core-agent-plan.md` and
`spec/design/agent-platform-delivery.md` for the work graph and evidence rules.
See `docs/deployment-from-source.md` for the new-machine build, configuration,
launch, verification, migration, and release-boundary runbook.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable Agent platform foundation
