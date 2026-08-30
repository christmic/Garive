# Core Agent design index

> Working documents for Garive's bounded Agent execution and the Runtime
> services immediately around it. Read the system ownership rules first; these
> documents refine mechanisms without changing those owners.

## Audience

The project owner and engineers discussing the first Agent/Runtime vertical
slice.

## Why

The documents in this directory were developed through design discussion and
remain the active workspace. They contain useful detail, but not every detail
has the same maturity. This index distinguishes settled boundaries from
provisional mechanisms so a rough idea is not mistaken for a contract.

## Reading order

| Order | Document | Scope | Status |
|---:|---|---|---|
| 1 | [`../system.md`](../system.md) | Product ownership and dependency direction. | accepted |
| 2 | [`loop.md`](loop.md) | One bounded Agent execution; derive, assemble, invoke, prepare, and return. | mixed: C0–C6 accepted; later mechanisms research |
| 3 | [`provider-adapter.md`](provider-adapter.md) | Protocol adapter, Provider, and Runtime ownership boundary. | accepted |
| 4 | [`effect-layer.md`](effect-layer.md) | Prepared calls, authorization, execution, receipts, and uncertain-effect recovery. | mixed: C4–C6 and C5b-A delivered; C5b-R accepted; later mechanisms research |
| 5 | [`ledger.md`](ledger.md) | Runtime-owned durable facts, projections, audit, and recovery. | mixed: L0/L1 accepted; later variants research |
| 6 | [`compression.md`](compression.md) | Context-pressure estimation and compression policy. | research |
| 7 | [`derive-testing.md`](derive-testing.md) | Correctness, property, retention, and equivalence testing for derive. | mixed: admitted gates implemented; numeric research remains |
| 8 | [`assemble-testing.md`](assemble-testing.md) | Provider assembly contract tests. | mixed: admitted gates implemented; broader research remains |
| 9 | [`memory.md`](memory.md) | Memory classification, lifecycle, distillation and retention research. | mixed: M0/M1 delivered; M2 accepted; quality/graph research |
| 10 | [`agent-foundation-capabilities.md`](agent-foundation-capabilities.md) | Sandbox/Safety, durable Goal/Plan, built-in tools and native-control boundary. | accepted |

## Settled boundaries

- Agent executes one immutable `AgentTurnRequest` with frozen
  `AgentExecutionPorts` and returns `AgentOutcome`.
- Runtime owns product Session lifecycle, durable turns, scheduling, storage,
  approvals, concrete execution, and restart recovery.
- Protocol adapters own official wire types and codecs only. Providers map the
  neutral contract; Runtime owns transport attempts and recovery.
- External effects are never blindly replayed after an uncertain crash window.
- Live UI events and durable facts are separate delivery contracts.
- Proto describes admitted wire/persistence boundaries, not every internal
  domain value.

## Delivered subsets and remaining research

The accepted Specs and `spec/STATUS.md` now prove C0–C6, L0/L1, portable
protocol/Provider mapping and the admitted capability slices. In particular,
exact durable fact catalogs, SQLite schemas, shared Rust/Kotlin semantics and
typed provider error mappings are no longer provisional for those slices.

The following still remain hypotheses until their named admission evidence
exists:

- compression thresholds, EWMA coefficients, and token formulas;
- production Creativity policy and numeric trade-off thresholds;
- Memory mechanisms outside accepted M0/M1/M2 contracts;
- effect streaming, speculative dispatch, mutating concurrency, cache, and
  workspace snapshots outside accepted C5b;
- performance and retention numeric gates;
- each hosted provider extension beyond independently accepted capabilities;
- byte-equality requirements outside canonical wire fixtures.

Keep these details in the current documents, mark unresolved choices, and
promote only the selected subset to `spec/` before implementation.

## Known cross-document rules

