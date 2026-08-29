# Delivery status board

> Single source of truth for Garive design, specification, API, implementation,
> and verification progress. Update the affected row in the same change set as
> its evidence; planning documents link here instead of copying status claims.

## Audience

Maintainers selecting the next implementation slice and reviewers deciding
whether a completion claim is supported by repository evidence.

## Status rules

| Field | Values | Meaning |
|---|---|---|
| Design | accepted, draft, missing | The problem and ownership decision exist under `docs/`. |
| Spec | accepted, draft, missing | An implementation-ready contract exists under `spec/design/`. |
| API | documented, partial, missing | Public definitions, invariants, failures, and examples are documented. |
| Code | implemented, partial, skeleton, missing | The declared slice exists without implying later slices. |
| Tests | verified, partial, missing | Executable evidence covers the accepted slice. |
| State | done, active, planned, gated | Overall delivery state derived from the preceding evidence. |

A row is `done` only when its accepted scope has accepted design and Spec,
documented API, implemented code, and verified tests. A later slice may remain
planned without reopening a narrower completed row.

## Core Agent and durability

| ID | Slice | Design | Spec | API | Code | Tests | State | Next evidence |
|---|---|---|---|---|---|---|---|---|
| D0 | Agent Definition and effective snapshot | accepted | accepted | documented | implemented | verified | done | Keep exact resolution, canonical digests, immutable bindings, and shared Rust/Kotlin fixtures green. |
| C0 | Execution identity and bounded control | accepted | accepted | documented | implemented | verified | done | Keep `missing_docs` and shared control fixtures green. |
| C1 | Model facts and outcomes | accepted | accepted | documented | implemented | verified | done | Keep the normalized outcome API gate and shared fixtures green. |
| C1b | Model request and stream contract | accepted | accepted | documented | implemented | verified | done | Keep request/stream docs, validation fixtures, and API gate green. |
| C2 | Deterministic context derive | accepted | accepted | documented | implemented | verified | done | Keep `missing_docs`, boundary/property tests and shared capability-admission fixture green; Core remains the sole derive owner. |
| C3 | Bounded model-only execution | accepted | accepted | documented | implemented | verified | done | Keep the explicit no-tool boundary and model-only scenarios green. |
| C4 | Tool resolution and prepared calls | accepted | accepted | documented | implemented | verified | done | Keep Portable Tool Schema, canonical digest, native tests, and shared fixtures green. |
| C5 | Governed effects and observations | accepted | accepted | documented | implemented | verified | done | Keep preparation, authority, interaction, receipt, observation, recovery, and fake-Runtime ordering evidence green. |
| C6 | Durable Runtime Turn orchestration | accepted | accepted | documented | implemented | verified | done | Keep command mapping, governed execution, fixed-prefix query, leases, cancellation, continuation/reconciliation, and native restart matrices green. |
| C7-A | Context-pressure baseline evidence | accepted | accepted | documented | implemented | verified | done | Keep strict corpus/process/CLI gates green; run an admitted provider counter to produce the separate publication-grade baseline. |
| C7-B | Exact provider counter composition | accepted | accepted | documented | implemented | verified | done | Keep all four corpus routes, fail-closed boundaries, secret-invariant/non-secret-variant digest tests and no-implicit-loader scan green; a live publication run remains C7 evidence. |
| C7-C | Publication-grade context-pressure runner | accepted | accepted | documented | implemented | verified | done | Keep strict tagged configs, permanent command non-publication, OS credential resolution, clean Git attestation and bounded no-retry HTTPS loopback/failure gates green; a live credentialed run remains C7 evidence. |
| C7 | Measured context compression | draft | missing | missing | missing | missing | gated | Publish and review a C7-A baseline before accepting thresholds or algorithms. |
| L0 | Durable Ledger vocabulary and state | accepted | accepted | documented | implemented | verified | done | Keep exact C6 payloads, lifecycle ownership, iteration/abandon transitions, and shared Rust/Kotlin matrices green. |
| L1-R | SQLite Ledger adapter | accepted | accepted | documented | implemented | verified | done | Keep v1→v3/future-schema gates, execution/schedule fenced writes, file restart matrices, and all shared ledger scenarios green. |
| L1-K | Kotlin PostgreSQL experiment | accepted | accepted | documented | implemented | verified | done | Keep real PostgreSQL shared scenarios, writer-race normalization, migration refusal, and admitted recovery-host subset green. |

