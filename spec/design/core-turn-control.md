# Core Agent C0 — turn control

## Responsibility

Own the valid in-memory control states for one bounded Agent execution. It does
not persist a Session, call a model/tool, or decide crash recovery.

## Interface

`garive-core` exposes:

- `TurnId`: opaque, non-empty identity supplied by Runtime;
- `TurnLimits { max_iterations }`: immutable bound for one execution;
- `TurnState`: `turn_id`, completed iteration count, limits, and status;
- `TurnStatus`: `Running`, `Suspended(reason)`, or `Terminal(reason)`;
- transition methods: `begin_iteration`, `suspend`, `resume`, `terminate`.

`begin_iteration` returns either `Started { iteration }` or
`Terminated(BudgetExhausted)`. It is the only operation that increments the
iteration count.

## Status values

Suspension reasons in C0:

- `ApprovalRequired`;
- `PartialModelOutput`;
- `RateBudgetExhausted`.

Terminal reasons in C0:

- `Answered`;
- `NoMoreToolCalls`;
- `BudgetExhausted`;
- `Cancelled`;
- `ProviderUnavailable`;
- `Failed`;
- `OperatorRequired`.

Later slices may enrich payloads without changing the transition rules.

## Invariants

1. A new state is `Running` with zero completed iterations.
2. `max_iterations` is non-zero and cannot change during an execution.
3. Only a running turn can begin an iteration or suspend.
4. Only a suspended turn can resume.
5. Starting iteration `N` increments the counter exactly once and returns `N`.
6. When the counter equals the limit, the next `begin_iteration` atomically
   enters terminal `BudgetExhausted` without incrementing.
7. A terminal state is immutable: no begin, suspend, resume, or second terminal
   transition succeeds.
8. Suspend/resume never changes `turn_id`, limits, or iteration count.
9. `TurnState` is an in-memory control model, not a serialized recovery record.

## Errors

- empty `TurnId` → `InvalidTurnId`;
- operation requiring running status → `NotRunning`;
- resume while not suspended → `NotSuspended`;
- any mutation after terminal → `AlreadyTerminal`.

Errors do not partially mutate state.

## Acceptance tests

- construct rejects empty identity and zero iteration limit at the type level;
- first and last allowed iterations return monotonic one-based numbers;
- the call after the limit returns `BudgetExhausted` and preserves the count;
- suspend → resume preserves identity and count;
- invalid transitions leave the prior state unchanged;
- every terminal reason prevents all later transitions.

## Architecture record

This spec selects one status enum instead of separate `phase` and
`termination_reason` fields, because the latter permit contradictory values
such as `Running + Answered`. Runtime persists facts and derives a fresh
`TurnState` on resume; serialization is intentionally absent from C0.
