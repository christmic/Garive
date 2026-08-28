# Spec → DDD Methodology

> **All design in Garive flows from `spec/` through a DDD lens.**
  Free-form thinking lives in `docs/`; once an idea is firm
  enough to implement faithfully, it lands in `spec/` and the
  domain is shaped before any code is written.

## Pipeline

```
docs/                      spec/                  engine/ + mobile/ + desktop/ + runtime/
  (think)        →           (specify)        →              (model)
```

1. **Explore** in `docs/`. Capture the question, options,
   trade-offs. No code yet.
2. **Specify** in `spec/`. Land a concrete contract:
   - `spec/proto/*.proto` for wire types.
   - `spec/design/<slice>.md` for the normative invariant
     (a paragraph per aggregate, named fields pinned to
     `.proto` tags).
   - `spec/fixtures/` for the data the contract is checked
     against.
3. **Model** in `engine/`, `mobile/`, `desktop/`,
   `runtime/`. The DDD artefacts (below) become concrete
   classes / crates / modules in each language.
4. **Verify** with `just conformance`. The conformance suite
   is the sync lock — implementation is not merge-ready until
   the diff is empty.

## Ubiquitous Language

A shared vocabulary across `docs/`, `spec/`, and code. The
terms used in `spec/proto/*.proto` field names are the source
of truth; code identifiers in every language must mirror those
names (translated to that language's convention).

| Spec term | Rust | Kotlin | TypeScript |
|-----------|------|--------|------------|
| `AgentIdentity` (proto message) | `AgentIdentity` | `AgentIdentity` | `AgentIdentity` |
| `ts_ms` (proto field) | `ts_ms` | `tsMs` | `tsMs` |
| `garive.v1.PingRequest` (proto package) | `garive::v1::PingRequest` | `com.garive.v1.PingRequest` | `garive.v1.PingRequest` |

If the ubiquitous-language mapping disagrees, the `.proto` wins —
fix the implementation, not the contract.

## Bounded Contexts

A bounded context is a **language-and-responsibility boundary**
that owns its own model. Garive ships these bounded contexts:

| Bounded Context | Owner Crate (Rust) | Wire Surface (proto) |
|-----------------|--------------------|-----------------------|
| Core Agent | `engine/core/` | `garive.v1.Agent*`, `garive.v1.Tool*` |
| Memory | `engine/memory/` | `garive.v1.Memory*` |
| Knowledge | `engine/knowledge/` | `garive.v1.Knowledge*` |
| Tools | `engine/tools/` | `garive.v1.Tool*`, `garive.v1.Skill*` |
| Skills | `engine/skill/` | `garive.v1.Skill*` |
| Multi-agent | `engine/multiagent/` | `garive.v1.Session*` |
| Observability | `engine/observability/` | (logs / traces only) |
| Gateway (Go) | `runtime/gateway/` | `garive.v1.Gateway*`, `garive.v1.Replica*` |

A context that needs to talk to another context does so via
**proto messages, not internal types**. Two contexts must not
share a `pub use`.

## Aggregate Boundaries

An aggregate is a consistency boundary. Inside an aggregate:

- Invariants hold after every command.
- All state changes go through the aggregate root.
- External callers see only the aggregate root.

**Mapping rule:** an aggregate root is one proto message, and
all its members live in the same proto package. One aggregate
= one `garive.vN.<Context>.<Aggregate>` proto message.

## Tactical Building Blocks

When modelling in code, use these names consistently across
languages:

| DDD concept | Naming convention |
|-------------|-------------------|
| Aggregate root | `<Name>` (e.g. `Agent`, `Session`, `MemoryEntry`) |
| Value object | `<Name>` (immutable, no identity) |
| Domain event | `<Aggregate><Verb>` past-tense (e.g. `AgentStarted`, `MemoryStored`) |
| Command | `<Verb><Object>` imperative (e.g. `StartAgent`, `StoreMemory`) |
| Repository | `<Aggregate>Repository` (Rust: `trait`; Kotlin: `interface`; TS: type) |
| Domain service | `<Verb><Object>Service` |

Events go on the ledger (`engine/ledger/`) — that is the only
allowed destination for a domain event.

## Cross-language Sync Lock

The conformance suite enforces that **identical domain logic
produces identical canonical JSON across languages** for a
fixed fixture set.

- One fixture = one scenario.
- The fixture names the input AND the expected output, so a
  failing diff points at the exact mismatch.
- An implementation that disagrees with the contract must be
  fixed. An implementation that disagrees with another
  implementation across languages **must also be fixed** —
  drift is a bug, never a negotiation.

## Anti-patterns

- ❌ **Anemic domain model**: a domain type with only getters
  and setters and no behaviour. Move logic into the aggregate.
- ❌ **Shared kernel leaking across contexts**: if two
  contexts both depend on the same Rust type, that type
  probably belongs in its own context or in `spec/`.
- ❌ **Cross-aggregate transactions**: two aggregates must
  not be modified in the same transaction. Communicate via
  domain events.
- ❌ **Proto as DTO only**: if your generated proto types are
  never seen outside the network / IPC boundary, your domain
  model is leaking across tiers. Keep the proto boundary
  thin; the domain types live behind it.
- ❌ **Skipping `spec/`**: writing a Rust type that has no
  proto / no fixture is a design smell. Either move it to
  `spec/` (if it's a contract) or keep it strictly internal
  and document the boundary.