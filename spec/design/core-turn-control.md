# C0 — Kernel execution control

## Status

Accepted focused contract under `agent-architecture.md` and
`agent-execution-contract.md`.

## Responsibility

Represent valid disposable control state for one Kernel Execution. Runtime
owns the durable Turn and constructs a new control after suspension.

## Types

- `TurnId`: non-empty durable Turn identity supplied by Runtime.
- `ExecutionId`: non-empty identity unique to this Kernel invocation.
- `ExecutionLimits { max_iterations: NonZeroU32 }`.
- `ExecutionOutcomeKind`: `Completed`, `Suspended`, `Stopped`, `Failed`.
- `ExecutionStatus`: `Active` or `Closed(ExecutionOutcomeKind)`.
- `ExecutionControl`: identities, completed iteration count, limits and status.

Construction accepts a Runtime-reconstructed `completed_iterations` cursor.
It rejects a cursor greater than `max_iterations`. Equality is valid but the
next `begin_iteration` closes as stopped by iteration limit.

## Operations

`begin_iteration()`:

- requires `Active`;
- if the count is below the limit, increments exactly once and returns the new
  one-based cumulative iteration number;
- if the count equals the limit, closes as `Stopped` and returns
  `IterationLimitReached` without incrementing.

`close(kind)` requires `Active`, closes exactly once, and preserves identities,
limits and count. There is no `resume()` or `suspend()` mutation. Suspension is
an execution outcome; continuation is a new `ExecutionControl` with the same
Turn ID, a new Execution ID and a reconstructed cursor.

## Errors

- empty identity → identity-specific validation error;
- cursor greater than limit → `CursorBeyondLimit`;
- any operation after close → `AlreadyClosed`.

Errors do not partially mutate control state.

## Invariants

1. Turn and Execution identities have distinct types and cannot substitute.
2. The iteration limit is non-zero and immutable.
3. The counter never exceeds the limit and increments only in
   `begin_iteration`.
4. Closing is monotonic and occurs once.
5. Continuation never reuses an Execution ID or an old in-memory control.
6. C0 has no persistence/serialization contract; shared JSON fixtures are test
   protocol only.

## Shared semantic fixture

`spec/fixtures/agent/execution-control.json` contains operation sequences. Rust
and Kotlin must consume every case and compare operation results plus final
status/count. A continuation pair demonstrates same Turn ID, new Execution ID,
and carried durable count without a resume operation.

## Acceptance

- fresh and reconstructed cursors behave identically for the same remaining
  limit;
- last admitted iteration starts once; the next begin stops without overcount;
- all four close kinds are immutable;
- invalid cursor and post-close operations preserve prior state;
- Rust and Kotlin native and shared-fixture tests pass.
