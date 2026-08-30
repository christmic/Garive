# PL1 — Durable executable Plan lifecycle

## Status

Accepted implementation contract. PL1 defines Goal-bound work topology and
step progress; it does not replace C5b effect batching, Scheduler recurrence or
the Core iteration loop.

## Ownership

`engine/plan` owns validation, canonical digest, DAG readiness and pure
transition semantics. Runtime owns identities, proposal/adoption authority,
step claims, leases, worker selection, persistence, clocks and recovery. Core
may propose a Plan or replan request and work on one claimed step. Kotlin
implements portable values/reducers only.

## Identity and bindings

`PlanId`, `PlanRevision`, `PlanCommandId`, `PlanStepId`, `StepClaimId` and
`StepAttemptId` are distinct opaque values. Plan revision is a positive
interoperable integer and starts at 1.

Every Plan revision binds exactly:

- one `GoalId`, `GoalRevision` and Goal definition digest;
- one Agent Definition/snapshot digest;
- one Tool catalogue digest;
- one safety-policy revision;
- one non-zero set of Plan bounds.

Any binding change requires a new Plan revision. A model-generated title,
Markdown list or tool-call list is untrusted proposal content until the exact
portable Plan validates and Runtime adopts it.

## Definition

```text
PlanDefinitionV1 {
  plan_id, plan_revision
  goal_id, goal_revision, goal_definition_digest
  agent_snapshot_digest
  tool_catalogue_digest
  safety_policy_revision
  steps: non-empty ordered unique PlanStepV1
  bounds: PlanBoundsV1
}

PlanStepV1 {
  step_id
  objective: non-empty bounded UTF-8
  depends_on: canonical unique PlanStepId set
  completion_criteria: non-empty canonical unique GoalCriterionId set
  required_capabilities: canonical unique exact references
  input_bindings: canonical unique content/fact digests
  max_attempts: non-zero u32
}

PlanBoundsV1 {
  max_steps, max_parallel_ready, max_total_attempts: non-zero u32
  token_budget?, duration_budget_ms?: optional non-zero u64
}
```

Step order is presentation/tie-break order. Dependencies determine readiness.
Every dependency must exist in the same revision, cannot reference itself and
the graph must be acyclic. Every Goal success criterion must be covered by at
least one step unless it was already satisfied by verified evidence at
adoption. A step cannot request a capability absent from the Goal and frozen
Agent snapshot.

V1 `max_parallel_ready` affects claim admission only. It does not permit Core
iterations or effects to execute concurrently; C5/C5b rules still decide tool
dispatch.

## Canonical digest

`plan_digest` is lowercase SHA-256 over RFC 8785 JSON for the complete
definition with this fixed prefix and no digest field:

```json
{
  "contract": "garive.plan-definition",
  "version": 1,
  "plan_id": "...",
  "plan_revision": 1,
  "goal_id": "...",
  "goal_revision": 1,
  "goal_definition_digest": "...",
  "agent_snapshot_digest": "...",
  "tool_catalogue_digest": "...",
  "safety_policy_revision": "...",
  "steps": [],
  "bounds": {}
}
```

Step array order is semantic. Dependency, criterion, capability and input
arrays are canonical. State, claims, attempts, clocks and Runtime actor data
are excluded. `step_digest` uses the same canonical step value plus the Plan
bindings required to prevent cross-revision reuse.

## Plan state

```text
Proposed -> Adopted -> Running -> Completed
               |         |------> Suspended -> Running
               |         |------> Failed
               `---------------> Superseded
Proposed ----------------------> Rejected
```

`PlanState = Proposed | Adopted | Running | Suspended | Completed | Failed |
Superseded | Rejected`. Completed, Failed, Superseded and Rejected are
terminal for that revision.

At most one adopted/running/suspended Plan revision exists for a Goal. Adoption
uses exact expected Goal and prior-Plan revisions. Starting the first step moves
Adopted to Running in the same transaction as its durable claim/start posture.
Plan Completed requires every step Completed and complete Goal-criterion
evidence; it does not itself close the Goal.

## Step state and readiness

```text
Pending -> Ready -> Claimed -> Running -> Completed
                    |           |------> Failed
                    |           `------> Suspended -> Ready
                    `------------------> Ready (expired before start)
