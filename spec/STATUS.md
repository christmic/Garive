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
| C5b | Deterministic governed effect batches | accepted | accepted | documented | implemented | verified | done | Keep Rust/Kotlin plan/transition parity, ordered SQLite publication, fake-clock timeout/cancel/recovery, confined-executor, sequential-differential and source-boundary gates green. |
| C6 | Durable Runtime Turn orchestration | accepted | accepted | documented | implemented | verified | done | Keep command mapping, governed execution, fixed-prefix query, leases, cancellation, continuation/reconciliation, and native restart matrices green. |
| C7-A | Context-pressure baseline evidence | accepted | accepted | documented | implemented | verified | done | Keep strict corpus/process/CLI gates green; run an admitted provider counter to produce the separate publication-grade baseline. |
| C7-B | Exact provider counter composition | accepted | accepted | documented | implemented | verified | done | Keep all four corpus routes, fail-closed boundaries, secret-invariant/non-secret-variant digest tests and no-implicit-loader scan green; a live publication run remains C7 evidence. |
| C7-C | Publication-grade context-pressure runner | accepted | accepted | documented | implemented | verified | done | Keep strict tagged configs, permanent command non-publication, OS credential resolution, clean Git attestation and bounded no-retry HTTPS loopback/failure gates green; a live credentialed run remains C7 evidence. |
| C7 | Measured context compression | draft | missing | missing | missing | missing | gated | Publish and review a C7-A baseline before accepting thresholds or algorithms. |
| L0 | Durable Ledger vocabulary and state | accepted | accepted | documented | implemented | verified | done | Keep exact C6 payloads, lifecycle ownership, iteration/abandon transitions, and shared Rust/Kotlin matrices green. |
| L1-R | SQLite Ledger adapter | accepted | accepted | documented | implemented | verified | done | Keep v1→v4/future-schema gates, execution/schedule fenced writes, file restart matrices, and all shared ledger scenarios green. |
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
| H1 | Live durable Host | accepted | accepted | documented | implemented | verified | done | Keep exact `api_version = "v1"`, schema-validated canonical typed continuation, shared-client live E2E, and restart replay evidence green. |
| H2 | Client-safe Host read model | accepted | accepted | documented | implemented | verified | done | Keep independent read bounds, canonical cursor, fixed-prefix concurrent/restart, corrupt lifecycle, HTTP query and Rust/Kotlin/TypeScript fixture gates green. |
| H3 | Public Agent activity projection | accepted | accepted | documented | implemented | verified | done | Keep closed safe-code/transition, receipt, redaction-canary, query-bound, SSE/timeline restart and Rust/Kotlin/TypeScript fixture gates green. |
| R1 | Local Runtime composition | accepted | accepted | documented | implemented | verified | done | Keep explicit configuration, fixed-prefix reconstruction, post-commit queue, real protocol flow, bounded shutdown and process-kill recovery gates green. |
| A-CLI | CLI shell | accepted | accepted | documented | implemented | verified | done | Keep real Runtime H1 completion/failure E2E plus create/reuse, terminal identity, and exit-code coverage green. |
| A-TUI | Resident terminal product | accepted | accepted | documented | implemented | partial | active | Keep the verified resident Runtime/PTY, reducer, editor, render, persistence, H2/H3 navigation and activity flows green while the T0-T7 specification audit and remaining platform/fixture closure continue. |
| A-WEB | Web shell | accepted | accepted | documented | implemented | verified | done | Keep strict injectable HTTP/SSE, all H1 mutations, shared reducer fixture and production TypeScript build green. |
| A-DESKTOP | Tauri/React shell | accepted | accepted | documented | implemented | verified | done | Keep embedded R1, typed IPC, backend-only configured startup and temporary-SQLite/real-protocol loops green. |
| A-DESKTOP-C | Desktop backend system configuration | accepted | accepted | documented | implemented | verified | done | Keep strict document parsing, injected secret/profile registries, OS credential resolution and configured startup gates green. |
| A-DESKTOP-C2 | Secure Desktop configuration onboarding | accepted | accepted | documented | implemented | verified | done | Keep preset-expanded catalogue/state, exact revision/digest planning, sensitive credential commit, all-stage recovery, main-window authority, configured-restart E2E, shared fixture and accessible first-run/reconfigure gates green. |
| A-DESKTOP-WORK | macOS local-first work product | accepted | accepted | partial | partial | partial | active | Complete shared H3 evidence, VoiceOver/real-200%-zoom/M75-M76 localization matrices and Developer-ID/notarized release gates. H1/H2/C2 sessions, exact Activity/approval detail, Workspaces and governed Artifacts are implemented; bounded strict-cursor pagination restores up to 512 Turns/256 Artifacts without silently dropping later work. Native menus route four closed data-free intents through the same React state machine. Strict bounded preference schema v2 migrates exact v1 appearance records and supports live theme/density plus system/English/Simplified Chinese/QA-pseudolocale selection without persisting sensitive state. Stable-key localization is implemented across Setup, navigation, Home, Composer, Search, Agents, Timeline, Inspector, Activity, exact approvals, Workspace picker/recovery/revoke, Artifact preview/export, Settings, Privacy and errors; source audit leaves only product/identity marks untranslated. Focused 1280x720 Chinese and expanded-pseudolocale evidence has no horizontal overflow or console errors. Composer tests prove Enter does not submit during CJK IME composition. An effective 640x400 reflow run covers Home, Approval, Artifact overlay and scrollable Settings without horizontal overflow; it exposed and fixed unnamed collapsed-rail controls. This is useful preflight evidence, not a claim of real macOS WebView 200% zoom. Full M75/M76, native CJK IME, VoiceOver, real 200% zoom and packaged-candidate evidence remain open. Picker focus containment/restoration, semantic Inspector tabs and system accessibility preferences have local evidence. A hardened-runtime Universal ad-hoc DMG passes local audit, while Gatekeeper, stapling and warning-free strip remain external release gates. |
| A-DESKTOP-WA | Governed Workspaces and artifacts | accepted | accepted | documented | partial | partial | active | Complete remaining accessibility/package matrices. Session detach has an idempotent durable receipt and restart-safe projection; global revoke journals a manifest-v2 tombstone and retries Keychain cleanup after restart. Artifact export now journals only bounded opaque pending IDs, removes the exact interrupted temporary when its directory is next explicitly authorized, preserves unrelated files, and never persists or exposes a path. Immutable receipt-bound Artifact projection, digest-verified preview and one-shot no-overwrite export are implemented, and the real SQLite approval-to-file-to-Artifact flow passes. |
| A-DESKTOP-VE | Desktop visual, journey and manual evidence | accepted | accepted | documented | missing | missing | active | Execute the stable M01–M85 full-function capture matrix on the candidate package, close visual defects, emit a digest-bound safe screenshot manifest, and deliver/replay the comprehensive versioned user manual on a clean supported Mac. Deterministic fixtures may prove presentation only, never product behavior. |
| A-MOBILE | KMP/Android/iOS shells | accepted | accepted | documented | implemented | verified | done | Keep KMP real-loopback transport, XCFramework, Swift, Android SDK 36 APK and API 36 Compose instrumentation gates green; remote physical-device connectivity is not claimed. |
| A-MOBILE-R | Native remote-work mobile product | accepted | accepted | documented | implemented | partial | active | KMP controller, secure pairing, native Compose/SwiftUI product, installable builds, Gateway/private APNs/FCM wake paths, complete user guide, and real native loopback walkthrough screenshots/actions are verified locally; close only with physical iOS/Android Gateway-to-Runtime create/reconnect/background/wake/decision/cancel/revoke evidence. |
| A-UX1-A | Shared product controller | accepted | accepted | documented | implemented | verified | done | Keep exact TypeScript/KMP fixture parity, strict fixture readers, Host projection, correlation, mutation, reconnect, activity, preference and native XCFramework gates green. |
| A-UX1 | Product client experience | accepted | accepted | partial | partial | partial | active | UX-A is complete. Close shared Web/native UI plus remaining localization/accessibility evidence; keep the Desktop H3 activity, exact approvals, bounded history, governed Artifact/Workspace, focus and preference evidence green. |
| G0 | Go Gateway | accepted | accepted | documented | implemented | verified | done | Keep TLS-only edge composition, one-time device pairing, route/header admission, SSE passthrough, expiry/revocation and race tests green; durable multi-instance grants are a later slice. |
| B0 | SWE benchmark harness | accepted | accepted | documented | implemented | verified | done | Keep strict official loading, the sole bounded concurrent route, release-once failure matrix, explicit command ports, unified-diff/prediction adapters, pinned official report coverage, JSONL tracking and CLI E2E green. Real Docker publication evidence remains external and gated. |

