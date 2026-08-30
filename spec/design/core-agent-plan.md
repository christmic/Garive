# Core Agent delivery DAG

## Status

Execution plan derived from the accepted Agent architecture/execution and
Rust/Kotlin conformance contracts. A slice cannot start behavior implementation
until its focused spec and acceptance fixtures are accepted.

The accepted D0/C4/C5/C6 implementation set is indexed by
[`agent-core-spec-set.md`](agent-core-spec-set.md). Shared fixtures still land
before each behavior slice.

The accepted post-H1 capability set is indexed by
[`agent-capability-spec-set.md`](agent-capability-spec-set.md). Each behavior
slice still begins with complete shared fixtures and coordinated fact changes.

## Milestones

```text
Foundation
  C0 execution control -----+
  C1 model facts -----------+--> C2 context surface
                                   |
                                   v
                            C3 model-only turn
                                   |
                    +--------------+--------------+
                    v                             v
             C4 tool preparation          capability ports
                    |                memory/knowledge/skill
                    v                             |
             C5 governed effects <---------------+
                    |
                    v
             C6 durable Runtime host
                    |
                    +--> C7 measured compression
                    `--> multi-Agent/delegation slices
```

## Current delivery status

[`../STATUS.md`](../STATUS.md) is the authoritative progress board. This file
owns dependency order and work-package acceptance, not mutable delivery status.

## Slice contracts

| Slice | Deliverable | Required evidence |
|---|---|---|
| C0 | Distinct Turn/Execution IDs, reconstructed cursor, bounded active/closed control. | Rust units plus Kotlin experimental execution-control fixtures. |
| C1 | Ordered ModelItems, known/unknown usage, four factual outcome envelopes. | Rust units plus Kotlin experimental model-outcome fixtures. |
| C2 | Purpose-specific context request/surface, deterministic masking/order/budget, minimal ledger read port. | Shared semantic fixtures, property tests, no SQLite dependency. |
| C3 | Immutable AgentTurnRequest, frozen ports, model-only bounded execution and AgentOutcome. | Rust scenarios plus the admitted Kotlin experiment, including cancellation and every model envelope. |
| C4 | Exact tool definition resolution, argument validation, immutable Prepared Call/digest/replay class. | Shared semantic fixtures; invalid intent never reaches authorization. |
| C5 | Authorization/interaction/execution reduction and model-visible observations. | approve/deny/replacement/ask-user/uncertain-effect scenarios; no concrete sandbox in Core. |
| C6 | Runtime Turn facts, request/effect receipts, suspension continuation, crash recovery with real storage. | Rust/SQLite process-restart tests and Kotlin/PostgreSQL transaction/recovery tests required by `agent-platform-delivery.md`. |
| C7 | Compression/masking policy selected from measured context pressure. | Quality/cost baseline; thresholds remain proposed until reproducible. |

## Change-set gate

For C0-C3 changes that retain an experimental cross-language conformance claim:

1. focused spec update;
2. shared fixture update;
3. Rust implementation and native tests;
4. Kotlin implementation and native tests;
5. root `just conformance` consuming all fixture cases in both languages;
6. architecture/dependency and strict native build checks.

C6 storage adapters are Runtime-owned and language-native. SQL/driver parity is
not implied; portable L0 semantics and public continuation/outcome behavior are
shared, while SQLite and PostgreSQL each require native integration evidence.

## Work packages

### F1 — repair the foundation

- replace C0 cross-call resume state with disposable ExecutionControl;
- replace C1 text-only/action-prescribing outcome with ordered items/fact
  envelopes;
- add Kotlin `core` and `llm` modules;
- add shared fixture readers and wire `just conformance`;
- remove placeholder parity claims.

Exit: Rust C0-C3 is executable and the admitted Kotlin experiment is green.

### F2 — context contract

- specify durable fact reference and purpose projections;
- define deterministic surface ordering/masking and budget result;
- admit only the ledger query operations required by derive;
- implement C2 in Rust/Kotlin from shared fixtures.

Exit: identical semantic surfaces for fixtures; canonical bytes only where an
explicit cache/digest contract requires them.

### F3 — first Agent milestone

- specify complete request/ports/events/outcome values;
- implement bounded model-only reducer/driver;
- use fake Runtime context and fake Model ports;
- cover answer, overflow rebuild, partial, rate unavailable, cancellation,
  iteration limit, missing usage, and required capability failure.

Exit: both languages run the same model-only capability scenarios and cannot
exceed limits or confuse suspension with continuation.

### F4 — governed tools

- C4 preparation/digest/replay class;
- C5 authorization, required interaction, effect result reduction;
- Runtime fake proves no invalid/unapproved call executes;
- uncertain `Started` path suspends for operator reconciliation.

### F5 — durable host

- select minimal durable facts from the ledger research document;
- implement real storage transactions and exact cursor reconstruction;
- test every request/effect crash boundary in a separate process;
- expose redacted status and reconciliation action through Runtime host.

## Post-approval implementation slices

The contract set is accepted. Each behavior row lands green and preserves the
dependency direction from the milestone DAG.

| Order | Slice | Behavior boundary | Required evidence |
|---:|---|---|---|
| 1 | S4-contract | Coordinate L0 fact/identity additions and D0/C4/C5 Kotlin admission; add accepted fixture schemas. | Spec status/link checks; complete fixture catalog, no behavior code. |
| 2 | C4-R/K | Tool catalog validation, Portable Tool Schema v1, normalization and digest. | Shared vectors plus independent Rust/Kotlin tests. |
| 3 | D0-R/K | Definition validation, exact resolution and effective snapshot digest. | Shared semantic/canonical fixtures and dependency gates. |
| 4 | C5a-R/K | Authorization verdict/grant/observation reducer with fake ports. | Shared approve/deny/replacement/unsupported scenarios. |
| 5 | C5-interaction-R/K | Interaction suspension and typed continuation reduction. | Shared request/resolve/cancel/conflict scenarios. |
| 6 | C5-recovery-R/K | Receipt and uncertainty recovery decisions. | Shared crash-position matrix; no concrete executor claim. |
| 7 | C6-domain | Runtime command/idempotency/fact mapping and disposable-execution recovery. | Rust domain tests plus admitted Kotlin semantic subset. |
| 8 | C6-Rust | SQLite Runtime composition and process restart matrix. | Real-file process-kill tests at every C6 checkpoint. |
| 9 | C6-Kotlin | PostgreSQL experimental recovery host. | Real PostgreSQL transaction/restart subset, reported separately. |

P2-C Provider mapping, P2-V0 vendor connection profiles, H1-T Runtime HTTP and
H1 durable Host are complete external slices. Concrete executor enforcement
remains independently scoped. Fakes can prove orchestration but cannot be used
to claim a concrete external boundary.

## Draft post-H1 capability order

After owner acceptance, deliver S0 exact Skill activation, M0 governed Memory,
K0 attributed Knowledge, Q0 durable scheduling, MA0 governed delegation and O0
observability using the fixtures/evidence declared by the capability Spec set.
Portable reducers target Rust/Kotlin shared semantics; concrete stores,
connectors, child lifecycle, workers and exporters stay Runtime-owned and
require independent Rust evidence.

## Explicitly deferred

- SQLite index/backup/GC catalog beyond C6's required facts;
- compression formulas and numeric SLOs before C3/C6 baseline;
- Gateway extraction and Kotlin copies of Engine modules outside admitted
  C0-C5/L0 semantics;
- production credentials, signing, distribution and deployment for product
  clients.

Provider adapters, the experimental Kotlin PostgreSQL verification host and
executable client skeletons are governed by `agent-platform-delivery.md`.
H1 completion alone does not authorize clients to construct secrets or
execution ports. The replacement boundary is specified by
[`local-runtime-composition.md`](local-runtime-composition.md) and
[`live-host-clients.md`](live-host-clients.md); implementation must preserve
their explicit configuration, reconstruction and retry rules.

## Next accepted increments

```text
dependency/toolchain audit ───────────────────────────────┐
                                                         v