| Topic | Canonical owner |
|---|---|
| Product/module ownership | [`../system.md`](../system.md) |
| One-execution control flow | [`loop.md`](loop.md) |
| Protocol/Provider/Runtime boundary | [`provider-adapter.md`](provider-adapter.md) |
| Tool effect lifecycle | [`effect-layer.md`](effect-layer.md) |
| Durable records and projections | [`ledger.md`](ledger.md) |
| Compression policy | [`compression.md`](compression.md) |
| Test categories | [`.agents/testing.md`](../../../.agents/testing.md) |

When two documents disagree, fix the non-owner document to reference the
canonical owner rather than defining the fact twice.

## Promotion to spec

A mechanism is ready for `spec/` when it has:

1. one owner and dependency direction;
2. explicit inputs, outputs, invariants, and failure terminals;
3. no unresolved crash or authority boundary;
4. acceptance examples that do not depend on an implementation;
5. a runnable verification plan.

## Implementation specs

- [`../../../spec/design/agent-core-spec-set.md`](../../../spec/design/agent-core-spec-set.md)
  — accepted review index for every remaining first-vertical Core/Runtime Spec.
- [`../../../spec/design/agent-definition-snapshot.md`](../../../spec/design/agent-definition-snapshot.md)
  — accepted exact definition resolution and frozen snapshot contract.
- [`../../../spec/design/prepared-tool-call.md`](../../../spec/design/prepared-tool-call.md)
  — accepted C4 validation, normalization and digest contract.
- [`../../../spec/design/governed-effects.md`](../../../spec/design/governed-effects.md)
  — accepted C5 authorization, interaction, receipt and observation contract.
- [`../../../spec/design/deterministic-effect-batches.md`](../../../spec/design/deterministic-effect-batches.md)
  — accepted C5b access and bounded parallel read-only batch contract.
- [`../../../spec/design/durable-runtime-turn.md`](../../../spec/design/durable-runtime-turn.md)
  — accepted C6 transaction, continuation and restart contract.
- [`../../../spec/design/durable-runtime-facts.md`](../../../spec/design/durable-runtime-facts.md)
  — accepted exact C6 durable fact payload profiles.
- [`../../../spec/design/agent-architecture.md`](../../../spec/design/agent-architecture.md)
  — normative Agent/product ownership and capability composition.
- [`../../../spec/design/agent-execution-contract.md`](../../../spec/design/agent-execution-contract.md)
  — Kernel request, ports, events, outcomes, model and effect semantics.
- [`../../../spec/design/cross-language-agent-contract.md`](../../../spec/design/cross-language-agent-contract.md)
  — Rust/Kotlin support matrix and executable conformance rule.
- [`../../../spec/design/core-agent-plan.md`](../../../spec/design/core-agent-plan.md)
  — dependency DAG, work packages and milestone gates.
- [`../../../spec/design/core-turn-control.md`](../../../spec/design/core-turn-control.md)
  — C0 typed turn state and transition contract.
- [`../../../spec/design/model-invoke-outcome.md`](../../../spec/design/model-invoke-outcome.md)
  — C1a provider-neutral invocation facts.
- [`../../../spec/design/memory-control-plane.md`](../../../spec/design/memory-control-plane.md)
  — accepted M2 auditable snapshot and import contract.
- [`../../../spec/design/host-read-model-v1.md`](../../../spec/design/host-read-model-v1.md)
  — accepted client-safe navigation and timeline read model.
- [`../../../spec/design/client-product-experience.md`](../../../spec/design/client-product-experience.md)
  — accepted product client state, interaction, UI, and accessibility contract.
- [`../../../spec/design/agent-foundation-capability-spec-set.md`](../../../spec/design/agent-foundation-capability-spec-set.md)
  — accepted Sandbox/Safety, Goal, Plan and tool delivery/evidence order.
- [`../../../spec/design/native-browser-computer-use.md`](../../../spec/design/native-browser-computer-use.md)
  — accepted browser-native and operating-system-native interaction contract.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
