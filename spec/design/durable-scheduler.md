# Q0 — Durable scheduling intent and dispatch

## Status

Accepted implementation contract in the Agent capability set.

## Scope and ownership

Q0 defines portable scheduling intent, occurrence identity and deterministic
recurrence reduction. Engine Scheduler owns value validation and pure next-due
calculation. Runtime owns authenticated commands, wall clocks, persistence,
leases, workers, cancellation and C6 dispatch.

Q0 schedules product commands; it does not keep an Agent Kernel alive. No
in-memory timer, process uptime or exporter callback is durable scheduling
truth.

## Identities

- `ScheduleId`: one durable schedule;
- `ScheduleRevisionId`: immutable schedule semantics revision;
- `ScheduleCommandId`: idempotent create/update/cancel identity;
- `OccurrenceId`: deterministic identity for one due occurrence;
- `ScheduleLeaseId`: operational worker ownership token.

None substitutes for Session, Turn, Execution or C6 Runtime command identity.
An occurrence derives a stable C6 command identity from schedule ID, revision
and ordinal; lease retries therefore cannot create a second semantic command.

The occurrence preimage is L0-canonical JSON containing contract
`garive.schedule-occurrence`, version `1`, schedule ID, revision ID, ordinal and
declared due UTC instant. `OccurrenceId` is `occurrence-` plus its lowercase
SHA-256. The C6 command identity is `schedule-command-` plus SHA-256 of the same
preimage with contract `garive.schedule-command`. Lease/worker/observed-now
values are excluded.

## Portable schedule

```text
ScheduleIntent {
  schedule_id, revision_id
  subject: StartTurn | ContinueTurnResourceReady
  subject_binding_digest
  timing:
    At { due_at_utc } |
    FixedDelay { first_due_at_utc, delay_ms, max_occurrences? }
  misfire_policy: FireOnce | Skip | Fail
  max_lateness_ms: non-zero u64
  effective_limits_digest
}
```

The `intent` ContentBinding contains L0-canonical JSON with contract
`garive.schedule-intent`, version `1`, subject, subject binding digest, timing,
misfire policy, lateness and effective-limits digest. `intent_digest` is its
content digest. Schedule/revision identities remain in the outer created fact;
reusing either identity with a changed intent conflicts.

All timestamps are canonical RFC 3339 UTC instants. `delay_ms` is non-zero and
checked for overflow. V1 deliberately excludes cron, local time, daylight
saving transitions and calendar recurrence; those need an independently
versioned timezone/calendar contract.

`StartTurn` binds an exact installed Agent definition and trusted input content
reference. `ContinueTurnResourceReady` binds one current suspension and
expected Session version. It cannot represent approval, external user input or
operator reconciliation.

## Pure recurrence

```text
next_occurrence(intent, last_handled_ordinal?, observed_now_utc)
  -> NotDue | Due(ordinal, due_at, occurrence_id) |
     Skipped(first_ordinal, last_ordinal, first_due_at, last_due_at,
             next_due?) | Exhausted | Invalid
```

Fixed delay is anchored to the prior declared due instant, not worker finish
time, so restart and slow execution do not drift semantics. Arithmetic is
checked. `max_occurrences` bounds handled ordinals committed by
`schedule.fired` or `schedule.skipped` facts.
`At` has only ordinal 1. `FixedDelay` ordinal 1 is `first_due_at_utc`; ordinal
`n` is that instant plus `(n - 1) * delay_ms`, using checked arithmetic.

Misfire behavior when `now > due + max_lateness`:

- `FireOnce`: produce only the earliest uncommitted overdue occurrence;
- `Skip`: return one bounded contiguous skipped range plus the first future due,
  then atomically record that range before another reduction;
- `Fail`: commit `schedule.failed` and disable the revision.

Q0 never emits an unbounded catch-up burst.

## Durable lifecycle

```text
Created -> Claimed -> Fired
   |          |        `-> next occurrence remains Created
   +-> Skipped -> next occurrence remains Created
   +----------+-----> Cancelled | Failed | Exhausted
```

1. create/update commits `schedule.created` with exact intent digest;
2. a worker transaction acquires/renews a bounded lease for one occurrence;
3. `schedule.claimed` commits before dispatch and binds lease/occurrence;
4. Runtime submits the deterministic C6 command;
5. `schedule.fired` commits the C6 command identity and disposition;
6. `schedule.skipped` commits one deterministic contiguous misfire range;
7. losing a lease prevents further writes but does not imply dispatch absence;
8. restart reconstructs from facts and the C6 command receipt/replay result.
9. `schedule.exhausted` durably disables a revision after no occurrence remains.

Claiming is operational fencing, not a public success. A fired schedule means
the C6 command was durably committed or exactly replayed; it does not mean the
Turn completed successfully.

## Cancellation and update

Cancellation commits against an expected schedule revision. It prevents new
claims but cannot retract an already committed C6 command. Updating creates a
new revision and atomically supersedes the prior active revision; changing
timing or subject under the same revision is a conflict.

Schedule ownership and Session/Agent authorization are Runtime checks on every
mutation and dispatch. A stored schedule never preserves expired credentials
or grants; Runtime revalidates current authority before the C6 command.

## Durable facts

The coordinated C6F amendment must define:

- `schedule.created`: command, schedule/revision, canonical intent digest and
  exact content bindings;
- `schedule.claimed`: occurrence/ordinal/due time, lease identity/epoch and
  observed durable prefix;
- `schedule.fired`: occurrence, deterministic C6 command ID, commit/replay
  disposition and committed position;
- `schedule.skipped`: deterministic contiguous ordinal/due range and observed
  clock value;
- `schedule.cancelled`: command, expected revision and safe reason;
- `schedule.failed`: occurrence/revision and stable failure class.
- `schedule.exhausted`: revision and exact final handled ordinal.

Leases require adapter-owned expiry columns/transactions; lease heartbeats are
not portable durable facts.

## Stable failures

`invalid_schedule`, `schedule_not_found`, `revision_conflict`,
`subject_not_resumable`, `authority_denied`, `clock_invalid`,
`occurrence_overflow`, `misfire_limit_exceeded`, `lease_lost`,
`dispatch_conflict`, `durability_failure`, and `corrupt_schedule_state`.

## Acceptance evidence

- shared Rust/Kotlin intent/recurrence/misfire/failure fixtures;
- property tests for monotonic due instants, bounded catch-up and overflow;
- Rust SQLite real-clock fake plus restart and two-worker lease races;
- process-kill tests before claim, after claim, after C6 commit and before fire;
- cancellation/update conflict and authorization revalidation tests;
- Engine Scheduler imports no clock, Tokio, database, queue or worker library.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
