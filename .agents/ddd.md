# Spec → DDD Methodology

> **All design in Garive flows from `spec/` through a DDD lens.**
  Free-form thinking lives in `docs/`; once an idea is firm
  enough to implement faithfully, it lands in `spec/` and the
  domain is shaped before any code is written.

## Pipeline

```
docs/                      spec/                  engine/ + mobile/ + desktop/ + runtime/
  (think)        →           (specify)        →              (model + tests first)
```

1. **Explore** in `docs/`. Capture the question, options,
   trade-offs. No code yet.
2. **Specify** in `spec/`. Land a concrete contract:
   - `spec/proto/*.proto` for wire types.
   - `spec/design/<slice>.md` for the normative invariant
     (a paragraph per aggregate, named fields pinned to
     `.proto` tags).
   - `spec/fixtures/` for the data the contract is checked
     against — **these are the acceptance tests**. Write them
     before any implementation.
3. **Model — tests first.** Per language (`engine/`, `mobile/`,
   `desktop/`, `runtime/`), each slice lands in **three
   commits**, in this order:
   - **3a. Red — failing test.** Write the unit / integration
     test for the slice (an aggregate, a repository, a domain
     service). The test references the fixture in
     `spec/fixtures/` and exercises the public API the
     implementation will offer. The test fails because nothing
     compiles yet — that is the expected state.
   - **3b. Green — minimal implementation.** Write the
     minimum code that makes the test pass. No extras, no
     future-proofing. Compile, run, see green.
   - **3c. Refactor.** With the test as a safety net, clean
     up duplication, sharpen names, push invariants into the
     aggregate root, push logic out of anemic getters/setters.
     Re-run the test. Stay green.
4. **Verify** with `just conformance`. The conformance suite
   is the cross-language sync lock — implementation is not
   rebase-ready until the diff is empty.

The 3a/3b/3c ordering is non-negotiable. A slice lands as one
feature branch with at least three commits, in that order.
Squash-merge (or in our current flow, fast-forward rebase)
keeps the slice visible as a unit while preserving each
TDD step in the branch's local history.

## Test Discipline (Rules)

| Rule | Description |
|------|-------------|
| **Test before code** | The first commit on a feature branch must be a failing test. |
| **Test names describe behaviour, not implementation** | `agent_loop_stops_after_max_turns`, not `test_loop_v2`. |
| **One assertion concept per test** | Multiple `assert_eq!` on related fields are fine; multiple unrelated behaviours in one test are not. |
| **No test deletes code in CI** | Don't `#[ignore]` or `.skip()` a failing test to make CI green. Fix the code. |
| **Fixtures are contracts** | `spec/fixtures/` inputs are read by tests in every language. Edit the fixture, regenerate the conformance diff, fix implementations. |
| **Conformance gates rebase** | `just conformance` empty diff is required before `git rebase origin/master` (see `.agents/git-workflow.md`). |
| **Property tests for invariants** | Aggregate invariants (e.g. "turn count never exceeds `max_turns`") get a `proptest` (Rust) / `kotest-property` (Kotlin) / `fast-check` (TS) suite, not just example tests. |
| **Coverage is a signal, not a goal** | High line coverage with no invariant coverage is worse than low coverage with strong invariant tests. |

## Why Red-Green-Refactor in a DDD Pipeline

DDD shapes the model. TDD shapes the model **before** any
behaviour locks in. The two together:

- **Red** is a design check — if writing the test is awkward,
  the public API of the aggregate / repository / service is
  wrong. Fix the design, not the test.
- **Green** is the smallest cut of behaviour that satisfies
  the contract. Anything more is speculation.
- **Refactor** is where the DDD invariants get pushed into the
  right place — invariants into the aggregate root, queries
  into repositories, orchestration into domain services.

Without the Red step, the implementation tends to grow the
  domain model backwards from "what the code happens to do"
  rather than forwards from "what the contract demands."

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