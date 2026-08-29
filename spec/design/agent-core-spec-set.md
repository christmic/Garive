# Agent Core implementation Spec set

> Review map for the project owner and Core/Runtime implementers. It fixes the
> complete D0/C4/C5/C6 contract boundary that must be accepted before the next
> Agent implementation slice begins.

## Audience

The project owner reviewing the next milestone and engineers who will implement
or verify Rust Core, Kotlin conformance, Runtime, and durable recovery.

## Why

C0-C3 and L0 are executable, but the remaining design was spread across draft
mechanism documents. Without one review set, authority, invocation identity,
durability and restart decisions could be selected piecemeal in code.

## Status

Accepted review index. The project owner accepted the complete set on
2026-08-29; coordinated contract amendments and fixtures still precede
behavior implementation.

## Scope

This set completes the contracts needed to move the existing model-only kernel
to one governed, restart-safe Agent Turn. It does not pull protocol codecs,
provider deployment, concrete sandboxes, public Host transport, or adaptive
compression into Core.

## Normative order

| Order | ID | Contract | Owner | Depends on |
|---:|---|---|---|---|
| 1 | C4 | [`prepared-tool-call.md`](prepared-tool-call.md) | Tools/Core | C1b |
| 2 | D0 | [`agent-definition-snapshot.md`](agent-definition-snapshot.md) | Engine definition values; Runtime resolution | Agent architecture, C4 tool definition |
| 3 | C5 | [`governed-effects.md`](governed-effects.md) | Core semantics; Runtime authority and effects | C4, L0 |
| 4 | C6 | [`durable-runtime-turn.md`](durable-runtime-turn.md) + [`durable-runtime-facts.md`](durable-runtime-facts.md) | Runtime | C0-C5, L0/L1 |

Existing accepted contracts remain normative for C0-C3 and L0. Where a
proposed document conflicts with an accepted contract, the accepted contract
wins and the proposal must be corrected before acceptance.

## Boundary map

```text
Agent Definition --Runtime resolves--> Effective Agent Snapshot
                                         |
model ToolIntent --Core C4-------------> Prepared Tool Call
                                         |
Runtime C5: invocation identity -> authorization -> execution/recovery
                                         |
Core C5 <--------------------------- governed observation/suspension
                                         |
Runtime C6: durable facts, restart reconstruction, terminal publication
```

The map fixes these ownership decisions:

- a Prepared Call has no `ToolInvocationId` and carries no authority;
- Runtime allocates invocation and interaction identities;
- authorization binds one exact Prepared Call digest and cannot rewrite it;
- Core reduces typed facts but never opens storage or a concrete executor;
- Runtime commits recovery evidence before publishing a durable outcome.

## Required conformance artifacts after acceptance

| Contract | Shared evidence | Native evidence |
|---|---|---|
| D0 | definition/snapshot semantic and digest fixture | Rust and Kotlin validation/property tests |
| C4 | preparation, validation, canonicalization and digest fixture | Rust and Kotlin native tests |
| C5 | authorization/effect/observation state scenarios | Rust and Kotlin reducer tests; Rust Runtime fake |
| C6 | restart and continuation scenarios | Rust/SQLite process tests; Kotlin/PostgreSQL experiment tests |

Fixtures are versioned inputs, not public wire DTOs. Canonical byte equality is
required only for the D0/C4 digest preimages explicitly named by those Specs.

## External dependencies, not Core contracts

- `P2-C` maps neutral model requests/outcomes to one compatible deployment;
- `P2-V0` formats Runtime-supplied endpoint/credential values and exact vendor
  error policy without loading or owning those values;
- `P2-VX` admits special vendor capabilities one semantic extension at a time;
- `H1-T` executes explicit Runtime-owned model HTTP attempts, while `H1`
  exposes committed Runtime commands/events/status to clients;
- concrete executors prove filesystem/process/network enforcement;
- `C7` compression remains gated until a measured C3/C6 workload baseline
  exists. [`context-pressure-baseline.md`](context-pressure-baseline.md) is the
  accepted C7-A evidence contract; implementing it does not admit compression.

These dependencies need focused Specs before their own implementation, but
their absence does not justify provider, transport, database, or sandbox types
inside C4/C5.

## Acceptance of this set

The set may be accepted only when:

1. every public value has an owner, invariants and stable failure classes;
2. every state transition names its durable boundary and crash decision;
3. Rust/Kotlin responsibilities are explicit without claiming nonexistent
   conformance;
4. no draft mechanism from `docs/` silently becomes a numeric release policy;
5. the project owner explicitly approves the documents.

Acceptance also coordinates changes to existing contracts: L0 gains the
Runtime-only `execution.abandoned`, `effect.observation`,
`tool.preparation_rejected`, and `turn.cancel_requested` facts defined by C6,
and the Rust/Kotlin conformance matrix admits D0/C4/C5 as experimental target
slices. The execution identity catalogue also gains the Runtime-owned command,
suspension, interaction, grant, receipt and dispatch-attempt identities used by
C5/C6. None may be claimed implemented until its fixtures and native tests
land.

## See also

- [`agent-architecture.md`](agent-architecture.md) — accepted ownership model.
- [`core-agent-plan.md`](core-agent-plan.md) — delivery DAG and work packages.
- [`../STATUS.md`](../STATUS.md) — implementation and evidence status.
- [`../AGENTS.md`](../AGENTS.md) — Spec admission and schema rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
