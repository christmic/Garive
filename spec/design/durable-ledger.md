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

## Fact kinds

The first admitted vocabulary is:

| Family | Kinds |
|---|---|
| Session | `session.opened`, `session.closed` |
| Turn | `turn.started`, `turn.input`, `turn.suspended`, `turn.completed`, `turn.stopped`, `turn.failed` |
| Execution | `execution.started`, `execution.completed`, `execution.suspended`, `execution.stopped`, `execution.failed` |
| Model | `model.prepared`, `model.started`, `model.completed`, `model.rejected`, `model.interrupted`, `model.unavailable`, `model.uncertain` |
| Interaction | `interaction.requested`, `interaction.resolved`, `interaction.cancelled` |
| Tool/effect | `effect.prepared`, `effect.authorized`, `effect.denied`, `effect.started`, `effect.receipt`, `effect.completed`, `effect.failed`, `effect.uncertain` |
| Projection | `context.summary`, `privacy.redacted` |

Adding a kind requires a schema-versioned payload spec and recovery decision.

## Aggregate state

### Turn

```text
Open -> Suspended -> Open ... -> Completed | Stopped | Failed
```

`Suspended` keeps the durable Turn resumable. A terminal Turn cannot return to
Open. Product retry after `Stopped`/`Failed` creates a new Turn.

### Execution

```text
Active -> Completed | Suspended | Stopped | Failed
```

Every Execution is terminal exactly once. Continuation creates a new
`ExecutionId` under the same Turn.

### Invocation

```text
Prepared -> Started -> terminal fact
```

The request/effect digest is committed in `Prepared` before dispatch. A
`Started` invocation without a trustworthy receipt/terminal after restart is
`Uncertain`; absence of a result never proves the external operation did not
happen.

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
- `find_model_request(request_id)` and `find_tool_invocation(invocation_id)`
  return lifecycle facts for recovery;
- `list_uncertain_invocations(session_id)` returns only Started invocations
  lacking an admitted terminal/receipt.

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

The schema enforces unique domain IDs, `(session_id, position)`, `fact_id`,
model request ID and tool invocation ID. Payload/digest columns are NOT NULL.
No trigger contains Agent policy; transition validation remains in the Runtime
adapter and domain contract.

Tests use a real temporary database file, close every connection, reopen it and
assert recovery. In-memory SQLite does not prove restart behavior.

## Kotlin PostgreSQL adapter

Location: `runtime/server-kt/persistence-postgres`.

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

Connection pooling and coroutine dispatch live in the adapter/host, not domain
modules. Tests use a real disposable PostgreSQL database. H2, SQLite and mocked
repositories are insufficient for PostgreSQL transaction claims.

## Shared semantic scenarios

`spec/fixtures/ledger/ledger-scenarios.json` covers:

- contiguous append and replay order;
- optimistic version conflict;
- exact idempotent replay and digest collision;
- suspension plus new-execution continuation;
- terminal immutability;
- prepared/started/terminal model lifecycle;
- Started without receipt classified uncertain after restart;
- atomic multi-fact terminal commit;
- fixed through-position reads;
- unknown fact preservation and corruption rejection.

Rust/Kotlin domain implementations consume every scenario. SQLite/PostgreSQL
integration tests replay the same scenarios against their real adapters.

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
