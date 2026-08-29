# Agent platform delivery contract

## Status

Accepted delivery map for the next Garive implementation phase. It extends the
Agent architecture and execution contracts without changing their ownership
rules.

Current implementation evidence and remaining work are tracked only in
[`../STATUS.md`](../STATUS.md). This document defines acceptance; it does not
duplicate mutable progress claims.

## Purpose

Turn the accepted Agent boundary into one production-first Rust Runtime,
verified provider adapters and buildable product clients, while using the
Kotlin Engine experiment to test portability assumptions. This document fixes
module ownership and sequencing; focused behavior remains in slice-specific
specs.

## Required outcomes

The phase is complete only when all rows have executable evidence:

| Area | Required outcome | Evidence |
|---|---|---|
| Agent Core | Deterministic context derivation and a bounded model-only loop. | Rust native tests; experimental Kotlin conformance over shared fixtures. |
| Ledger contract | Durable Turn/fact/invocation vocabulary with monotonic positions and atomic terminal writes. | Rust domain tests plus the declared Kotlin experiment. |
| Rust host | SQLite-backed Runtime persistence and restart recovery. | Real temporary SQLite crash-boundary tests. |
| Kotlin experiment host | PostgreSQL-backed portability verification; no product-server or embedded/mobile database claim. | Real PostgreSQL integration tests in a disposable database. |
| OpenAI | Responses API request, response, SSE stream and error normalization. | Official-shape fixtures consumed by Rust/Kotlin adapters. |
| Anthropic | Messages API request, response, SSE stream and error normalization. | Official-shape fixtures consumed by Rust/Kotlin adapters. |
| Host API | Versioned Session/Turn/event/status contract for all clients. | Generated consumers and semantic round trips. |
| Product surfaces | CLI, TUI, Web, Desktop, Android and iOS boot against one Host abstraction. | Native build plus live H1 evidence for migrated surfaces; fixture clients are test support only. |

Passing compilation without the named boundary test is not completion.

## Source layout

```text
engine/                         Rust portable Agent domain
  core/                         context + bounded loop
  llm/                          provider-neutral model contract
  ledger/                       durable vocabulary and ports

adapters/                       concrete external protocols
  openai-responses/             Rust Responses-compatible protocol adapter
  anthropic-messages/           Rust Messages-compatible protocol adapter

runtime/
  replica/                      Rust composition root + SQLite adapter

clients/
  host-rs/                      shared Rust H1 HTTP/SSE client for CLI/TUI

experiments/
  engine-kt/                    experimental Kotlin Engine Gradle build
    core/                       admitted portable semantics
    llm/                        provider-neutral model values
    ledger/                     durable vocabulary and ports
    persistence-postgres/       PostgreSQL adapter and migrations
    adapter-openai-responses/   Kotlin Responses-compatible protocol adapter
    adapter-anthropic-messages/ Kotlin Messages-compatible protocol adapter
    proto/                      generated experimental bindings
    server-host/                executable verification fixture

cli/                            Rust one-shot host client
tui/                            Rust interactive terminal host client
web/                            browser client
desktop/backend/                Tauri host/client bridge
desktop/frontend/               desktop React surface
mobile/shared/                  KMP host client
mobile/androidApp/              Compose UI
mobile/iosApp/                  SwiftUI UI
```

`experiments/engine-kt/` deliberately remains outside `engine/` and `runtime/`.
It is an independent implementation of admitted semantics for research and
conformance, not a source mirror, production Engine, or product Runtime.

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
- Kotlin experimental domain modules do not depend on Ktor, JDBC, Exposed,
  jOOQ or a server framework. Those dependencies begin in experiment
  adapter/host modules.
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
          └─> CLI/TUI/Web/Desktop/Mobile clients
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

Rust owns the production typed Session/Turn/Execution/request/invocation IDs,
monotonic fact positions, fact kinds, terminal outcomes, prepared-call digest,
effect lifecycle, read cursor and append transaction contract. Kotlin
independently checks the admitted portable subset.

### L1-Rust — SQLite

`runtime/replica` owns schema migrations and a SQLite adapter configured for
foreign keys and WAL. A transaction atomically appends facts and advances the
Turn terminal/cursor. Restart tests use a real database file and verify
request-before-dispatch, terminal atomicity and uncertain-effect recovery.

### L1-Kotlin experiment — PostgreSQL

`experiments/engine-kt/persistence-postgres` owns PostgreSQL migrations and the
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

Vendor connection profiles are a separate Provider slice. They format only
explicit Runtime-supplied endpoint and credential values, produce exact error
policy, and own no environment lookup, credential store, HTTP attempt or retry.
Hosted capabilities require individual extension Specs rather than a generic
allowlist.

### OpenAI module

The first admitted protocol is `POST /v1/responses` with SSE streaming. Chat
Completions compatibility is outside this slice. The adapter preserves ordered
output items, function-call argument deltas, reasoning references, refusal,
terminal response and usage. Unknown semantic event types fail closed; sanitized
transport telemetry is Runtime-owned. Only
`response.completed` produces a completed model fact.

### Anthropic module

The first admitted protocol is `POST /v1/messages` with SSE streaming. The
adapter preserves content-block indexes, text/thinking/signature/tool-input
deltas, stop reason, cache-aware usage and error events. A stream completes
only after a valid `message_stop` following the required lifecycle.

## Product surfaces

Each surface starts with a deliberately bounded executable slice:

| Surface | First behavior |
|---|---|
| CLI | Submit one Turn, render final/typed terminal, return documented exit code. |
| TUI | Display ordered durable H1 events, cursor and one terminal state. |
| Web | Use a strict injectable H1 HTTP/SSE client and render committed Session/Turn state. |
| Desktop | Tauri backend embeds R1 and React consumes a typed terminal IPC result; backend configuration remains separate. |
| Android | Compose app calls KMP fake Host client and renders terminal state. |
| iOS | SwiftUI app calls the generated/shared bridge and renders terminal state. |

Skeleton means runnable structure with a test, not empty directories or a
command that prints “not wired”. Credentials, signing, distribution and real
network deployment remain later product slices.

## Shared evolution

- C0-C3 and L0 are production-first in Rust and experimentally checked in
  Kotlin from shared semantic fixtures.
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
5. strict Rust builds and pinned Gradle conformance builds where claimed;
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
