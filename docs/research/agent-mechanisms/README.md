# Initial Agent mechanism research

> Historical design exploration from Garive's first architecture pass. These
> documents preserve useful mechanisms and discarded assumptions; they do not
> define current ownership, APIs, schemas, or implementation requirements.

## Audience

Engineers evaluating a specific mechanism after reading the accepted
architecture under [`docs/architecture/`](../../architecture/).

## Why this exists

The first Garive pass explored the loop, ledger, provider recovery, tool
effects, compression, and their test strategies in detail before the product
module boundaries were stable. That produced useful ideas, but also coupled
Agent reasoning to product Session recovery, concrete persistence, provider
HTTP semantics, and speculative performance thresholds.

The documents remain available as research evidence. A mechanism may return
to the active design only after its owner, failure boundary, and acceptance
evidence are defined in the current architecture and then promoted to
`spec/`.

## Documents

| Document | Retained value | Do not inherit without review |
|---|---|---|
| [`loop.md`](loop.md) | derive/assemble separation; bounded iteration | product Session ownership; pause and recovery state |
| [`ledger.md`](ledger.md) | durable fact vocabulary; audit motivation | one universal ledger; replay of uncertain effects |
| [`provider-adapter.md`](provider-adapter.md) | provider-neutral boundary goal | universal HTTP mappings; outcome-count assumptions |
| [`effect-layer.md`](effect-layer.md) | prepare, authorize, execute separation | execution without durable effect receipts |
| [`compression.md`](compression.md) | context-pressure feedback ideas | unmeasured constants and provider-usage normalization |
| [`derive-testing.md`](derive-testing.md) | golden/property/retention test categories | constitutional numeric gates without a baseline |
| [`assemble-testing.md`](assemble-testing.md) | provider-dialect contract testing | unsourced provider rules and byte-equality everywhere |

## Status

Nothing in this directory is normative. Contradictions inside or between these
documents are historical evidence, not decisions to reconcile in place.

## See also

- [`../../architecture/README.md`](../../architecture/README.md) — accepted
  architecture index.
- [`../../../spec/README.md`](../../../spec/README.md) — promotion boundary for
  implementation-ready contracts.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: superseded
