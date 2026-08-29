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
| C2 | Deterministic context derive | accepted | accepted | documented | implemented | verified | done | Keep `missing_docs`, boundary tests, and property tests green. |
| C3 | Bounded model-only execution | accepted | accepted | documented | implemented | verified | done | Keep the explicit no-tool boundary and model-only scenarios green. |
| C4 | Tool resolution and prepared calls | accepted | accepted | documented | implemented | verified | done | Keep Portable Tool Schema, canonical digest, native tests, and shared fixtures green. |
| C5 | Governed effects and observations | accepted | accepted | documented | implemented | verified | done | Keep preparation, authority, interaction, receipt, observation, recovery, and fake-Runtime ordering evidence green. |
| C6 | Durable Runtime Turn orchestration | accepted | accepted | documented | implemented | verified | done | Keep command mapping, governed execution, fixed-prefix query, leases, cancellation, continuation/reconciliation, and native restart matrices green. |
| C7 | Measured context compression | draft | missing | missing | missing | missing | gated | Record a C3/C6 baseline before accepting thresholds or algorithms. |
| L0 | Durable Ledger vocabulary and state | accepted | accepted | documented | implemented | verified | done | Keep exact C6 payloads, lifecycle ownership, iteration/abandon transitions, and shared Rust/Kotlin matrices green. |
| L1-R | SQLite Ledger adapter | accepted | accepted | documented | implemented | verified | done | Keep v1→v2/future-schema gates, leased writes, file restart matrix, and all shared ledger scenarios green. |
| L1-K | Kotlin PostgreSQL experiment | accepted | accepted | documented | implemented | verified | done | Keep real PostgreSQL shared scenarios, writer-race normalization, migration refusal, and admitted recovery-host subset green. |

## Protocols, Providers, Host, and clients

| ID | Slice | Design | Spec | API | Code | Tests | State | Next evidence |
|---|---|---|---|---|---|---|---|---|
| P1-O | Responses-compatible protocol adapters | accepted | accepted | documented | implemented | verified | done | Keep shared request/response/error/SSE fixtures, exact event catalogues, strict native builds, and adapter boundary gates green. |
| P1-A | Messages-compatible protocol adapters | accepted | accepted | documented | implemented | verified | done | Keep shared request/response/error/SSE fixtures, block/delta lifecycle matrices, strict native builds, and adapter boundary gates green. |
| P2-C | Compatible deployment Provider mapping | accepted | accepted | documented | implemented | verified | done | Keep explicit deployment boundaries, every shared failure case, and buffered/streamed Rust/Kotlin normalization green. |
| P2-V0 | Official vendor connection profiles | accepted | accepted | documented | implemented | verified | done | Keep explicit Runtime-supplied values, redacted diagnostics, exact error policies, every shared Rust/Kotlin fixture case, and Provider boundary gates green. |
| P2-VX | Hosted vendor capabilities | accepted | missing | missing | missing | missing | planned | Admit each hosted tool/special API only with its own neutral semantics, extension types and fixtures; never allowlist arbitrary extensions. |
| H0 | Host API v1 schema and bindings | accepted | accepted | documented | implemented | verified | done | Keep Proto SSOT field docs, generated-binding gate, and round-trip test green. |
| H1-T | Runtime-owned model HTTP transport | accepted | accepted | documented | implemented | verified | done | Keep explicit no-proxy/no-retry limits, exact failure classification, fragmented SSE, cancellation, and real-loopback matrices green. |
| H1 | Live durable Host | accepted | accepted | documented | implemented | verified | done | Keep durable command replay/conflict, commit-before-dispatch, restart projection, loopback-only HTTP/SSE, and shared failure fixtures green. |
| A-CLI | CLI shell | accepted | accepted | partial | partial | verified | active | Replace Fake Host only after H1 exists. |
| A-TUI | TUI shell | accepted | accepted | partial | partial | verified | active | Replace Fake Host only after H1 exists. |
| A-WEB | Web shell | accepted | accepted | partial | partial | verified | active | Replace Fake Host only after H1 exists. |
| A-DESKTOP | Tauri/React shell | accepted | accepted | partial | partial | verified | active | Replace Fake Host only after H1 exists. |
| A-MOBILE | KMP/Android/iOS shells | accepted | accepted | partial | partial | partial | active | Add the Android APK gate when an SDK is available; replace Fake Host after H1. |
| G0 | Go Gateway | accepted | missing | missing | skeleton | missing | gated | Admit only after a live Host requires a separately scaled edge. |
| B0 | SWE benchmark harness | accepted | missing | missing | skeleton | missing | gated | Admit after one real end-to-end Agent workflow exists. |

## Capability backlog

| Slice | Design | Spec | API | Code | Tests | State | Admission dependency |
|---|---|---|---|---|---|---|---|
| Memory | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin bounds and revision fixtures, exact L0 payloads, Runtime namespace/restricted authority, atomic writes, and SQLite commit-before-context/restart evidence green. |
| Knowledge | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin request/evidence/failure fixtures, exact L0 lifecycle transitions, explicit source authority, connector commit ordering, Core attribution, and SQLite crash-position recovery green. |
| Skill | accepted | accepted | documented | implemented | verified | done | Keep exact digest/order/bounds fixtures, Rust/Kotlin Core narrowing, L0 validation, and SQLite commit-before-model/restart evidence green. |
| Scheduler | accepted | accepted | documented | skeleton | missing | planned | Freeze Q0/CF0 fixtures, then implement recurrence and SQLite occurrence leasing. |
| Multi-Agent | accepted | accepted | documented | skeleton | missing | planned | Coordinate DelegationPending/result continuation fixtures, then implement budgeted parent/child recovery. |
| Creativity | draft | missing | missing | skeleton | missing | gated | Reproducible evaluation baseline. |
| Evaluation | draft | missing | missing | skeleton | missing | gated | Runnable Agent and pinned benchmark evidence. |
| Observability | accepted | accepted | documented | skeleton | missing | planned | Freeze O0 fixtures, then implement portable validation and bounded Runtime sinks. |

## Update checklist

1. Link concrete design, Spec, source, and test evidence in the affected change.
2. Change only the columns proved by that evidence.
3. Keep unsupported and gated states explicit.
4. Do not mark a parent phase done from compilation, fake-host shells, or a
   narrower child slice.

## See also

- [`design/core-agent-plan.md`](design/core-agent-plan.md) — dependency DAG and work packages.
- [`design/agent-platform-delivery.md`](design/agent-platform-delivery.md) — platform acceptance contract.
- [`design/agent-capability-spec-set.md`](design/agent-capability-spec-set.md) — draft post-H1 capability review set.
- [`../.agents/testing.md`](../.agents/testing.md) — evidence levels and repository gates.
- [`AGENTS.md`](AGENTS.md) — Spec admission and schema rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
