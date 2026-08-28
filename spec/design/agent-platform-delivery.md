# Agent platform delivery contract

## Status

Accepted delivery map for the next Garive implementation phase. It extends the
Agent architecture and execution contracts without changing their ownership
rules.

## Purpose

Turn the accepted Agent boundary into two executable server-capable
implementations, durable hosts, verified provider adapters, and buildable
product clients. This document fixes module ownership and sequencing; focused
behavior remains in slice-specific specs.

## Required outcomes

The phase is complete only when all rows have executable evidence:

| Area | Required outcome | Evidence |
|---|---|---|
| Agent Core | Deterministic context derivation and a bounded model-only loop. | Rust/Kotlin shared fixtures plus native tests. |
| Ledger contract | Durable Turn/fact/invocation vocabulary with monotonic positions and atomic terminal writes. | Domain tests in both languages. |
| Rust host | SQLite-backed Runtime persistence and restart recovery. | Real temporary SQLite crash-boundary tests. |
| Kotlin host | PostgreSQL-backed server persistence; no embedded/mobile database claim. | Real PostgreSQL integration tests in a disposable database. |
| OpenAI | Responses API request, response, SSE stream and error normalization. | Official-shape fixtures consumed by Rust/Kotlin adapters. |
| Anthropic | Messages API request, response, SSE stream and error normalization. | Official-shape fixtures consumed by Rust/Kotlin adapters. |
| Host API | Versioned Session/Turn/event/status contract for all clients. | Generated consumers and semantic round trips. |
| Product surfaces | CLI, TUI, Web, Desktop, Android and iOS boot against one host abstraction. | Native build plus one fake-host interaction per surface. |

Passing compilation without the named boundary test is not completion.

## Source layout

```text
engine/                         Rust portable Agent domain
  core/                         context + bounded loop
  llm/                          provider-neutral model contract
  ledger/                       durable vocabulary and ports

adapters/                       concrete external protocols
  llm-openai/                   Rust OpenAI Responses adapter
  llm-anthropic/                Rust Anthropic Messages adapter

runtime/
  replica/                      Rust composition root + SQLite adapter
  server-kt/                    Kotlin server Gradle build
    agent-core/                 portable C0-C5 semantics
    llm-contract/               provider-neutral model values
    ledger-contract/            durable vocabulary and ports
    persistence-postgres/       PostgreSQL adapter and migrations
    provider-openai/            OpenAI Responses adapter
    provider-anthropic/         Anthropic Messages adapter
    server-host/                Kotlin server composition root

cli/                            Rust one-shot host client
tui/                            Rust interactive terminal host client
web/                            browser client
desktop/backend/                Tauri host/client bridge
desktop/frontend/               desktop React surface
mobile/shared/                  KMP host client
mobile/androidApp/              Compose UI
mobile/iosApp/                  SwiftUI UI
```

The currently supported code under `experiments/engine-kt/` is promoted into
`runtime/server-kt/`; it is not copied. The Kotlin server remains an independent
implementation of admitted semantics, not a source mirror of Rust.

## Dependency direction

```text
Apps -> versioned Host API -> Runtime host -> Engine contracts
                                      |
                                      +-> persistence adapter
                                      `-> provider adapter -> official wire API
```

- Rust Engine never imports adapters, Runtime, SQL, HTTP, or App types.
- Provider adapters depend on `garive-llm`, not on Core or Runtime policy.
- Runtime owns credentials, concrete clients, persistence, transactions,
  recovery, retry budgets and composition.
- Kotlin domain modules do not depend on Ktor, JDBC, Exposed, jOOQ or a server
  framework. Those dependencies begin in adapter/host modules.
- Apps never import Engine internals or database adapters.

## Work graph

```text
C1b model request/stream facts ───────┐
                                      ├─> C3 bounded model-only loop
C2 deterministic context ─────────────┘              |
                                                     v
L0 ledger vocabulary/ports ───────────────> L1 durable hosts
                                                     |
P0 official protocol evidence ─> P1 adapters ────────+
                                                     |
H0 Host API ─────────────────────────────────────────+
          └─> CLI/TUI/Web/Desktop/Mobile skeletons