## Protocols, Providers, Host, and clients

| ID | Slice | Design | Spec | API | Code | Tests | State | Next evidence |
|---|---|---|---|---|---|---|---|---|
| P1-O | Responses-compatible protocol adapters | accepted | accepted | documented | implemented | verified | done | Keep shared request/response/error/SSE fixtures, exact event catalogues, strict native builds, and adapter boundary gates green. |
| P1-A | Messages-compatible protocol adapters | accepted | accepted | documented | implemented | verified | done | Keep shared request/response/error/SSE fixtures, block/delta lifecycle matrices, strict native builds, and adapter boundary gates green. |
| P2-C | Compatible deployment Provider mapping | accepted | accepted | documented | implemented | verified | done | Keep explicit deployment boundaries, every shared failure case, and buffered/streamed Rust/Kotlin normalization green. |
| P2-V0 | Official vendor connection profiles | accepted | accepted | documented | implemented | verified | done | Keep explicit Runtime-supplied values, redacted diagnostics, exact error policies, every shared Rust/Kotlin fixture case, and Provider boundary gates green. |
| P2-VX | Hosted vendor capabilities | accepted | missing | missing | missing | missing | planned | Admit each hosted tool/special API only with its own neutral semantics, extension types and fixtures; never allowlist arbitrary extensions. |
| P2-VX-ATC | Anthropic exact input-token count | accepted | accepted | documented | implemented | verified | done | Keep exact projection/profile/response fixtures and no-environment/no-transport gates green; a credentialed C7-A publication run is separate evidence. |
| H0 | Host API v1 schema and bindings | accepted | accepted | documented | implemented | verified | done | Keep Proto SSOT field docs, generated-binding gate, and round-trip test green. |
| H1-T | Runtime-owned model HTTP transport | accepted | accepted | documented | implemented | verified | done | Keep explicit no-proxy/no-retry limits, exact failure classification, fragmented SSE, cancellation, and real-loopback matrices green. |
| H1 | Live durable Host | accepted | accepted | documented | implemented | verified | done | Keep durable command replay/conflict, commit-before-dispatch, restart projection, loopback-only HTTP/SSE, and shared failure fixtures green. |
| R1 | Local Runtime composition | accepted | accepted | documented | implemented | verified | done | Keep explicit configuration, fixed-prefix reconstruction, post-commit queue, real protocol flow, bounded shutdown and process-kill recovery gates green. |
| A-CLI | CLI shell | accepted | accepted | documented | implemented | verified | done | Keep explicit create/reuse, real-loopback H1, terminal output, stable command identity and exit-code tests green. |
| A-TUI | TUI shell | accepted | accepted | documented | implemented | verified | done | Keep explicit loopback H1 and ordered durable event/cursor rendering tests green; resident multi-turn UX is a later slice. |
| A-WEB | Web shell | accepted | accepted | documented | implemented | verified | done | Keep strict injectable HTTP/SSE, all H1 mutations, shared reducer fixture and production TypeScript build green. |
| A-DESKTOP | Tauri/React shell | accepted | accepted | documented | implemented | verified | done | Keep embedded R1, typed IPC, backend-only configured startup and temporary-SQLite/real-protocol loops green. |
| A-DESKTOP-C | Desktop backend system configuration | accepted | accepted | documented | implemented | verified | done | Keep strict document parsing, injected secret/profile registries, OS credential resolution and configured startup gates green. |
| A-MOBILE | KMP/Android/iOS shells | accepted | accepted | documented | implemented | verified | done | Keep KMP JVM/real-H1, XCFramework, Swift, Android SDK 36 APK and API 36 Compose instrumentation gates green. Signing and distribution remain later product slices. |
| G0 | Go Gateway | accepted | missing | missing | missing | missing | gated | Admit only after a live Host requires a separately scaled edge. |
| B0 | SWE benchmark harness | accepted | accepted | documented | implemented | verified | done | Keep strict official loading, the sole bounded concurrent route, release-once failure matrix, explicit command ports, unified-diff/prediction adapters, pinned official report coverage, JSONL tracking and CLI E2E green. Real Docker publication evidence remains external and gated. |