H1 version/typed-continuation repair ---------------------+

M2-A projection/parser -> M2-B planner -> M2-C SQLite -> M2-D Desktop flow
                              |                              ^
                              v                              |
                         shared Rust/Kotlin                  |
                                                             |
H2 proto ----------------> H2 Runtime projection ---------+
D0 H3 catalogue -> H3 proto -> H3 Runtime projection -----+-> UX-A
                                                               |    |
                                                               v    v
                                                        UX-B Desktop UX-C
A-DESKTOP-C -> A-DESKTOP-C2 -------------------------------^

C5b declarations -> shared planner -> Runtime read batches -> executor evidence
```

| Order | Package | Boundary | Exit evidence |
|---:|---|---|---|
| 1 | V1 | Review official stable toolchain/SDK/dependency sources; update owners and lockfiles under the dependency rule. | Native builds prove the selected compatible stable set; every hold is documented. |
| 2 | H1-F | Exact API-version emitter, canonical JSON continuation and real Host/client integration. | Existing Host/client fixtures, schema validation, restart replay and Runtime-backed CLI/TUI E2E. |
| 3 | M2-A/B | Canonical Memory snapshot parser/projection and authority-safe import planner. | Shared Rust/Kotlin fixture and plan digests. |
| 4 | C5b-A | Tool access policy, pure exact resolver contract, Prepared Call digest amendment, conflict planner. | Shared Rust/Kotlin graph/plan fixture plus sequential differential properties. |
| 5 | H2/H3-W | D0 public activity catalogue plus additive Host v1 read-model/activity messages and client mappings. | Snapshot digest, Proto tag audit, and Rust/KMP/TypeScript presence/unknown-value round trips. |
| 6 | M2-C | Runtime filesystem capability and atomic SQLite Memory import receipts. | Real-file bounds/symlink tests and crash/replay matrix. |
| 7 | C5b-R | Bounded parallel read-only dispatcher with timeout/cancel/recovery ordering. | Completion-permutation properties and real confined executor tests. |
| 8 | H2/H3-R | Installed-Agent, Session/timeline and redacted activity projections/events. | File-backed SQLite restart/concurrency/corruption/redaction matrices. |
| 9 | A-DESKTOP-C2 | Staged backend setup/rotation and first-run UI. | OS credential-store, crash recovery, redaction and configured restart E2E. |
| 10 | UX-A | Shared pure application controller. | Complete TypeScript/KMP controller scenarios over H2/H3 fixtures. |
| 11 | UX-B | Desktop reference product UI. | Configured embedded-Runtime restart E2E plus accessibility gates. |
| 12 | M2-D | Desktop Memory export, edit handoff, dry-run diff, confirmation, import/erasure receipt. | Product E2E over M2-C and the A-UX1 controller boundary. |
| 13 | UX-C | Web, KMP, Android API 37 Compose, and iOS native presentation. | Same-host Web E2E, controller conformance, native builds and platform UI scenarios. |

M2, C5b, and the H2/H3 Host chain may progress independently after their own
fixtures are accepted. UX-A requires coordinated H2/H3 wire fixtures; UX-B
requires their Runtime projections and A-DESKTOP-C2. M2-D requires both M2-C
and the Desktop controller. No package may use a later UI mock as evidence for
an earlier Runtime boundary.

The complete requirement, language, fixture, and dependency coverage for these
packages is frozen by
[`agent-product-increment-spec-set.md`](agent-product-increment-spec-set.md).

## First milestone acceptance

F1-F3 are complete only when one deterministic model-only Turn can suspend and
continue as two Execution IDs under one Turn ID, or complete/stop/fail exactly
once, with Rust and Kotlin consuming the same complete fixture/scenario sets.
