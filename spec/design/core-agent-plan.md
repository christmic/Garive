# Core Agent delivery DAG

## Status

Execution plan derived from the accepted Agent architecture/execution and
Rust/Kotlin conformance contracts. A slice cannot start behavior implementation
until its focused spec and acceptance fixtures are accepted.

The accepted D0/C4/C5/C6 implementation set is indexed by
[`agent-core-spec-set.md`](agent-core-spec-set.md). Shared fixtures still land
before each behavior slice.

The post-H1 Memory/Knowledge/Skill/Scheduler/Observability proposal is indexed
by [`agent-capability-spec-set.md`](agent-capability-spec-set.md). It remains a
draft review set and admits no behavior until owner acceptance.

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
| 5 | C5b-R/K | Interaction suspension and typed continuation reduction. | Shared request/resolve/cancel/conflict scenarios. |
| 6 | C5c-R/K | Receipt and uncertainty recovery decisions. | Shared crash-position matrix; no concrete executor claim. |
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
Product clients still use the versioned fake Host until a separate Runtime
composition/credential-provisioning slice replaces it; H1 completion alone
does not authorize clients to construct secrets or execution ports.

## First milestone acceptance

F1-F3 are complete only when one deterministic model-only Turn can suspend and
continue as two Execution IDs under one Turn ID, or complete/stop/fail exactly
once, with Rust and Kotlin consuming the same complete fixture/scenario sets.
