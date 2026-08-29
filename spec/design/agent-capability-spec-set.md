# Agent capability implementation Spec set

> Review map for the portable capability contracts that follow the completed
> D0/C0-C6 and H1 foundation.

## Status

Accepted implementation index. The project owner directed every Spec in this
set to proceed through verified development on 2026-08-29. Coordinated
C3/C6F/L0 and fixture changes still precede each behavior slice.

## Purpose

Turn the remaining accepted capability ownership descriptions into executable
contracts without creating a second Runtime inside Engine. This set covers
Memory, Knowledge, Skill, Scheduler, Multi-Agent delegation and Observability.
Compression, Creativity, Evaluation and hosted vendor extensions remain gated
by their existing admission evidence.

## Normative order

| Order | ID | Contract | Portable owner | Runtime owner |
|---:|---|---|---|---|
| 1 | S0 | [`skill-activation.md`](skill-activation.md) | exact instruction-skill values and activation reduction | registry resolution and durable activation commit |
| 2 | M0 | [`memory-capability.md`](memory-capability.md) | memory proposal/query/result semantics | namespace authority, persistence, retention and receipts |
| 3 | K0 | [`knowledge-retrieval.md`](knowledge-retrieval.md) | source/query/evidence/citation semantics | connectors, credentials, retrieval durability and freshness |
| 4 | Q0 | [`durable-scheduler.md`](durable-scheduler.md) | schedule intent and recurrence values | clocks, durable leases, workers and command dispatch |
| 5 | MA0 | [`multi-agent-delegation.md`](multi-agent-delegation.md) | delegation intent, budget and result reduction | child identity/lifecycle, authority, persistence and recovery |
| 6 | O0 | [`agent-observability.md`](agent-observability.md) | low-cardinality signal and measurement values | redaction, buffering, exporters and operational policy |

[`capability-runtime-facts.md`](capability-runtime-facts.md) is the coordinated
CF0 payload companion for S0/M0/K0/Q0/MA0 and must be accepted with this set.

S0 is first because it is an immutable context capability with no external
effect. M0 and K0 then define two deliberately different evidence sources. Q0
uses the completed C6 command/recovery boundary. MA0 then adds bounded child
Turn ownership over that foundation. O0 observes all slices but can never
become a prerequisite for their correctness.

## Shared boundary map

```text
Effective Agent Snapshot
  +-- exact Skill revisions -------------> bounded activation context
  +-- Memory descriptor -> Runtime port --> committed memory evidence
  +-- Knowledge source -> Runtime port ----> committed cited evidence

Runtime schedule facts -> leased due occurrence -> idempotent C6 command

parent delegation -> governed budget -> child Turn -> parent continuation

durable facts + live semantic events -> Runtime redaction -> observability sink
```

The following rules apply to every contract:

1. capability availability is frozen in the effective snapshot;
2. Engine never discovers stores, connectors, workers, exporters or secrets;
3. content that can influence a later model request is committed before that
   request starts and is bound by digest/reference;
4. a live callback, cache hit or exporter success is not durable evidence;
5. unsupported is explicit and cannot silently switch to weaker semantics;
6. identifiers are typed and cannot substitute for Session, Turn, Execution,
   model request, tool invocation or one another;
7. bounded counts and byte/token limits are construction inputs, not process
   environment defaults.

## Coordinated durable vocabulary

Acceptance admits the following fact families for a later coordinated C6F/L0
amendment. The focused Specs define their exact payloads and transitions:

- `skill.activated`;
- `memory.proposed`, `memory.committed`, `memory.rejected`, `memory.superseded`,
  `memory.tombstoned`, `memory.retrieval_recorded`;
- `knowledge.requested`, `knowledge.dispatched`, `knowledge.completed`,
  `knowledge.failed`;
- `schedule.created`, `schedule.claimed`, `schedule.fired`,
  `schedule.skipped`, `schedule.cancelled`, `schedule.failed`;
- `delegation.requested`, `delegation.authorized`, `delegation.denied`,
  `delegation.child_started`, `delegation.child_terminal`,
  `delegation.observed`.

O0 adds no durable fact family. It derives signals from existing committed
facts and live events. Each name becomes a valid L0 fact only when its focused
behavior slice lands with the matching C6F schema, validators and shared
fixture. S0, M0 and K0 have admitted their listed facts; later families stay
opaque until their slices land.

## Cross-language delivery target

Portable semantic values and reducers are targeted for production Rust and
experimental Kotlin conformance. Runtime adapters remain independently
verified: Rust/SQLite is production-first; no Kotlin product Runtime parity is
claimed.

| Slice | Shared fixture | Rust evidence | Kotlin evidence | Runtime evidence |
|---|---|---|---|---|
| S0 | `agent/skill-activation-v1.json` | native + fixture | native + fixture | SQLite commit-before-model test |
| M0 | `agent/memory-capability-v1.json` | native + fixture | native + fixture | restart, authority and retention tests |
| K0 | `agent/knowledge-retrieval-v1.json` | native + fixture | native + fixture | fake connector + crash-position tests |
| Q0 | `agent/durable-scheduler-v1.json` | native + fixture | value/reducer fixture | real SQLite clock/lease/restart tests |
| MA0 | `agent/multi-agent-delegation-v1.json` | native + fixture | native + fixture | SQLite parent/child process-kill matrix |
| O0 | `agent/observability-v1.json` | native + fixture | native + fixture | exporter backpressure/redaction tests |

Fixtures are semantic conformance data, not public DTOs. Canonical byte
identity is required only for explicitly named digest preimages.

## Explicit exclusions

- autonomous extraction of model statements into trusted memory;
- treating retrieval rank as truth, authority or permission;
- executable plugins hidden behind a Skill descriptor;
- cron/time-zone syntax without a separate calendar compatibility contract;
- scheduling a worker by holding an in-memory timer only;
- raw prompts, model output, credentials or unbounded IDs as metric labels;
- adaptive compression thresholds without measured C3/C6 evidence;
- delegation or child Agent lifecycle hidden in Skill or Scheduler;
- parallel fan-out, DAG, swarm or voting semantics before MA0 evidence.

## Acceptance gate

The owner may accept this set only when:

1. each public value, identity, limit and stable failure has one owner;
2. every model-visible value names its durability/crash boundary;
3. Memory, Knowledge and Skill cannot grant tool or connector authority;
4. scheduler lease loss cannot duplicate semantic command identity;
5. delegation cannot create budget, inherit authority or reuse a child Turn;
6. observability loss cannot affect Agent state or recovery;
7. C3/C6F/L0 and the Rust/Kotlin matrix changes are listed before behavior;
8. the old research documents are explicitly non-normative where they conflict.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
