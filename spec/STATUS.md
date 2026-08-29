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
| C0 | Execution identity and bounded control | accepted | accepted | documented | implemented | verified | done | Keep `missing_docs` and shared control fixtures green. |
| C1 | Model facts and outcomes | accepted | accepted | documented | implemented | verified | done | Keep the normalized outcome API gate and shared fixtures green. |
| C1b | Model request and stream contract | accepted | accepted | documented | implemented | verified | done | Keep request/stream docs, validation fixtures, and API gate green. |
| C2 | Deterministic context derive | accepted | accepted | documented | implemented | verified | done | Keep `missing_docs`, boundary tests, and property tests green. |
| C3 | Bounded model-only execution | accepted | accepted | documented | implemented | verified | done | Keep the explicit no-tool boundary and model-only scenarios green. |
| C4 | Tool resolution and prepared calls | accepted | missing | missing | skeleton | missing | planned | Accept a focused Prepared Call/digest/replay Spec before implementation. |
| C5 | Governed effects and observations | accepted | missing | missing | skeleton | missing | planned | Accept authorization, interaction, receipt, and uncertain-effect Specs. |
| C6 | Durable Runtime Turn orchestration | accepted | draft | partial | partial | partial | active | Specify and prove one real persisted Turn across every crash boundary. |
| C7 | Measured context compression | draft | missing | missing | missing | missing | gated | Record a C3/C6 baseline before accepting thresholds or algorithms. |
| L0 | Durable Ledger vocabulary and state | accepted | accepted | documented | implemented | verified | done | Keep Rust/Kotlin scenario and exhaustive transition matrices green. |
| L1-R | SQLite Ledger adapter | accepted | accepted | partial | implemented | partial | active | Add migration/backup policy and the remaining process-boundary fault matrix. |
| L1-K | Kotlin PostgreSQL experiment | accepted | accepted | documented | implemented | partial | active | Complete the remaining process-boundary and concurrency fault matrix. |

## Providers, Host, and clients

| ID | Slice | Design | Spec | API | Code | Tests | State | Next evidence |
|---|---|---|---|---|---|---|---|---|
| P1-O | OpenAI Responses protocol adapter | accepted | accepted | documented | partial | verified | active | Add the real authenticated HTTP transport without changing normalized semantics. |
| P1-A | Anthropic Messages protocol adapter | accepted | accepted | documented | partial | verified | active | Add the real authenticated HTTP transport without changing normalized semantics. |
| H0 | Host API v1 schema and bindings | accepted | accepted | documented | implemented | verified | done | Keep Proto SSOT field docs, generated-binding gate, and round-trip test green. |
| H1 | Live durable Host | accepted | draft | missing | missing | missing | planned | Depends on C6 and one real provider transport. |
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
| Memory | accepted | missing | missing | skeleton | missing | planned | C5 ports and C6 durable facts. |
| Knowledge | accepted | missing | missing | skeleton | missing | planned | C5 ports and evidence attribution Spec. |
| Skill | accepted | missing | missing | skeleton | missing | planned | C4/C5 prepared and governed invocation. |
| Scheduler | accepted | missing | missing | skeleton | missing | planned | C6 durable continuation and Runtime clocks. |
| Multi-Agent | accepted | missing | missing | skeleton | missing | gated | C6 recovery plus delegation identity and budget Spec. |
| Creativity | draft | missing | missing | skeleton | missing | gated | Reproducible evaluation baseline. |
| Evaluation | draft | missing | missing | skeleton | missing | gated | Runnable Agent and pinned benchmark evidence. |
| Observability | accepted | missing | missing | skeleton | missing | planned | Stable C5/C6 event vocabulary. |

## Update checklist

1. Link concrete design, Spec, source, and test evidence in the affected change.
2. Change only the columns proved by that evidence.
3. Keep unsupported and gated states explicit.
4. Do not mark a parent phase done from compilation, fake-host shells, or a
   narrower child slice.

## See also

- [`design/core-agent-plan.md`](design/core-agent-plan.md) — dependency DAG and work packages.
- [`design/agent-platform-delivery.md`](design/agent-platform-delivery.md) — platform acceptance contract.
- [`../.agents/testing.md`](../.agents/testing.md) — evidence levels and repository gates.
- [`AGENTS.md`](AGENTS.md) — Spec admission and schema rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