```

`StepState = Pending | Ready | Claimed | Running | Suspended | Completed |
Failed`. Readiness is a pure projection:

- the Plan is Adopted, Running or resumable Suspended;
- the step is Pending or resumable Suspended;
- every dependency is Completed with verified terminal evidence;
- no hard Plan/step/Goal bound is exhausted;
- current claimed/Running count is below `max_parallel_ready`.

A failed step does not automatically fail the Plan. Frozen policy chooses a
bounded retry, suspension, replan request or explicit Plan failure. An
attempt-limit breach cannot return the step to Ready.

## Claims and attempts

Runtime claims one Ready step using expected Plan revision, state version,
worker identity and a non-zero lease. Claim commit precedes worker dispatch.
Only the current unexpired claim may create a `StepAttemptId` and start a
Kernel Execution. A lease is coordination authority, not effect authority;
every tool still passes C4/F0/C5.

Expired Claim with no started attempt may return to Ready using a fenced
recovery transaction. Once a Kernel Execution or effect has started, recovery
first classifies C6/C5 durable positions. Another worker cannot start merely
because wall-clock lease time passed.

## Replanning and carry-forward

Replanning proposes revision `N + 1` with the same Plan ID and current Goal
revision, or a new Plan ID when policy explicitly chooses replacement. Runtime
validates and adopts the new revision while atomically superseding the old one.

Completed step evidence may be carried forward only when:

1. old and new `step_digest` are equal;
2. each dependency terminal/result digest is equal;
3. each input binding and Goal criterion binding is equal;
4. evidence is still present and valid at the adoption commit version.

Claimed, Running, Suspended, Failed or uncertain work is never marked Completed
by carry-forward. A started uncertain effect is reconciled under its original
Plan/step/invocation even if a new Plan is adopted.

## Runtime facts

Plan revision is immutable definition identity. Runtime additionally maintains
a positive contiguous `state_version` for each `(plan_id, plan_revision)`:
`plan.proposed` creates state version 1 and every admitted lifecycle, claim or
attempt mutation records `previous_state_version` and exactly the next
`state_version`. This prevents definition revision and mutable progress
concurrency from being conflated.

All PL1 facts are Session-scoped L0 facts: outer Turn, Execution, model request
and tool invocation identities are absent. A step-start fact binds the C6
Execution in its payload because the Plan remains cross-Turn. Runtime commits
the Plan mutation and any corresponding C6 posture atomically.

| Fact | Required payload |
|---|---|
| `plan.proposed` | command, Plan/revision, state version 1, definition digest/content, Goal and frozen Agent/Tool/Safety bindings, proposer reference |
| `plan.adopted` | old/new state versions, expected Goal/prior-Plan revisions, actor/policy reference, canonical carry-forward evidence binding |
| `plan.rejected` | old/new state versions and stable safe reason |
| `plan.superseded` | old/new state versions, replacement Plan/revision/digest and canonical unresolved-work binding |
| `plan.step.claimed` | old/new state versions, step/digest, claim, worker, positive lease epoch, monotonic clock revision and `[claimed_at_tick, expires_at_tick)` |
| `plan.step.claim_expired` | old/new state versions, step/claim/lease epoch and observed monotonic expiry tick |
| `plan.step.started` | old/new state versions, step/claim/lease epoch, attempt, Kernel Execution/snapshot, Prepared-v3 Sandbox profile and Safety decision bindings |
| `plan.step.completed` | old/new state versions, step/attempt/Execution, result digest and canonical step/criterion evidence bindings |
| `plan.step.failed` | old/new state versions, step/attempt/Execution, stable reason, optional evidence and closed retry posture |
| `plan.step.suspended` | old/new state versions, step/attempt/Execution and typed continuation reference |
| `plan.step.resumed` | old/new state versions, step and resolved continuation reference |
| `plan.suspended` | old/new state versions and typed Plan-level continuation reference |
| `plan.resumed` | old/new state versions and resolved continuation reference |
| `plan.completed` | old/new state versions and canonical complete reduction evidence |
| `plan.failed` | old/new state versions, stable terminal reason and optional canonical evidence |

Every payload carries `command_id`, `plan_id` and `plan_revision`. Every
non-proposal fact carries contiguous old/new state versions. Digests are
lowercase SHA-256; content uses the L0 exact inline-or-reference binding. Lease
ticks are non-negative integers from one named monotonic clock revision and
`expires_at_tick` must be greater than `claimed_at_tick`. `retry_posture` is
`retry | suspend | replan | fail`. Continuation kind is `interaction |
reconciliation`; it never embeds user text or credentials.

Command receipt and corresponding state facts commit atomically. Claims use
monotonic lease readings; durable facts retain portable recorded-at values
only for audit. Projections validate topology/digest before applying progress.

## Recovery

| Position | Decision |
|---|---|
| proposal committed, not adopted | expose Proposed; never execute it |
| adopted, no claim | recompute readiness from facts |
| claim committed, no start | fence/reclaim after proven lease expiry |
| attempt started | reconstruct C6 and every C5 invocation position |
| receipt/result, no step terminal | reconstruct exact step terminal evidence |
| step terminal, no Plan terminal/publication | rerun pure reduction; do not execute |
| supersede transaction incomplete | old Plan remains authoritative |

## Stable failures

`plan_invalid`, `plan_cycle`, `plan_command_conflict`,
`plan_revision_conflict`, `plan_binding_stale`, `plan_transition_invalid`,
`step_not_ready`, `step_claim_conflict`, `step_claim_stale`,
`step_attempt_conflict`, `step_evidence_conflict`, `plan_bound_exceeded`, and
`plan_recovery_corrupt` are compatibility codes.

## Acceptance evidence

- shared Rust/Kotlin fixture for canonical topology/digest, cycles, unknown
  dependencies, readiness, transitions, retry limits and carry-forward;
- properties for deterministic topological readiness, terminal closure,
  monotonic revision and no evidence invention;
- L0 payload and real SQLite projection/migration tests;
- competing claim/adoption races and fake-monotonic-clock lease tests;
- process-restart fault injection at every recovery row;
- one Runtime integration binding a step to C6/F0/C5 without bypass paths.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
