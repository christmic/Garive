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
| 2 | [`loop.md`](loop.md) | One bounded Agent execution; derive, assemble, invoke, prepare, and return. | draft |
| 3 | [`provider-adapter.md`](provider-adapter.md) | Provider-neutral invocation boundary and provider-specific transport recovery. | draft |
| 4 | [`effect-layer.md`](effect-layer.md) | Prepared calls, authorization, execution, receipts, and uncertain-effect recovery. | draft |
| 5 | [`ledger.md`](ledger.md) | Runtime-owned durable facts, projections, audit, and recovery. | draft |
| 6 | [`compression.md`](compression.md) | Context-pressure estimation and compression policy. | research |
| 7 | [`derive-testing.md`](derive-testing.md) | Correctness, property, retention, and equivalence testing for derive. | research |
| 8 | [`assemble-testing.md`](assemble-testing.md) | Provider assembly contract tests. | research |

## Settled boundaries

- Agent executes one immutable `AgentTurnRequest` with frozen
  `AgentExecutionPorts` and returns `AgentOutcome`.
- Runtime owns product Session lifecycle, durable turns, scheduling, storage,
  approvals, concrete execution, and restart recovery.
- Provider adapters own official wire translation and transport retries; Core
  sees provider-neutral outcomes.
- External effects are never blindly replayed after an uncertain crash window.
- Live UI events and durable facts are separate delivery contracts.
- Proto describes admitted wire/persistence boundaries, not every internal
  domain value.

## Provisional mechanisms

The following remain hypotheses until an executable slice produces evidence:

- exact entry-kind catalogs and SQLite schema;
- compression thresholds, EWMA coefficients, and token formulas;
- byte-equality requirements outside canonical wire fixtures;
- cross-language implementation parity;
- performance and retention numeric gates;
- provider-specific error mappings and payload legality tables.

Keep these details in the current documents, mark unresolved choices, and
promote only the selected subset to `spec/` before implementation.

## Known cross-document rules

| Topic | Canonical owner |
|---|---|
| Product/module ownership | [`../system.md`](../system.md) |
| One-execution control flow | [`loop.md`](loop.md) |
| Provider outcome classification | [`provider-adapter.md`](provider-adapter.md) |
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

- [`../../../spec/design/core-agent-plan.md`](../../../spec/design/core-agent-plan.md)
  — slice order and first milestone.
- [`../../../spec/design/core-turn-control.md`](../../../spec/design/core-turn-control.md)
  — C0 typed turn state and transition contract.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