## Capability backlog

| Slice | Design | Spec | API | Code | Tests | State | Admission dependency |
|---|---|---|---|---|---|---|---|
| Memory | accepted | accepted | documented | implemented | verified | done | Keep shared Rust/Kotlin bounds/revision/capability-admission fixtures, exact L0 payloads, Runtime authority, atomic writes, and SQLite commit-before-C2/restart evidence green. |
| Memory M1 | accepted | accepted | documented | implemented | verified | done | Keep M1-A through M1-H green: shared Rust/Kotlin lifecycle/maintenance/recall/feedback semantics, exact L0 facts, SQLite membership/restart checks, committed C2 admission and pinned quality reductions. |
| Memory M1-H | accepted | accepted | documented | implemented | verified | done | Keep recall-fact-bound obligations, fixed-prefix owner/selection/membership validation, restart forgery refusal and exact content-free Rust/Kotlin feedback ratios green. |
| Memory M2 | accepted | accepted | partial | partial | partial | active | M2-A through M2-C are complete; implement M2-D opaque-capability Desktop review, confirmation and receipt workflow. |
| Memory M2-A/B | accepted | accepted | documented | implemented | verified | done | Keep exact-identity documents, canonical manifest/layout validation, authority-safe ordered plans and shared Rust/Kotlin digests green. |
| Memory M2-C | accepted | accepted | documented | implemented | verified | done | Keep exact grants, namespace projection/journal transactions, canonical receipts, content-scrubbing erasure, fsynced directory export and import/export crash/replay/corruption matrices green. |
| Memory M2-D | accepted | accepted | missing | missing | missing | active | Add Desktop-native opaque file capabilities, bounded review/confirmation IPC and durable receipt recovery after M2-C. |
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
- [`design/agent-product-increment-spec-set.md`](design/agent-product-increment-spec-set.md) — complete active increment contract/fixture coverage.
- [`design/desktop-system-configuration.md`](design/desktop-system-configuration.md) — backend-only Desktop configuration contract.
- [`../.agents/testing.md`](../.agents/testing.md) — evidence levels and repository gates.
- [`AGENTS.md`](AGENTS.md) — Spec admission and schema rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
