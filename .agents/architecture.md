# Architecture rules

> Repository-wide ownership and dependency constraints. Detailed reasoning
> lives in `docs/architecture/system.md`; implementation-ready contracts live
> in `spec/`.

## Product layers

```text
clients -> API/Channel -> Runtime -> Agent -> model contract
                         |           |
                         |           +-> neutral tool/model ports
                         +-> provider and infrastructure adapters
```

Dependencies point downward. Runtime is the application composition root.

## Ownership

| Layer | Owns |
|---|---|
| `engine/llm/` | Provider-neutral model values; admitted provider adapters stay behind that contract. |
| `engine/core/` | One bounded Agent execution, context shaping, iteration policy, prepared calls, and internal outcomes. |
| `engine/tools/` | Tool definitions, immutable prepared calls, neutral authorization/execution ports, and result normalization. |
| `runtime/replica/` | Product Sessions, durable turns, scheduling, storage, approvals, concrete execution, credentials, recovery, and observability. |
| `runtime/gateway/` | Optional service edge only; auth/routing/rate limits when deployment evidence admits it. |
| `spec/` | Admitted public, cross-process, and persistent compatibility contracts. |
| `cli/`, `tui/`, `desktop/`, `mobile/` | Clients over a Runtime-owned host/API boundary. |
| `experiments/engine-kt/` | Optional semantic experiment; never a second source of truth. |

## Hard dependency rules

- Agent code must not depend on Runtime, public API, Channels, clients,
  provider HTTP types, SQLite, credentials, or concrete execution adapters.
- Runtime may depend on Agent, API, provider adapters, and infrastructure.
- Clients and Channels must not depend on Agent internals or storage.
- Provider adapters must not own Agent retry policy, Session recovery, or UI
  events.
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

Do not create empty directories for possible future capabilities. Add a source
directory when a scoped slice has an owner, a dependency direction, and a
runnable verification command. Deferred languages, gateways, and apps remain
documented decisions rather than tracked placeholders.

## Reference

- `docs/architecture/system.md` — product ownership rationale.
- `docs/architecture/core/README.md` — active mechanism index.
- `spec/README.md` — contract promotion rules.
