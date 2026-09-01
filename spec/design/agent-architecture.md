# Garive Agent architecture contract

## Status

Accepted foundation for Core Agent development. Mechanism research in `docs/`
may refine implementation choices but may not contradict this ownership model.

## Purpose

Define what an Agent is, what one execution owns, how Runtime hosts it, and how
the capability crates participate without creating a second product Runtime.

## Ubiquitous language

| Term | Meaning | Owner |
|---|---|---|
| Agent Definition | Immutable, revisioned description of instructions, capability references, policies, and default limits. | Engine domain; Runtime registry resolves it. |
| Agent Instance | Product identity bound to one exact Agent Definition revision and effective configuration. | Runtime |
| Session | Durable product conversation/workspace shared by one or more Agent Instances. | Runtime |
| Turn | One durable user/system objective for one Agent Instance. It may span multiple Kernel Executions after suspension. | Runtime |
| Kernel Execution | One bounded invocation of Core with immutable input and frozen ports. Its memory is disposable. | Core |
| Iteration | One derive → model invoke → optional intent handling step inside a Kernel Execution. | Core |
| Durable Fact | Committed evidence used by Runtime to reconstruct a Turn. | Runtime ledger |
| Live Event | Best-effort progress for observers; never the only record of a durable outcome. | Core emits; Runtime maps/publishes |
| Prepared Call | Validated immutable tool request with stable digest and requirements. | Tools/Core |
| Invocation | Runtime-authorized attempt to execute one exact Prepared Call under a stable identity. | Runtime |

These identities are distinct. A model role, provider model, tool call, branch,
or sub-agent is not automatically an Agent Instance.

## Product boundary

```text
Clients / Channels
        |
        v
Runtime host and composition root
  - identity, Session, Turn, persistence, recovery
  - authorization, concrete execution, credentials
  - exact Agent Definition/config/port selection
        |
        v
Core Agent kernel
  - bounded iteration and decisions
  - context projection requests
  - model outcome reduction
  - tool intent preparation and model-visible observations
        |
        +--> neutral Engine capability contracts
        `--> frozen ports implemented by Runtime/adapters
