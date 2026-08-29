# L0/L1 — durable Agent ledger

## Status

Accepted portable ledger contract plus database-specific requirements for the
Rust SQLite host and Kotlin PostgreSQL host.

## Responsibility

Persist the facts required to reconstruct Sessions, Turns and external
invocations without serializing an in-memory Agent loop. The ledger is
append-only at the fact boundary; mutable projection rows are caches guarded by
the same transaction and can be rebuilt from facts.

## Identities

The ledger preserves distinct non-empty identities:

- `SessionId`, `TurnId`, `ExecutionId`;
- `FactId` supplied by Runtime for idempotent append;
- `ModelRequestId` and `ToolInvocationId`;
- Agent Instance/Definition identity and exact definition revision;
- optional content-addressed payload reference.

No database-generated integer substitutes for a public/domain identity. A
database may use internal row keys only as an implementation detail.
Turn, Execution, model-request and tool-invocation identities are globally
bound to the Session that first commits them. A different Session cannot reuse
one of those identities; otherwise `load_turn` and recovery queries would be
ambiguous across adapters.

## Durable position

Each Session owns a monotonically increasing `u64` fact position beginning at
one. Position order is the authoritative replay order; wall-clock time is
metadata and never ordering truth.

Appending one transaction reserves a contiguous position range. A failed or
rolled-back transaction exposes no positions. Concurrent appends for one
Session are serialized by the adapter. Different Sessions may progress
independently.

## Fact envelope

```text
DurableFact {
  fact_id,
  session_id,
  position,
  turn_id?, execution_id?,
  model_request_id?, tool_invocation_id?,
  kind,
  schema_version,
  payload_json,
  payload_sha256,
  recorded_at,
}
```

`payload_json` is UTF-8 JSON validated before append. The digest is over the
contract-declared canonical representation and binds idempotency. Unknown fact
kinds/schema versions remain readable as opaque audit facts but are not applied
to a projection without an admitted decoder.

`recorded_at` is a syntactically valid RFC 3339 timestamp with an explicit
offset. Adapters may normalize its storage representation; it is excluded from
idempotency equality because a retry may reconstruct transport metadata, and it
never participates in ordering.

Canonical payload version 1 accepts JSON null, booleans, strings, arrays,
objects and integer numbers representable as signed or unsigned 64-bit values.
It rejects floating-point/exponent values and duplicate object keys. Objects
are recursively sorted by Unicode scalar-value key order, insignificant
whitespace is removed, arrays retain order, and strings use the platform JSON
encoder's minimal escapes. The SHA-256 digest is lowercase hexadecimal over
the resulting UTF-8 bytes. A future numeric surface requires a new canonical
payload version rather than language-native floating-point formatting.

## Fact kinds

The first admitted vocabulary is:

| Family | Kinds |
|---|---|
| Session | `session.opened`, `session.closed` |
| Turn | `turn.started`, `turn.input`, `turn.cancel_requested`, `turn.suspended`, `turn.completed`, `turn.stopped`, `turn.failed` |
| Execution | `execution.started`, `execution.abandoned`, `execution.completed`, `execution.suspended`, `execution.stopped`, `execution.failed` |
| Model | `model.prepared`, `model.started`, `model.completed`, `model.rejected`, `model.interrupted`, `model.unavailable`, `model.uncertain` |
| Interaction | `interaction.requested`, `interaction.resolved`, `interaction.cancelled` |
| Tool/effect | `tool.preparation_rejected`, `effect.prepared`, `effect.authorized`, `effect.denied`, `effect.started`, `effect.receipt`, `effect.completed`, `effect.failed`, `effect.uncertain`, `effect.observation` |
| Skill | `skill.activated` |
| Memory | `memory.proposed`, `memory.committed`, `memory.rejected`, `memory.superseded`, `memory.tombstoned`, `memory.retrieval_recorded` |
| Knowledge | `knowledge.requested`, `knowledge.completed`, `knowledge.failed` |
| Scheduler | `schedule.created`, `schedule.claimed`, `schedule.fired`, `schedule.cancelled`, `schedule.failed` |
| Delegation | `delegation.requested`, `delegation.authorized`, `delegation.denied`, `delegation.child_started`, `delegation.child_terminal`, `delegation.observed` |
| Projection | `context.summary`, `privacy.redacted` |

