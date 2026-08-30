# Active Agent product increment Spec set

> This index freezes the complete contract set for the next Garive product
> increment: inspectable Memory, deterministic effects, restart-safe navigation,
> public Agent activity, secure Desktop setup, and coherent client behavior.

## Audience

Maintainers planning or reviewing the next implementation slices across Rust,
Kotlin/KMP, TypeScript, Tauri, Android, and iOS.

## Status and boundary

The Specs indexed here are accepted and implementation-ready. Acceptance means
the boundary, ownership, compatibility, failures, dependencies, and executable
evidence are defined. It does not mean API, code, or tests exist; the live
delivery state remains solely in [`../STATUS.md`](../STATUS.md).

This set refines the existing accepted D0–C6/H1/R1 platform. It does not reopen
Engine authority, copy Runtime storage into clients, make H1 remotely reachable,
or admit deferred Memory automation and effect speculation.

The audit found two prerequisite H1 implementation defects: the Rust emitter
uses a package-qualified API string while clients/fixtures require exact
`api_version = "v1"`, and its string-only continuation path does not yet validate
the durable C5 response schema for non-string values. H1 is therefore `partial`
in `STATUS.md`. The implementation phase must correct both and add a real
live-Host-to-shared-client E2E before any H2/H3 or product completion claim.

## Requirement coverage

| Product requirement | Normative owner | Required outcome |
|---|---|---|
| User can inspect and move Memory safely | [M2](memory-control-plane.md) | Canonical bounded snapshots, pure review plan, explicit authority, atomic durable receipt. |
| Desktop can review Memory files without exposing paths | [M2-D](desktop-memory-control.md) | Native picker, opaque capability, redacted diff, explicit confirmation and recovery. |
| Independent reads may run concurrently without changing truth | [C5b](deterministic-effect-batches.md) | Versioned access declarations, exact conflict graph, bounded plan, ordered durable publication. |
| Client can discover Agents and reopen Sessions | [H2](host-read-model-v1.md) | Bounded fixed-prefix discovery, Session pages and complete Turn timelines. |
| Client can show what the Agent is doing | [H3](host-agent-activity-v1.md) | Redacted committed interaction/effect snapshots and replayable activity events. |
| Desktop can become configured without exposing secrets | [A-DESKTOP-C2](desktop-configuration-onboarding.md) | Write-only staged setup/rotation, OS secret storage and crash recovery. |
| Desktop/Web/mobile share one product behavior | [A-UX1](client-product-experience.md) | Pure controller semantics, native presentation, explicit local/durable ownership and accessibility gates. |
| Mobile controls server-hosted Agents away from a computer | [A-MOBILE-R](mobile-remote-work-client.md) | Authenticated HTTPS, exact durable commands, native Work/conversation/decision UI, background return, and physical-device evidence. |
| Toolchains and SDKs remain current and reproducible | [dependency rule](../../.agents/dependency-versions.md) | Official-source stable selection, explicit compatibility holds, lockfiles and native build evidence. |

Existing [M0](memory-capability.md) and
[M1](memory-hypothesis-lifecycle.md) remain authoritative for Memory classes,
authority, lifecycle, maintenance, recall, and learning. M2 adds a control
plane; it does not create a second Memory model. Existing C4/C5/C6 remain
authoritative for preparation, authorization, interaction, execution,
durability, and recovery. C5b changes only the explicitly versioned Prepared
Call v2 path.

## Dependency DAG

```text
official version audit ───────────────────────────────────────────────┐
                                                                    v
H1 version/typed-continuation repair --------------------------------+

M2-A parser/projection -> M2-B planner -> M2-C durable control -> M2-D
             `---------- shared Rust/Kotlin fixtures ---------'       |
                                                                    v
H2 wire -----------------> H2 Runtime read model ---------+
D0 H3 catalogue -> H3 wire -> H3 Runtime projection -----+-> UX-A controller
                                                               |          |
                                                               v          v
                                                         UX-B Desktop   UX-C
A-DESKTOP-C -> A-DESKTOP-C2 backend/setup ---------------------^