```

Dependencies point from Runtime to Engine. No Engine crate imports Runtime,
client, transport, database, credential, or concrete sandbox types.

## Agent Definition

An Agent Definition contains only portable intent and policy:

- stable definition identity and immutable revision;
- instruction source references and precedence policy;
- enabled capability references: tools, skills, memory, knowledge, delegation;
- model-role requirements expressed as capabilities, not provider credentials;
- context/iteration/budget policy bounds;
- governance requirements and allowed execution requirement classes;
- feature/capability version declarations.

It does not contain authenticated user identity, Session state, secrets,
provider clients, database handles, workspace paths, or running tasks. Runtime
resolves references and freezes an effective execution input before Core runs.

## Engine capability ownership

| Crate | Owns | Does not own |
|---|---|---|
| `core` | Kernel execution protocol, iteration control, outcome reduction, portable events. | Session/recovery, storage, concrete adapters. |
| `llm` | Provider-neutral request/items/stream/outcome/usage/capability values and model port. | HTTP, credentials, product failover, or retry-budget selection. |
| `tools` | Tool definitions, model intents, validation, immutable prepared calls, neutral results. | Authorization facts, sandboxes, process execution. |
| `goal` | Goal definitions, success evidence requirements and pure lifecycle semantics. | Actor identity, persistence, clocks or automatic success. |
| `plan` | Goal-bound Plan topology, digests, readiness and pure transition semantics. | Claims, workers, leases, authorization or scheduling infrastructure. |
| `ledger` | Durable-fact vocabulary and query/append ports required by Engine policies. | SQLite schema, transactions, backup. |
| `memory` | Memory candidates, retrieval intent, ranking/policy contracts. | Persistent store or automatic authority over prompts. |
| `knowledge` | Sources, citations, evidence and retrieval contracts. | Network clients, indexes, product truth. |
| `skill` | Skill descriptors, selection and bounded invocation contracts. | Plugin installation or process hosting. |
| `multiagent` | Delegation intent, child requirement and result semantics. | Child lifecycle, mailbox, scheduling, durable topology. |
| `scheduler` | Portable scheduling intent and policy values. | Clocks, queues, workers. |
| `creativity` | Alternative-generation/exploration policy. | Hidden Agents or unbounded model calls. |
| `eval` | Evaluation request/result/evidence values. | Benchmark environments and score storage. |
| `config` | Validated portable policy values. | Files/env/secrets loading. |
| `observability` | Low-cardinality semantic Agent events/measurements. | Exporters and durable audit truth. |
| `proto` | Generated Rust bindings for admitted wire contracts. | Internal domain ownership. |

Capabilities are explicit inputs. A directory or installed implementation does
not grant an Agent permission to use it.
Conversely, a Worker cannot silently omit snapshot-declared capabilities. Its
capability-preparation port is mandatory even for a no-capability snapshot and
must fail before Core when the exact installation cannot be resolved.

## Kernel Execution contract

Runtime invokes Core with exactly:

```text
AgentTurnRequest + AgentExecutionPorts -> AgentEvent* + AgentOutcome
```

The request is immutable and names the Turn, Agent Definition revision,
execution attempt, trusted input, reconstructed cursor, capability snapshot,
and hard limits. Ports are frozen implementations selected by Runtime. Core
must not discover services or mutate Runtime configuration while executing.

An execution returns once with one `AgentOutcome`:

- `Completed`: final Agent response for the Turn;
- `Suspended`: durable external input/reconciliation is required;
- `Stopped`: a declared hard limit or cancellation ended work without success;
- `Failed`: an invariant or required capability failed.

Suspension ends the current Kernel Execution, not the durable Turn. Runtime
commits the suspension, later reconstructs a new request, and invokes a new
Kernel Execution with the same `turn_id` and a new `execution_id`. No in-memory
`TurnState` is resumed across calls.

## State and recovery

- Core state is a disposable projection used only during one Kernel Execution.
- Runtime durable facts are the sole recovery source.
- A request/call receipt is committed before an uncertain external invocation.
- Missing output does not prove an invocation did not occur.
- External effects follow Prepared → Authorized → Started → Receipt → Result.
- An uncertain `Started` effect is replayed only when its replay class and
  executor contract prove safety; otherwise the Turn suspends for reconciliation.
- Live deltas may be lost. Completed responses, suspensions, stops, failures,
  grants, receipts and model-visible tool observations require durable facts.

## Multi-Agent semantics

Delegation produces a request for Runtime to start or address another Agent
Instance. The child owns its own definition revision, execution IDs, budgets,
ports and outcomes. A child cannot inherit credentials or authority implicitly.
The parent observes only a governed, durable child result. Internal model roles
used for drafting/critique remain model calls, not Agent Instances.

## Hard invariants

1. One component owns each durable fact and authority decision.
2. Core cannot perform concrete external effects without a Runtime port.
3. Runtime cannot rewrite an approved Prepared Call in place.
4. Agent identity, Turn identity, execution identity, model request identity,
   and tool invocation identity are never reused as substitutes for one another.
5. Every Kernel Execution is bounded by non-zero iteration limits and explicit
   cancellation; token/deadline limits are enforced when provided.
6. Unknown usage is not zero; unknown provider capability is not supported.
7. Provider-specific transport/status values do not cross the `llm` boundary.
8. Rust and Kotlin support claims require the conformance level defined for the
   same slice; directory parity is not behavioral parity.

## Non-goals

- prescribing SQLite tables, HTTP APIs, UI DTOs, provider SDKs or sandbox tech;
- requiring every capability in every Agent Definition;
- making protobuf the internal domain model;
- allowing research thresholds to become release gates without evidence.

## Acceptance

Architecture tests and dependency metadata must demonstrate that Engine has no
Runtime/App dependency. Execution, model, and tool specs must use the identities
and outcome ownership above. Any incompatible slice must amend this contract
before implementation rather than adding an exception in code.
