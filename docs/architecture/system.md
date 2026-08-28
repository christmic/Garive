# Product module architecture

> Defines Garive's product layers and ownership before implementation begins.
> The first implementation is Rust-first and deliberately small; additional
> languages, gateways, and apps must be earned by a concrete product slice.

## Audience

Engineers deciding where a capability belongs or whether a new crate, service,
adapter, or application is justified.

## Why

An Agent product has three different kinds of state:

- model-facing reasoning state for one bounded execution;
- durable product state that survives process loss and user absence;
- public state exposed to clients and operators.

Combining them produces hidden authority, unsafe recovery, and UI or protocol
types leaking into the reasoning kernel. Garive assigns each kind to one owner.

## Layer map

```text
Clients and operator surfaces
           |
           v
      Public API DTOs
           |
           v
Channels -> Runtime composition root -> provider/infrastructure adapters
                |
                v
          Agent kernel ports
                |
                v
       Provider-neutral model contract
```

Dependencies point downward. Runtime is the only production layer allowed to
join authenticated identity, durable Session state, an Agent execution,
credentials, authorization, and concrete infrastructure.

## Ownership

| Layer | Owns | Must not own |
|---|---|---|
| Model contract | Provider-neutral request, response, stream, tool-schema, capability, usage, and error values. | HTTP clients, credentials, Sessions, tools, or UI events. |
| Provider adapter | One verified provider wire protocol and client implementation. | Agent policy, product recovery, or public API DTOs. |
| Agent kernel | One bounded reasoning execution, context shaping, model/tool iteration, prepared tool calls, and internal events/outcome. | Product Sessions, SQLite, credentials, concrete sandboxes, MCP processes, Channels, or client DTOs. |
| Runtime | Session lifecycle, durable turns, exact revisions, scheduling, interruption, approvals, storage, effect recovery, concrete execution, credentials, and observability. | Provider-specific wire shapes or presentation state. |
| Public API | Versioned serializable request, response, event, identifier, and redacted view shapes. | Tokio channels, databases, Agent execution, or provider clients. |
| Channel | Native transport adaptation to a Runtime-owned host port. | Session storage, Agent internals, credentials, or parallel product truth. |
| Client | Ephemeral presentation state and user interaction. | Durable Session truth, tool authority, provider credentials, or recovery decisions. |

## Agent execution boundary

Runtime constructs an immutable input and a frozen set of ports for one Agent
execution:

```text
AgentTurnRequest + AgentExecutionPorts
  -> model/tool iteration
  -> AgentEvent*
  -> AgentOutcome
```

`AgentTurnRequest` contains model-visible conversation and trusted execution
context, not a serialized product Session. `AgentExecutionPorts` contains
Runtime-selected authorization, interaction, model, context, and execution
capabilities. Neither performs service discovery during a turn.

The Agent may prepare a tool call and request execution through a port. Runtime
owns whether, where, and with what durable recovery policy that call executes.

## Runtime boundary

Runtime is the application composition root and owns the product lifecycle:

```text
authenticate + authorize Session
  -> commit user input and running turn
  -> freeze revisions and Agent execution input
  -> run Agent with selected ports
  -> commit terminal outcome
  -> publish redacted client events
```

No successful terminal event is published before its durable transaction
commits. Interrupted, failed, cancelled, and operator-required outcomes are
durable terminals, not reconstructed from process memory.

## Initial source layout

Garive keeps the existing top-level product layout. Empty subdirectories are
not contracts; a module becomes real only when its first executable slice
lands.

| Path | Responsibility |
|---|---|
| `engine/core/` | Bounded Agent kernel. |
| `engine/llm/` | Provider-neutral model contract and admitted provider adapters. |
| `engine/tools/` | Tool definitions, immutable prepared calls, and neutral ports. |
| `runtime/replica/` | Product Runtime, Session lifecycle, storage, execution, recovery, and composition. |
| `runtime/gateway/` | Optional service edge; admitted only when deployment evidence requires it. |
| `cli/` | One-shot client over the Runtime host boundary. |
| `tui/` | Interactive terminal client over the same boundary. |
| `desktop/`, `mobile/` | Product clients; no Agent or Session ownership. |
| `experiments/engine-kt/` | Optional semantic experiment, not a second source of truth. |

Internal source modules should be named for owned responsibilities, not generic
buckets such as `common`, `manager`, `utils`, or `engine`.

## Technology admission

| Technology | Initial status | Admission evidence |
|---|---|---|
| Rust implementation | accepted | First vertical slice and reliability requirements. |
| Kotlin Agent implementation | experimental | A real JVM/on-device execution need plus shared behavioral conformance before production admission. |
| Go gateway | deferred | Measured edge throughput or operational isolation that the Rust Runtime host cannot meet. |
| Desktop app | deferred | Stable Runtime host/API boundary and a scoped user workflow. |
| Mobile app | deferred | Stable API plus an offline/on-device product requirement. |
| SWE benchmark harness | deferred | Runnable Agent adapter and a reproducible baseline question. |

Experimental and deferred trees must not be described as shipping in lockstep
with the Rust implementation.

## Protocol boundaries

Serialization is used only across a real boundary:

1. client or Channel to Runtime public API;
2. Runtime to an out-of-process service;
3. persisted records whose compatibility must outlive one binary;
4. cross-language integration that has been explicitly admitted.

Internal Rust domain values are not required to mirror protobuf messages.
Proto is not the ubiquitous-language owner and does not define aggregate
boundaries.

## Negative dependencies

- Agent must not depend on Runtime, API, Channel, provider adapters, or apps.
- Model contract must not depend on Agent or Runtime.
- API must not depend on Agent, Runtime implementation, async transports, or
  provider adapters.
- Channel and clients must not depend on Agent internals.
- Provider adapters must not depend on Agent or public API.
- Runtime is the only layer allowed to depend on both Agent and API.

These rules should become executable dependency checks when the first crates
land.

## See also

- [`core/README.md`](core/README.md) — active mechanism documents.
- [`core/loop.md`](core/loop.md) — bounded Agent execution.
- [`core/effect-layer.md`](core/effect-layer.md) — external-effect boundary.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
