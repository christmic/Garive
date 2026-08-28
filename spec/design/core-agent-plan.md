# Core Agent delivery DAG

## Status

Execution plan derived from the accepted Agent architecture/execution and
Rust/Kotlin conformance contracts. A slice cannot start behavior implementation
until its focused spec and acceptance fixtures are accepted.

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

## Slice contracts

| Slice | Deliverable | Required evidence |
|---|---|---|
| C0 | Distinct Turn/Execution IDs, reconstructed cursor, bounded active/closed control. | Rust + Kotlin units and shared execution-control fixtures. |
| C1 | Ordered ModelItems, known/unknown usage, four factual outcome envelopes. | Rust + Kotlin units and shared model-outcome fixtures. |
| C2 | Purpose-specific context request/surface, deterministic masking/order/budget, minimal ledger read port. | Shared semantic fixtures, property tests, no SQLite dependency. |
| C3 | Immutable AgentTurnRequest, frozen ports, model-only bounded execution and AgentOutcome. | Rust/Kotlin fake context/model capability scenarios including cancellation and every model envelope. |
| C4 | Exact tool definition resolution, argument validation, immutable Prepared Call/digest/replay class. | Shared semantic fixtures; invalid intent never reaches authorization. |
| C5 | Authorization/interaction/execution reduction and model-visible observations. | approve/deny/replacement/ask-user/uncertain-effect scenarios; no concrete sandbox in Core. |
| C6 | Runtime Turn facts, request/effect receipts, suspension continuation, crash recovery with real storage. | Rust/SQLite process-restart tests and Kotlin/PostgreSQL transaction/recovery tests required by `agent-platform-delivery.md`. |
| C7 | Compression/masking policy selected from measured context pressure. | Quality/cost baseline; thresholds remain proposed until reproducible. |

## Change-set gate

For C0-C5, each shared semantic change merges only with:

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

Exit: C0/C1 support matrix is executable and green in both languages.

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

## Explicitly deferred

- SQLite index/backup/GC catalog beyond C6's required facts;
- compression formulas and numeric SLOs before C3/C6 baseline;
- Gateway extraction and Kotlin copies of Engine modules outside admitted
  C0-C5/L0 semantics;
- production credentials, signing, distribution and deployment for product
  clients.

Provider adapters, the Kotlin PostgreSQL server, and executable client
skeletons are active work in `agent-platform-delivery.md`. Product clients may
use the versioned fake Host boundary before C6 is complete, but cannot claim a
live end-to-end Agent workflow until the durable Host slice passes.

## First milestone acceptance

F1-F3 are complete only when one deterministic model-only Turn can suspend and
continue as two Execution IDs under one Turn ID, or complete/stop/fail exactly
once, with Rust and Kotlin consuming the same complete fixture/scenario sets.