C5b declaration/digest -> shared planner -> Runtime dispatcher/evidence

M1-G committed recall -> M1-H attributable application/outcome chain
```

H2 and H3 wire changes coordinate in one additive Host v1 tag review. H3
timeline snapshots depend on H2; its event projection otherwise builds on H1.
UX-A begins only after H2/H3 wire fixtures are accepted. UX-B requires their
Runtime projections and C2 setup. M2-D requires M2-C and the Desktop controller.
UX-C may prove controller/native presentation with an injected contract
transport, but physical-device live connectivity remains gated on an
authenticated Gateway or separately admitted on-device Runtime.

M2, C5b, and the Host/client chain may progress independently after their own
fixture schema lands. No client mock proves a Runtime contract and no
cross-language planner fixture proves filesystem, database, network, executor,
or native-UI enforcement.

## Implementation packages

| Order | Package | Deliverable | Completion evidence |
|---:|---|---|---|
| 1 | V1 | Audit official stable versions and compatible holds. | Lockfile/wrapper diff, source links, all affected native build gates. |
| 2 | H1-F | Fix exact API version and typed schema-validated continuation. | Existing fixtures plus real Runtime-Host/client E2E and restart replay. |
| 3 | M2-A/B | Canonical snapshot parser/projector and pure import planner. | Complete Rust/Kotlin semantic and canonical vectors. |
| 4 | C5b-A | Prepared v2 declaration/resolver/conflict planner. | Rust/Kotlin graph/plan vectors and sequential differential properties. |
| 5 | M1-H | Recall-fact-bound application/outcome chain and exact quality reduction. | Rust/Kotlin chain fixture, fixed-prefix membership, restart corruption and content-free integer evidence. |
| 6 | H2/H3-W | D0 public activity catalogue, additive Proto and generated consumer mappings. | Snapshot-digest fixture, tag audit, Rust/KMP/TypeScript presence and unknown-value round trips. |
| 7 | M2-C | Runtime file capability and SQLite control transaction. | Symlink/bound tests plus crash/replay matrix. |
| 8 | C5b-R | Bounded read dispatcher and ordered durable publication. | Completion-permutation, timeout/cancel/restart and confined-executor tests. |
| 9 | H2/H3-R | Fixed-prefix read/activity projection and SSE extension. | SQLite restart/concurrency/corruption/redaction matrices. |
| 10 | A-DESKTOP-C2 | Staged setup/rotation backend and first-run UI. | Credential-store, crash recovery, redaction and configured restart E2E. |
| 11 | UX-A | Pure application controller and persistence adapter. | Shared scenarios across TypeScript and KMP. |
| 12 | UX-B | Desktop reference product. | Embedded Runtime restart E2E and accessibility scenarios. |
| 13 | M2-D | Desktop Memory file capability, review and control workflow. | Product E2E over M2-C and the Desktop controller. |
| 14 | UX-C | Web and native Android/iOS presentation. | Same-host Web E2E, controller fixtures, API 37/iOS builds and UI scenarios. |

An order entry is a dependency-safe default, not permission to combine unrelated
changes. Repository small-batch and status-evidence rules still apply.

## Language and ownership matrix

| Contract | Rust | Kotlin/KMP | TypeScript/native UI |
|---|---|---|---|
| M2 snapshot and plan | Canonical parser, projector, planner | Independent parser/planner semantics | No parser; consumes redacted M2-D views. |
| M2 durable control | Runtime filesystem/SQLite owner | No product adapter | Desktop typed IPC and React workflow. |
| C5b plan | Canonical resolver/planner | Independent semantic planner | No scheduling authority. |
| C5b execution | Runtime/executor owner | No parity claim | Public state only through H3. |
| H2/H3 wire | Host projection and Proto binding | Generated binding plus controller mapping | Strict generated/mapped values; native UI renders shared state. |
| A-DESKTOP-C2 | Tauri backend, filesystem and secret store | Not applicable | Write-only setup form; never reads secret/config document. |
| A-UX1 | Host/runtime E2E support | Shared controller semantics | TypeScript controller plus React/Compose/SwiftUI presentation. |

“Shared” means the exact conformance level named by the owning Spec. It never
requires line-for-line implementation parity or a Kotlin copy of Runtime-owned
I/O and storage.

## Fixture catalogue

| Fixture | Required case families | Consumers |
|---|---|---|
| `spec/fixtures/host/live-host-v1.json` and `live-host-client-v1.json` | exact API version, string/JSON continuation, replay/conflict, real E2E | Rust Host/client, CLI/TUI, KMP |
| `spec/fixtures/agent/memory-control-plane-v1.json` | snapshot, parser, plan, authority, bound, digest | Rust/Kotlin M2-A/B |
| `spec/fixtures/agent/deterministic-effect-batches-v1.json` | declaration, normalization, conflict, plan, failure | Rust/Kotlin C5b-A |
| `spec/fixtures/host/host-read-model-v1.json` | definitions, Session pages/views, timeline, cursor, failure | Rust/KMP/TypeScript H2 |
| `spec/fixtures/host/host-agent-activity-v1.json` | projection, timeline, reducer, bound, redaction | Rust/KMP/TypeScript H3/A-UX1 |
| `spec/fixtures/host/client-product-experience-v1.json` | bootstrap, navigation, conversation, command, reconnect, failure | TypeScript/KMP controllers |
| `spec/fixtures/host/desktop-setup-v1.json` | catalogue, plan, commit/recovery, error, redaction | Rust/TypeScript Desktop C2 |
| `spec/fixtures/host/desktop-memory-control-v1.json` | capability, export, review, confirmation, receipt, recovery | Rust/TypeScript M2-D |

Each root object declares `schema_version = 1`. Readers reject unknown root/case
fields, duplicate case names, omitted expected results, and cases not consumed
by every claimed language. Canonical-byte expectations use lowercase SHA-256
and the exact RFC 8785 rules stated by the owning Spec; semantic-only fixtures
must not accidentally freeze incidental serializer bytes.

## Cross-contract invariants

1. Runtime/Ledger remain durable truth; controller caches and preferences never
   manufacture Session, Turn, suspension, Memory, effect, or activity state.
2. Every external mutation has an explicit authority check, stable idempotency
   identity, bounded input, commit boundary, and restart classification.
3. H2/H3 expose only public projections. Raw facts, Engine values, content
   bindings, configuration, credentials, paths, receipts, and evidence stay
   behind Runtime/Desktop backend boundaries.
4. Desktop frontend receives typed product views and opaque capabilities only.
   Its two admitted write channels are C2 setup commit and ordinary Host/M2-D
   commands; neither can read a secret or configuration document.
5. H1 is explicitly loopback-only. Mobile UI/controller completion is not a
   claim that a physical device can remotely reach a Desktop Host.
6. Unknown wire fields/values follow the owning Spec's preservation/fail-closed
   rule. Unknown durable schemas never become guessed public state.
7. Version updates use official sources and preserve reproducible locks. A
   compatibility hold names the blocker and review condition; “latest” is not
   resolved independently at runtime.

## Spec completion gate

This Spec set is complete only when review proves:

- every requirement row resolves to exactly one normative owner and all linked
  Specs are `accepted` with matching `STATUS.md` rows;
- public types, canonical forms, ordering, bounds, authority, stable failures,
  compatibility, recovery, and fixture roots are explicit;
- dependency edges and language claims agree across this index, the delivery
  DAG, client/platform Specs, and tier `AGENTS.md` files;
- Markdown links resolve, IDs are unique, terminology has no conflicting owner,
  and no active contract contains an unresolved placeholder;
- implementation status remains truthful: missing API/code/tests stay missing
  until executable evidence lands.

## See also

- [`core-agent-plan.md`](core-agent-plan.md) — repository-wide dependency order.
- [`agent-core-spec-set.md`](agent-core-spec-set.md) — implemented D0/C4/C5/C6 baseline.
- [`agent-capability-spec-set.md`](agent-capability-spec-set.md) — earlier capability set.
- [`../../.agents/testing.md`](../../.agents/testing.md) — evidence levels and gates.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