Adding a kind requires a schema-versioned payload spec and recovery decision.
The accepted C6 shapes are defined by
[`durable-runtime-facts.md`](durable-runtime-facts.md); accepted capability
shapes are defined by [`capability-runtime-facts.md`](capability-runtime-facts.md).

## Aggregate state

### Turn

```text
Open -> Suspended -> Open ... -> Completed | Stopped | Failed
```

`Suspended` keeps the durable Turn resumable. A terminal Turn cannot return to
Open. Product retry after `Stopped`/`Failed` creates a new Turn.
`turn.started(kind=start)` creates an absent Turn;
`turn.started(kind=continue)` reopens the same Turn from Suspended and binds the
consumed suspension. The following `execution.started` carries a new Execution
identity.

### Execution

```text
Active -> Abandoned | Completed | Suspended | Stopped | Failed
```

Every Execution is terminal exactly once. Continuation creates a new
`ExecutionId` under the same Turn. An Execution cannot become terminal while
one of its model requests or effects remains `Started`, or while an effect has
a receipt that still lacks its explicit completion/failure classification.
Likewise, a Turn cannot suspend or terminate while one of its Executions is
active, and a Session cannot close while any Turn, Execution, or dispatched
invocation remains non-terminal. These ordering checks prevent a parent from
closing before the child facts needed for deterministic recovery can be
appended.

`Abandoned` is Runtime-only recovery truth for a lost disposable Kernel
invocation. It is admitted only after all child invocations are terminal,
pre-dispatch, or explicitly classified safe/uncertain. It does not terminally
close the Turn; Runtime may atomically abandon it and start a fresh Execution
under the still-open Turn, subject to the C6 recovery bound.

### Invocation

```text
Prepared -> Started -> terminal fact
effect.uncertain -> effect.reconciled -> effect.observation
```

The request/effect digest is committed in `Prepared` before dispatch. A
`Started` invocation without a trustworthy receipt/terminal after restart is
`Uncertain`; absence of a result never proves the external operation did not
happen.

Tool authorization is an optional transition between `Prepared` and
`Started`. A trustworthy `effect.receipt` proves that the effect returned and
may be followed by `effect.completed`; it is recovery-terminal for uncertainty
queries but does not replace the explicit completion fact.

`effect.uncertain` cannot transition directly to an observation or ordinary
executor terminal. Only the C6 operator-reconciliation transaction may append
`effect.reconciled`, with durable evidence and a model-safe observation.

## Append transaction

Runtime calls:

```text
commit(expected_session_version, facts, projection_change?)
  -> committed version + contiguous positions
```

The adapter must atomically:

1. lock/compare the Session version;
2. validate identities, aggregate transitions and invocation digest binding;
3. assign contiguous positions;
4. insert every fact;
5. update the optional Session/Turn/Execution projection;
6. advance the Session version;
7. commit before reporting success.

Version mismatch returns `ConcurrentModification` without a partial append.
`FactId` replay with the same digest returns the original committed position;
the same ID with another digest returns `IdempotencyCollision`.

## Read ports

- `read_facts(session_id, after_position, through_position, kinds)` returns
  strictly ordered immutable facts;
- `load_turn(turn_id)` returns the projection plus its fact/version watermark;
- `list_recoverable_turns(session_id)` returns the IDs whose projection is Open
  or Suspended, ordered by Turn ID; callers then freeze each Turn with
  `load_turn` before selecting a recovery action;
- `find_model_request(request_id)` and `find_tool_invocation(invocation_id)`
  return lifecycle facts for recovery;
- `list_uncertain_invocations(session_id)` returns only Started invocations
  lacking an admitted terminal/receipt.

The enumeration is only a discovery hint. Recovery decisions never use its
possibly stale projection row: they are derived again from the verified fixed
prefix returned by `load_turn`.

