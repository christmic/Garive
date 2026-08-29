# C6 — Durable Runtime Turn orchestration and recovery

> Contract for Runtime/storage engineers and reviewers defining the transaction
> order, continuation protocol, crash decisions, and publication rule for one
> restart-safe Agent Turn.

## Audience

Engineers implementing the Rust Runtime/SQLite host, Kotlin/PostgreSQL
experiment, Core port bridge, recovery coordinator, or Host integration.

## Why

Core memory is disposable and L0 provides facts, but neither alone defines
when requests/effects become durable or how a host replaces a lost execution.
Without this contract, restart behavior would depend on process timing.

## Status

Accepted implementation contract for C6.

## Scope and owner

C6 composes accepted Engine contracts into one durable Turn. Runtime owns
Session/Turn lifecycle, definition resolution, storage transactions,
continuations, request/effect dispatch, recovery and Host publication. Core
remains a disposable bounded function and never owns a database or resumable
in-memory Turn object.

This Spec defines Runtime semantics, not public Host wire fields, provider
mapping, concrete executor isolation, scheduler topology, retention, backup or
adaptive compression.

## Runtime command surface

```text
StartTurn {
  command_id, session_id, agent_instance_id,
  exact_definition_id_revision, trusted_input,
  product_policy_reference
}

ContinueTurn {
  command_id, session_id, turn_id,
  continuation_input,
  expected_suspension_id, expected_session_version
}

CancelTurn { command_id, session_id, turn_id, reason }
ReconcileInvocation { command_id, invocation_id, operator_evidence, decision }
GetTurn { session_id, turn_id, through_position? }
```

Runtime creates `TurnId`, `ExecutionId`, `ModelRequestId`,
`ToolInvocationId`, `InteractionId`, `SuspensionId`, `GrantId`, `ReceiptId`,
dispatch-attempt identities and durable `FactId` values. The command caller
supplies a non-empty `CommandId`. Client/model identities are never reused in
their place. `command_id` provides idempotency: same command and canonical
payload returns the committed result; a different payload is a conflict.

Commands use optimistic Session versioning. A conflict commits nothing and
returns the latest durable watermark needed to retry deliberately.

## Start transaction

Before invoking Core, Runtime atomically commits:

1. `turn.started(kind=start)`, binding Session, Agent Instance, exact
   definition revision, Effective Agent Snapshot digest and trusted-input
   digest;
2. `turn.input`, containing/referring to the admitted trusted input;
3. `execution.started`, with a fresh Execution ID, reconstructed cursor,
   effective limits and snapshot digest.

Runtime then freezes all ports and calls Core. Definition resolution failure
commits no Turn unless product policy explicitly records a separate rejected
command audit fact. A crash after this transaction and before Core produces an
active Execution with no started external invocation; recovery may invoke a
new Execution only after terminally classifying the abandoned Execution.

## Kernel boundary protocol

Runtime supplies immutable `AgentTurnRequest + AgentExecutionPorts`. Port calls
that cross a durability or external-effect boundary return only after the
required ledger transaction commits. Core events are proposals until Runtime
maps them:

- live deltas may be published without persistence and may be lost;
- durable lifecycle/effect/interaction/terminal events commit first;
- payloads are redacted and schema-versioned before persistence/publication;
- a port error never implies that an external action did not occur.

Runtime accepts exactly one `AgentOutcome` per Execution and converts it into
one atomic terminal transaction. Duplicate equal terminal proposals are
idempotent; a different second terminal is corruption/invariant failure.

A host crash can destroy an active Kernel invocation before it returns any
`AgentOutcome`. C6 therefore adds Runtime-only `execution.abandoned`: it
terminally records loss of the disposable invocation without claiming a Core
failure and without terminally closing the Turn. It is legal only during
restart reconstruction, after all child model/effect lifecycles are safe or
explicitly classified. A bounded Runtime recovery counter prevents endless
abandon/restart loops. Acceptance of C6 requires adding this fact and transition
to L0 before implementation.

## Durable payload profiles v1

Every C6 fact uses L0 canonical payload v1, `schema_version = 1`, lowercase
hexadecimal digests and envelope-owned typed identities. Payloads use integer
counts/durations only; secret text, credentials, raw provider/executor data and
wall-clock ordering do not enter semantic fields.