## Capability backlog

| Slice | Design | Spec | API | Code | Tests | State | Admission dependency |
|---|---|---|---|---|---|---|---|
| Memory | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin bounds/revision/capability-admission fixtures, exact L0 payloads, Runtime authority, atomic writes, and SQLite commit-before-C2/restart evidence green. |
| Memory M1 | accepted | accepted | documented | implemented | verified | done | Keep M1-A through M1-G green: shared Rust/Kotlin lifecycle/maintenance/recall semantics, exact L0 facts, SQLite recovery, committed-recall C2 admission and the pinned synthetic quality regression. Representative empirical quality remains a separate evidence gate. |
| Knowledge | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin request/evidence/failure/capability-admission fixtures, exact L0 transitions, source authority, connector commit ordering, C2 attribution, and SQLite recovery green. |
| Skill | accepted | accepted | documented | implemented | verified | done | Keep exact digest/order/bounds and capability-admission fixtures, Rust/Kotlin narrowing, L0 validation, and SQLite commit-before-C2/restart evidence green. |
| Scheduler | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin recurrence/failure properties, exact L0 facts, SQLite lease races, authority/update conflicts, real C6 dispatch, restart and process-kill matrices green. |
| Multi-Agent | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin canonical intent/budget/result properties, exact L0 lifecycle projection, durable grant-before-child ordering, cancellation/isolation, SQLite restart and six-boundary process-kill matrices green. |
| Creativity | draft | missing | missing | skeleton | missing | gated | Execute and review representative CR-B external paired evidence before admitting production behavior or thresholds. |
| Creativity CR-A | accepted | accepted | documented | implemented | verified | done | Keep the four-class strict corpus, exact paired reducer, blind bounded command ports, content-free evidence CLI and empty production Creativity boundary green; CR-B external evidence remains gated. |
| Creativity CR-B | accepted | accepted | documented | implemented | verified | done | Keep compatible-dialect model ports, OS credential references, exact clean Git attestation, transport failure matrix and content-free evidence v2 gates green; execute and review real external runs separately. |
| Evaluation | accepted | accepted | documented | implemented | verified | done | Keep exact rational score, duplicate/bound failures, baseline provenance and pure-Engine boundary gates green. |
| Observability | accepted | accepted | documented | implemented | verified | done | Keep the shared Rust/Kotlin catalogue, canonical digest, forbidden-label properties, explicit Runtime limits, commit-position, sampling, priority, backpressure, redaction-canary and bounded-shutdown gates green. |

## Update checklist

1. Link concrete design, Spec, source, and test evidence in the affected change.
2. Change only the columns proved by that evidence.
3. Keep unsupported and gated states explicit.
4. Do not mark a parent phase done from compilation, fake-host shells, or a
   narrower child slice.

## See also

- [`design/remaining-admission-audit.md`](design/remaining-admission-audit.md) — evidence-based decisions for every remaining gated/planned slice.
- [`design/core-agent-plan.md`](design/core-agent-plan.md) — dependency DAG and work packages.
- [`design/agent-platform-delivery.md`](design/agent-platform-delivery.md) — platform acceptance contract.
- [`design/agent-capability-spec-set.md`](design/agent-capability-spec-set.md) — draft post-H1 capability review set.
- [`design/desktop-system-configuration.md`](design/desktop-system-configuration.md) — backend-only Desktop configuration contract.
- [`../.agents/testing.md`](../.agents/testing.md) — evidence levels and repository gates.
- [`AGENTS.md`](AGENTS.md) — Spec admission and schema rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