Pagination uses durable position, never offset or wall time. A read captures a
fixed `through_position` so one Kernel Execution sees a stable prefix.

## Corruption behavior

Digest mismatch, missing referenced identity, non-monotonic position,
impossible transition or projection watermark beyond the fact stream returns a
typed corruption error. Runtime fails closed and exposes operator
reconciliation; the adapter does not skip the row or rebuild from guessed data.

## Rust SQLite adapter

Location: `runtime/replica`.

Required connection policy:

- SQLite foreign keys enabled;
- WAL journal mode;
- `synchronous=FULL` for durability claims;
- bounded busy timeout;
- explicit schema migration table;
- write transactions begin IMMEDIATE to serialize one-writer allocation.

The schema enforces unique domain IDs, `(session_id, position)` and `fact_id`.
Partial unique indexes admit exactly one `model.prepared` fact per model request
ID and one `effect.prepared` fact per tool invocation ID; later lifecycle facts
reuse those identities. Payload/digest columns are NOT NULL.
No trigger contains Agent policy; transition validation remains in the Runtime
adapter and domain contract.

Tests use a real temporary database file, close every connection, reopen it and
assert recovery. In-memory SQLite does not prove restart behavior.

## Kotlin PostgreSQL adapter

Location: `experiments/engine-kt/persistence-postgres`.

Required transaction policy:

- PostgreSQL migrations are versioned and run before serving;
- the Session projection row is locked with `SELECT ... FOR UPDATE` while
  comparing/advancing its version;
- fact positions and projection changes commit in one transaction;
- unique constraints bind domain/idempotency identities;
- JSON payload is stored as `jsonb`, while the canonical digest is stored and
  checked independently;
- timestamps use `timestamptz`; replay ordering still uses position;
- statement/lock timeouts are explicit.

PostgreSQL serialization abort `SQLSTATE 40001` is the storage-native form of
an optimistic writer race and is normalized to `ConcurrentModification`; it is
not exposed as a generic durability failure.

Connection pooling and coroutine dispatch live in the adapter/host, not domain
modules. Tests use a real disposable PostgreSQL database. H2, SQLite and mocked
repositories are insufficient for PostgreSQL transaction claims.

## Shared semantic scenarios

`spec/fixtures/ledger/ledger-scenarios.json` covers:

- contiguous append and replay order;
- optimistic version conflict;
- exact idempotent replay and digest collision;
- suspension plus new-execution continuation;
- abandoned-execution recovery with a fresh Execution identity;
- terminal immutability;
- prepared/started/terminal model lifecycle;
- Started without receipt classified uncertain after restart;
- Started tool effects without receipt/terminal classified uncertain after
  restart;
- durable preparation rejection, effect observation, and cancellation request;
- atomic multi-fact terminal commit;
- fixed through-position reads;
- unknown fact preservation and corruption rejection.

Rust/Kotlin domain implementations consume every scenario. SQLite/PostgreSQL
integration tests replay the same scenarios against their real adapters.

The shared scenarios are complemented by mirrored exhaustive domain matrices
in `engine/ledger/tests/ledger_transition_matrix.rs` and
`experiments/engine-kt/ledger/src/test/.../LedgerTransitionMatrixTest.kt`.
Those matrices enumerate every admitted Turn, Execution, model and effect
terminal; reject skipped/repeated/cross-owner transitions and premature parent
closure; and cover commit, query, identity and canonical-payload boundaries.

## Backup and migration boundary

L1 requires forward migrations from an empty database and schema-version
refusal for a newer unsupported database. Online backup, retention, compaction,
replication and disaster-recovery operations require separate measured specs;
they are not implied by WAL or PostgreSQL durability.

## Acceptance

- portable domain/fixture tests pass in Rust and Kotlin;
- SQLite close/reopen tests prove committed recovery and rollback invisibility;
- PostgreSQL integration tests prove locking, conflicts and unique constraints;
- no Engine domain crate imports SQL libraries;
- uncertain effects are never automatically replayed without their replay
  contract and exact receipt evidence;
- every successful terminal reported to a client is committed first.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