| Fact | Required v1 payload fields |
|---|---|
| `turn.started` | `command_id`, `agent_instance_id`, `definition_id`, `definition_revision`, `snapshot_digest`, `trusted_input_digest` |
| `turn.input` | `input_kind`, `content_digest`, optional `content_reference` |
| `execution.started` | `snapshot_digest`, `through_position`, `completed_iterations`, effective limits, `recovery_ordinal` |
| `execution.abandoned` | `reason=runtime_lost`, last safe position, `recovery_ordinal` |
| `model.prepared` | neutral request digest, capability/deployment identity, recovery-policy revision, attempt bound |
| `model.started` | request digest, dispatch-attempt identity |
| model terminal | request digest, normalized outcome kind, usage evidence, optional bounded content/evidence reference |
| `effect.prepared` | Prepared Call digest, tool/revision, replay class, model call correlation |
| effect authorization | Prepared digest, authority revision, grant/denial code and granted limits where applicable |
| `effect.started` | Prepared digest, grant ID, executor identity/revision, dispatch-attempt identity |
| `effect.receipt` | receipt ID, Prepared digest, grant ID, executor terminal classification, result/evidence digest/reference |
| effect terminal | Prepared digest, terminal class/code, receipt/result binding |
| `effect.observation` | Prepared digest, model call ID, v1 governed-observation digest/content reference |
| `tool.preparation_rejected` | source model request/call, proposed tool name, stable C4 code/path content |
| interaction facts | interaction ID, invocation ID, Prepared digest, suspension ID, schema/response digest, state/expiry code |
| execution/Turn terminal | typed outcome/reason, cumulative usage, response/continuation/evidence digest/reference |
| `turn.cancel_requested` | command ID, stable reason code, requested-through position |

Optional fields are absent rather than guessed. Large content uses a
content-addressed reference plus digest; the append transaction verifies the
reference contract before reporting success. Exact value shapes and enum
catalogues are defined by [`durable-runtime-facts.md`](durable-runtime-facts.md).
Unknown newer versions remain auditable but cannot drive recovery.

Acceptance of C6 therefore amends the L0 vocabulary with
`execution.abandoned`, `effect.observation`, `tool.preparation_rejected` and
`turn.cancel_requested` plus their transition rules. Until that coordinated
amendment lands, C6 behavior implementation remains gated and L0's existing
`done` claim stays scoped to its prior vocabulary.

## Model request lifecycle

For each logical neutral model request:

```text
model.prepared -> model.started ->
  model.completed | model.rejected | model.interrupted |
  model.unavailable | model.uncertain
```

`model.prepared` binds request ID, Execution ID, neutral request digest,
selected model capability/deployment identity, recovery-policy revision and
attempt budget before dispatch. `model.started` commits immediately before the
provider boundary. Runtime/Provider may perform only retries proven safe by the
accepted policy while retaining the logical request identity and recording
attempt evidence outside Core semantics.

A crash or transport loss after `model.started` without a normalized terminal
becomes `model.uncertain`. Runtime may retry only when the Provider contract
proves the request has no external side effect and the same logical response
cannot later be double-applied; otherwise it suspends/fails according to the
frozen recovery policy. A normalized terminal and its usage bind atomically.

## Effect and interaction lifecycle

C6 persists the exact C5 transitions and bindings:

- `effect.prepared`: invocation ID, Prepared Call digest, tool/revision,
  replay class and model correlation;
- `effect.authorized|denied`: authority revision and exact digest;
- `interaction.requested|resolved|cancelled`: interaction, invocation,
  suspension and response digest bindings;
- `effect.started`: grant/executor identity and dispatch boundary;
- `effect.receipt`: trustworthy executor receipt digest/reference;
- `effect.completed|failed|uncertain`: one terminal classification;
- model-visible observation: exact invocation/model correlation and bounded
  neutral result digest/reference.

An observation must commit before a later `model.prepared` that includes it.
Receipt recovery and uncertain decisions follow C5; C6 cannot downgrade them
for availability.

## Execution and Turn terminal transactions

Runtime atomically maps Core outcome:

| Core outcome | Required durable facts | Turn state |
|---|---|---|
| `Completed` | `execution.completed` + `turn.completed` + final response/usage binding | terminal |
| `Suspended` | `execution.suspended` + `turn.suspended` + typed continuation requirement | resumable |
| `Stopped` | `execution.stopped` + `turn.stopped` + reason/usage binding | terminal |
| `Failed` | `execution.failed` + `turn.failed` + stable failure/evidence binding | terminal |

`execution.abandoned` is not produced from a Core outcome and is not published
as a user terminal. Runtime may atomically abandon the lost Execution and start
a fresh Execution under the still-open Turn after recovery proves that no child
invocation remains uncertain. The new cursor counts committed model/effect
work, never lost in-memory iteration increments.

