# Architecture index

> Accepted product boundaries for engineers shaping Garive from a clean start.
> Read this index before adding a source directory, crate, service, protocol,
> or implementation-ready specification.

## Audience

Contributors making module, dependency, persistence, execution, or product
surface decisions.

## Why

Garive is an upgrade of the ideas explored in Sylvander, not a directory-level
copy of it. The initial Garive scaffold pre-created future languages, clients,
services, and engine buckets before their ownership was settled. That made
research look like committed architecture and repeated boundaries that
Sylvander later had to untangle.

The accepted architecture starts with responsibility ownership. The planned
module/App skeleton may land early, but active behavior and release claims only
land with executable slices.

## Indexed documents

| Document | Maturity |
|---|---|
| [`system.md`](system.md) | Product layers, dependency direction, ownership, and source layout. |
| [`core/loop.md`](core/loop.md) | Accepted C0–C6 ownership plus unadmitted mechanism research. |
| [`core/ledger.md`](core/ledger.md) | Accepted L0/L1 ownership plus unadmitted storage/research variants. |
| [`core/provider-adapter.md`](core/provider-adapter.md) | Provider-neutral model boundary and provider recovery. |
| [`core/effect-layer.md`](core/effect-layer.md) | Accepted C4–C6 ownership plus unadmitted mechanism research. |
| [`core/agent-foundation-capabilities.md`](core/agent-foundation-capabilities.md) | Accepted Sandbox/Safety, Goal, Plan and built-in/native tool architecture. |
| [`core/compression.md`](core/compression.md) | Context-pressure and compression policy research. |
| [`core/derive-testing.md`](core/derive-testing.md) | Derive correctness and quality test design. |
| [`core/assemble-testing.md`](core/assemble-testing.md) | Provider assembly contract test design. |

## Status vocabulary

| Status | Meaning |
|---|---|
| `draft` | An option under review; implementation must not depend on it. |
| `accepted` | The current architecture direction; implementation should make it true. |
| `mixed` | Settled ownership/subsets coexist with clearly marked unadmitted research; linked Specs are normative. |
| `research` | A hypothesis or evidence plan that cannot authorize behavior. |
| `superseded` | Historical context only; a newer document owns the decision. |
| `deprecated` | Still present during removal; no new dependency may be added. |

Accepted architecture is still not a wire or storage contract. A slice becomes
normative only after its invariants and acceptance examples land in `spec/`.

The `core/` documents are active discussion documents. Their settled ownership
rules come from `system.md`; promoted mechanism subsets are owned by linked
accepted Specs. Remaining research text cannot override those Specs or create
an implementation claim.

## Adding a source directory

A new source directory must have all four:

1. A named owner in [`system.md`](system.md).
2. A dependency direction that does not introduce a cycle.
3. An implementation-ready contract in `spec/` when it crosses a process,
   persistence, or public API boundary.
4. A truthful status; buildable tiers also need a runnable verification command.

Planned App/service placeholders are retained as target boundaries. Duplicate
trees, generated drift, and placeholders described as shipping are not.

## See also

- [`../README.md`](../README.md) — documentation lifecycle.
- [`../../spec/README.md`](../../spec/README.md) — normative promotion gate.
- [`../../.agents/architecture.md`](../../.agents/architecture.md) — concise
  repository-wide dependency rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
