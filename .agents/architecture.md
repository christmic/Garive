# Architecture rules

> Repository-wide ownership and dependency constraints. Detailed reasoning
> lives in `docs/architecture/system.md`; implementation-ready contracts live
> in `spec/`.

## Product layers

```text
clients -> API/Channel -> Runtime -> Agent -> model contract
                         |           |
                         |           +-> neutral tool/model ports
                         +-> Providers -> protocol adapters
                         +-> infrastructure adapters
```

Dependencies point downward. Runtime is the application composition root.

## Ownership

| Layer | Owns |
|---|---|
| `engine/llm/` | Provider-neutral model values; Provider composition stays behind that contract. |
| `adapters/` | Provider-independent protocol types, codecs, error envelopes, and incremental stream decoders. |
| `providers/` | Deployment/model selection, neutral/protocol mapping, capabilities, and provider-specific error policy. |
| `engine/core/` | One bounded Agent execution, context shaping, iteration policy, prepared calls, and internal outcomes. |
| `engine/tools/` | Tool definitions, immutable prepared calls, neutral authorization/execution ports, and result normalization. |
| `engine/ledger/`, `memory/`, `knowledge/` | Durable-fact vocabulary, memory/knowledge semantics, and neutral storage/retrieval ports. Runtime owns adapters and persistence. |
| `engine/skill/`, `multiagent/`, `scheduler/`, `creativity/`, `eval/` | Agent capability policy and portable contracts. Runtime owns workers, clocks, processes, and benchmark I/O. |
| `engine/config/`, `observability/`, `proto/` | Validated policy values, neutral events, and admitted Rust wire bindings. Environment loading/exporters/codegen live at the boundary. |
| `runtime/replica/` | Product Sessions, durable turns, scheduling, storage, approvals, concrete execution, credentials, recovery, and observability. |
| `runtime/gateway/` | Optional service edge only; auth/routing/rate limits when deployment evidence admits it. |
| `spec/` | Admitted public, cross-process, and persistent compatibility contracts. |
| `cli/`, `tui/`, `desktop/`, `mobile/` | Clients over a Runtime-owned host/API boundary. |
| `experiments/engine-kt/` | Experimental Kotlin Engine conformance implementation and verification adapters; no product Runtime ownership. |

## Hard dependency rules

- Agent code must not depend on Runtime, public API, Channels, clients,
  provider HTTP types, SQLite, credentials, or concrete execution adapters.
- Runtime may depend on Agent, API, provider adapters, and infrastructure.
- Clients and Channels must not depend on Agent internals or storage.
- Protocol adapters must not depend on Garive model types, read environment
  configuration, or own retries, credentials, Provider policy, or recovery.
- Providers must not own Agent retry policy, Session recovery, or UI events.
- Internal Rust domain values do not require protobuf counterparts.
- A new crate or module must have one named owner and no reverse dependency.

## Durable behavior

- Runtime commits a running turn before invoking Agent computation.
- Runtime commits a terminal fact before publishing a terminal client event.
- An external effect uses a stable invocation identity and durable receipt.
- Recovery never blindly replays an effect whose commit status is uncertain.
- Live streaming deltas may be ephemeral; durable outcomes and reconnect
  snapshots are separate contracts.

## Directory admission

The planned product skeleton may reserve a tier when its target role is
accepted. Engine reservations are real Cargo crates and must build; App/service
reservations carry an explicit status and become buildable with their first
slice. Do not create duplicate trees or imply that a placeholder ships.

## Reference

- `docs/architecture/system.md` — product ownership rationale.
- `docs/architecture/core/README.md` — active mechanism index.
- `spec/README.md` — contract promotion rules.