No parent terminal may commit while a child model/effect/interaction remains in
an illegal active state under L0. Runtime publishes a terminal Host event only
after this transaction commits. If publication fails, reconnect reads the
durable projection; Runtime does not append a second terminal.

## Continuation

Only a durably Suspended Turn may continue. Runtime validates the expected
suspension identity, input kind/schema, expiry, unresolved interaction or
reconciliation target, Session version, definition revision and snapshot
digest. It then atomically:

1. commits the continuation input/resolution;
2. appends `turn.started(kind=continue)` bound to the consumed suspension,
   definition and snapshot, reopening the Turn from Suspended;
3. starts a fresh Execution ID with a cursor reconstructed from a fixed durable
   prefix and cumulative limits/usage;
4. changes the projection from Suspended to Open in the same transaction.

There is no `resume()` call on old Core state. Equal replay returns the same
committed continuation; conflicting/replayed-after-consumption input fails.
Stopped, Failed and Completed Turns cannot continue.

## Restart reconstruction

On startup or lease takeover, Runtime treats the ledger as sole truth:

1. load and integrity-check a fixed fact prefix and projection watermark;
2. verify identity, digest and transition bindings;
3. enumerate active Executions and started/receipt-only invocations;
4. classify each model/effect using its accepted recovery contract;
5. append explicit recovered terminal/uncertain facts transactionally;
6. suspend for typed operator action when safety cannot be proved;
7. otherwise append `execution.abandoned` and start a fresh Execution, or
   report an already committed terminal.

Unknown required fact schema, digest mismatch, impossible transition, missing
referent or projection-ahead-of-ledger is corruption. Runtime fails closed and
exposes redacted reconciliation status; it does not skip or guess.

## Crash-boundary matrix

Native process-restart tests must kill the host at each boundary:

| Boundary | Recovery assertion |
|---|---|
| before/after start transaction | absent command or one boundedly abandoned/reconstructed Execution |
| before/after `model.prepared` | no dispatch without preparation |
| before/after `model.started` | safe policy decision; no fabricated terminal |
| after model terminal before next iteration | normalized result applied once |
| before/after `effect.prepared` | no authorization/dispatch without preparation |
| before/after authorization | no unapproved dispatch; exact grant binding |
| before/after `effect.started` | replay only with C5 proof; otherwise uncertain |
| after receipt before result | recover receipt without re-execution |
| after result before observation | observation reconstructed once |
| before/after interaction resolution | one consumed response and fresh Execution |
| before/after terminal transaction | no partial parent/child terminal state |
| after terminal before publish | reconnect returns committed terminal once |

Fault injection names stable checkpoints; elapsed timing is not the oracle.

## Concurrency, leases and cancellation

At most one active execution lease drives a Turn. Lease expiry does not itself
grant replay authority; the new owner performs restart reconstruction. Session
version and unique identities reject split-brain commits.

Cancellation is durably requested, delivered through the frozen cancellation
port and checked at existing Core boundaries. If an external invocation is
Started, cancellation still follows its receipt/uncertainty contract. Runtime
cannot label an uncertain effect `Cancelled` merely because the caller left.

## Failure classes

Stable classes include `command_conflict`, `concurrent_modification`,
`turn_not_resumable`, `continuation_mismatch`, `definition_mismatch`,
`snapshot_mismatch`, `required_capability_unavailable`, `dispatch_uncertain`,
`effect_uncertain`, `durability_failure`, `corrupt_ledger`, and
`invariant_violation`. Diagnostics are redacted and are not compatibility keys.

## Language and storage evidence after approval

- Rust Runtime/SQLite is the production implementation and must pass the full
  process-kill matrix using a real database file;
- Kotlin/PostgreSQL remains an experimental portability host and must pass the
  admitted transaction/recovery subset against real PostgreSQL;
- C6 does not require source, SQL, driver or server-framework parity;
- shared semantic scenarios cover public continuation/outcome/recovery
  decisions, while each adapter proves its own transaction mechanics;
- protocol/provider fakes may prove orchestration but cannot prove official
  wire mapping; mocked storage cannot prove crash recovery.

## See also

- [`durable-runtime-facts.md`](durable-runtime-facts.md) — exact v1 payloads.
- [`governed-effects.md`](governed-effects.md) — effect recovery decisions.
- [`durable-ledger.md`](durable-ledger.md) — accepted L0 append/read semantics.
- [`host-api-v1.md`](host-api-v1.md) — current public Host wire contract.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
