# Design-to-domain workflow

> Garive uses domain language to clarify behavior and ownership. DDD does not
> force every internal type into protobuf, every directory into a bounded
> context, or every feature into a fixed commit choreography.

## Workflow

```text
docs discussion -> accepted architecture -> spec when needed -> tests -> code
```

1. **Discuss in `docs/`.** Record the problem, alternatives, trade-offs, and
   unresolved questions.
2. **Settle ownership.** Name the layer that owns the behavior and verify the
   dependency direction in `docs/architecture/system.md`.
3. **Promote a contract when a boundary needs compatibility.** Use `spec/`
   for public API, cross-process, cross-language, or durable persistence
   contracts. Purely internal behavior can remain a typed Rust contract.
4. **Write the test first.** Observe the test fail locally, then implement the
   smallest behavior that passes it.
5. **Land green commits.** Every committed step remains buildable and testable;
   a failing TDD checkpoint does not become permanent trunk history.

## Ubiquitous language

The design document that owns a concept names it. Code uses the same concept
with language-appropriate naming. A wire schema owns only its serialized field
names and compatibility rules; it does not automatically own internal domain
vocabulary.

| Concept | Owner |
|---|---|
| Product `Session`, durable `Turn`, authenticated actor | Runtime |
| `AgentTurnRequest`, `AgentExecutionPorts`, `AgentOutcome` | Agent kernel |
| Prepared tool call and execution requirement | Agent/tool contract |
| Invocation grant, effect receipt, recovery decision | Runtime execution |
| Public request/event/view fields | Public API spec |
| Provider request/response fields | Verified provider adapter |

Avoid ambiguous names such as `SessionContext`, `Manager`, `EngineState`, or
`Message` when they combine owners. Prefer names that reveal authority and
lifetime.

## Boundaries before aggregates

An aggregate is useful only when a real consistency boundary exists. Do not
predeclare one aggregate per crate or map one aggregate to one proto message.

For each stateful concept, answer:

1. Who may change it?
2. What transaction or atomic fact makes the change durable?
3. What invariant must hold after the change?
4. What identity survives retries and restarts?
5. Which projections may be rebuilt from durable facts?

Runtime may commit multiple records in one transaction when product
correctness requires it. A blanket ban on cross-aggregate transactions is not
a substitute for defining the actual atomic boundary.

## Domain and wire types

- Internal domain values are handwritten and optimized for invariants.
- Public, cross-process, and persistent compatibility values are specified at
  their boundary and mapped explicitly.
- Generated protobuf types remain at the serialization edge unless a specific
  design proves they are also the correct internal value.
- Two modules in the same process may share a stable internal contract without
  serializing through proto.
- Mapping code is intentional evidence of a boundary, not duplication to hide.

## Events and durable facts

Not every internal event belongs in one universal ledger.

| Kind | Owner | Durability |
|---|---|---|
| Agent progress event | Agent/Runtime bridge | ephemeral unless promoted |
| Client streaming delta | Runtime/Channel | ephemeral; reconnect uses snapshots |
| Turn lifecycle terminal | Runtime | durable |
| External-effect receipt | Runtime execution | durable before terminal publication |
| Audit/approval decision | Runtime authorization | durable |
| Provider wire event | Provider adapter | normalized or discarded at boundary |

Persist the minimum facts needed for correctness, recovery, audit, and product
history. Build query/UI projections from those facts rather than forcing every
runtime signal into the durable model.

## Test discipline

- Write a failing test before implementation, but commit only green states.
- Fixtures describe accepted behavior; they are not automatically wire
  contracts.
- Property tests cover declared invariants and state machines.
- Cross-language conformance is required only for behavior intentionally
  supported by more than one implementation.
- Coverage and benchmark numbers become gates after a measured baseline, not
  before.

## Anti-patterns

- A wire DTO carrying execution authority.
- Agent code opening a database, resolving credentials, or selecting a concrete
  sandbox.
- Runtime behavior hidden behind provider-specific response types.
- A generic module name that combines unrelated owners.
- A fixture changed only to make an implementation pass.
- A speculative bounded context or proto message created because a directory
  already exists.

## Reference

- `docs/architecture/system.md` — product ownership.
- `docs/architecture/core/README.md` — active mechanism index.
- `.agents/multi-language.md` — optional cross-language admission.
- `.agents/testing.md` — verification layers and evidence maturity.