```

The graph allows protocol adapters and product skeletons to progress in
parallel with Core, but no surface may bypass the Host API or claim a complete
Agent workflow before C3/L1 exist.

## Agent slices

### C1b — request and stream

Add immutable request identity, capability target, ordered inputs, output
limits, admitted tool descriptors, model event facts, observer contract and an
async/suspending Model port. Provider-specific fields remain outside.

### C2 — deterministic context

Derive a purpose-specific, bounded surface from ordered durable candidates.
Required facts are never silently dropped; optional facts use a deterministic
newest-first retention rule and final chronological ordering. Redaction and
purpose exclusion are explicit results.

### C3 — bounded loop

Execute one disposable Kernel Execution using frozen ports. Cover completion,
context overflow rebuild, partial output, unavailability, cancellation,
iteration limit, missing usage and port failure. Suspension closes the current
execution.

## Ledger slices

### L0 — portable vocabulary

Both languages implement typed Session/Turn/Execution/request/invocation IDs,
monotonic fact positions, fact kinds, terminal outcomes, prepared-call digest,
effect lifecycle, read cursor and append transaction contract.

### L1-Rust — SQLite

`runtime/replica` owns schema migrations and a SQLite adapter configured for
foreign keys and WAL. A transaction atomically appends facts and advances the
Turn terminal/cursor. Restart tests use a real database file and verify
request-before-dispatch, terminal atomicity and uncertain-effect recovery.

### L1-Kotlin — PostgreSQL

`runtime/server-kt/persistence-postgres` owns PostgreSQL migrations and the
JDBC/R2DBC adapter selected by its focused spec. Tests run against a disposable
PostgreSQL instance and verify the same semantic fixtures plus transaction
isolation and unique invocation constraints. No H2/SQLite substitute proves
PostgreSQL behavior.

## Provider slices

### Protocol evidence

Each adapter spec records:

- official documentation URL and retrieval/review date;
- official SDK repository, exact commit/tag and inspected type paths;
- endpoint, required headers, request union, response union, stream event
  sequence, usage fields and error envelope;
- the supported subset and explicit unknown/unsupported behavior.

Provider fixtures are sanitized captures or minimal examples derived directly
from those official schemas. Fixtures invented from Garive domain types are
not protocol evidence.

### OpenAI module

The first admitted protocol is `POST /v1/responses` with SSE streaming. Chat
Completions compatibility is outside this slice. The adapter preserves ordered
output items, function-call argument deltas, reasoning references, refusal,
terminal response, usage and unknown event types for audit. Only
`response.completed` produces a completed model fact.

### Anthropic module

The first admitted protocol is `POST /v1/messages` with SSE streaming. The
adapter preserves content-block indexes, text/thinking/signature/tool-input
deltas, stop reason, cache-aware usage and error events. A stream completes
only after a valid `message_stop` following the required lifecycle.

## Product surfaces

The initial skeleton is intentionally small but executable:

| Surface | First behavior |
|---|---|
| CLI | Submit one Turn, render final/typed terminal, return documented exit code. |
| TUI | Display ordered fake-host events and one terminal state. |
| Web | Boot a strict TypeScript app and render Session/Turn state from a fake Host client. |
| Desktop | Tauri backend command and React frontend share the Host DTO contract. |
| Android | Compose app calls KMP fake Host client and renders terminal state. |
| iOS | SwiftUI app calls the generated/shared bridge and renders terminal state. |

Skeleton means runnable structure with a test, not empty directories or a
command that prints “not wired”. Credentials, signing, distribution and real
network deployment remain later product slices.

## Shared evolution

- C0-C3 and L0 are jointly implemented in Rust/Kotlin from shared semantic
  fixtures.
- Provider adapters share official wire fixtures but keep language-native wire
  models and parsers.
- SQLite and PostgreSQL share semantic ledger scenarios, not SQL text or byte
  identity.
- Host protobuf/JSON compatibility is a wire gate for every admitted client.
- Unsupported capability is explicit; fallback never changes safety semantics.

## Merge gates

Every landed slice includes:

1. focused accepted spec and evidence coordinates;
2. fixtures and a schema/version owner;
3. implementation-native tests;
4. cross-language semantic/wire checks where admitted;
5. strict Rust and pinned Gradle builds;
6. real SQLite/PostgreSQL/provider parser tests where behavior is claimed;
7. truthful support matrix and build recipes.

## Non-goals

- copying Sylvander types without re-verifying current official contracts;
- provider-neutralizing away protocol facts required for correct recovery;
- using a mock database to claim SQLite/PostgreSQL durability;
- embedding Agent policy in adapters or UI clients;
- declaring every future provider, tool, App feature or release pipeline done.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
