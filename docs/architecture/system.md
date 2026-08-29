# Product module architecture

> Defines Garive's product layers and ownership before implementation begins.
> The first implementation is Rust-first. The full module and App layout is the
> accepted target skeleton; each tier becomes active through concrete slices.

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
Channels -> Runtime composition root -> Providers -> protocol adapters
                |
                +-----------------------> infrastructure adapters
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
| Protocol adapter | One verified, portable wire protocol with typed JSON, errors, and incremental stream decoding. | Garive model types, deployments, vendor defaults, retries, credentials, or recovery. |
| Provider | Deployment/model selection, neutral/protocol mapping, capability admission, and verified provider error policy. | Agent policy, product recovery, environment discovery inside adapters, or public API DTOs. |
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

Garive keeps the product layout below. C0-C6, durable ledgers, portable
protocol/Provider mappings and the first executable client shells are active;
untouched capability crates remain explicit boundaries rather than implied
implementations.

| Path | Responsibility |
|---|---|
| `engine/core/` | Bounded Agent kernel. |
| `engine/llm/` | Provider-neutral model contract; concrete adapters live outside Engine. |
| `engine/tools/` | Tool definitions, immutable prepared calls, and neutral ports. |
| `engine/ledger/` | Durable-fact vocabulary and storage ports; Runtime supplies storage adapters. |
| `engine/memory/`, `engine/knowledge/` | Agent memory/knowledge policy, evidence, and retrieval contracts. |
| `engine/skill/`, `engine/multiagent/` | Skill/delegation semantics and neutral execution ports. |
| `engine/scheduler/` | Scheduling intent/policy; Runtime supplies clocks and workers. |
| `engine/creativity/`, `engine/eval/` | Exploration and evaluation semantics; benchmark I/O remains outside Engine. |
| `engine/config/`, `engine/observability/` | Validated policy values and neutral Agent events, not environment loaders/exporters. |
| `engine/proto/` | Rust bindings for wire contracts admitted through `spec/proto/`. |
| `adapters/` | Provider-independent protocol types, codecs, and incremental stream decoders. |
| `providers/` | Portable deployment composition plus explicit official vendor connection/error profiles; no configuration loading or Runtime transport. |
| `runtime/replica/` | Product Runtime, Session lifecycle, storage, execution, recovery, and composition. |
| `runtime/gateway/` | Planned Go service edge for auth, admission, routing, and load balancing. |
| `clients/host-rs/` | Shared Rust H1 HTTP/SSE client and ephemeral event reducer for CLI/TUI; no Runtime or Engine dependency. |
| `cli/` | One-shot client over the Runtime host boundary. |
| `tui/` | Interactive terminal client over the same boundary. |
| `desktop/`, `mobile/` | Product clients; no Agent or Session ownership. |
| `experiments/engine-kt/` | Experimental Kotlin conformance implementation plus PostgreSQL, protocol, compatible Provider and vendor-profile verification modules. |

Internal source modules should be named for owned responsibilities, not generic
buckets such as `common`, `manager`, `utils`, or `engine`.

## Technology admission

| Technology | Initial status | Admission evidence |
|---|---|---|
| Rust implementation | accepted | First vertical slice and reliability requirements. |
| Kotlin Engine experiment | portable conformance experiment | D0, C0-C5 and admitted capability values/reducers use shared fixtures and native tests; no product-server claim. |
| Go gateway | planned | Activate after the Runtime host contract and first edge workflow exist. |
| Desktop app | configured embedded Runtime | Tauri backend owns R1, strict typed IPC, bounded system configuration and OS credential resolution. |
| Mobile app | live Host client shell | Shared KMP H1 client with Compose and SwiftUI consumers; Android APK evidence remains SDK-gated. |
| SWE benchmark harness | deferred | Runnable Agent adapter and a reproducible baseline question. |

Only slices listed in the cross-language matrix carry a conformance claim;
directory similarity does not create production support or block Rust-only
evolution.

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
- Protocol adapters must not depend on Garive model/Agent/Runtime types or
  public API, and must not read environment configuration.
- Providers may depend on the neutral model contract and protocol adapters but
  must not depend on Agent Core or public API.
- Runtime is the only layer allowed to depend on both Agent and API.

`just architecture` enforces the local Engine → Runtime/App dependency
boundary from Cargo metadata. Language-external product boundaries require
their own checks when admitted.

## See also

- [`core/README.md`](core/README.md) — active mechanism documents.
- [`core/loop.md`](core/loop.md) — bounded Agent execution.
- [`core/effect-layer.md`](core/effect-layer.md) — external-effect boundary.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
