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

Proposal and adoption are authoritative Runtime operations over one fixed
Session prefix. Runtime reconstructs the complete Goal graph and requires the
bound Goal ID, current revision and definition digest to match, the Goal to be
non-terminal, and the number of proposed revisions to remain within
`max_plan_revisions`. Exact proposal-command replay is admitted even at the
bound; reuse of the same Plan identity/revision with another command or digest
is `plan_command_conflict`. Rejection is also a fixed-prefix Runtime operation
and records the authenticated actor, exact admission-policy revision and a
stable secret-free reason. The general pure transition entry point rejects
Adopt, Reject and Step Start so none can bypass the ledger check.

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

`goal_revision` is the optimistic-concurrency anchor observed when this Plan
lineage was proposed/adopted; it is not a demand that the Goal lifecycle stop
advancing. Proposal and adoption require exact revision equality. After the
Plan activates that Goal, Plan work requires the Goal to be Active, its current
revision to be at least the anchor and its definition digest to remain exact.
Suspend/resume may therefore advance Goal revisions without invalidating the
Plan, while Goal revision through `Revise` changes the definition digest and
fences the old Plan. Terminal Goal states always fence new Plan work.

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

`CompletePlan` is not admitted through the generic pure Runtime transition
entry. Runtime must reconstruct the current Goal and Plan at one Session
watermark, verify the complete typed Goal evidence set against that prefix,
and only then plan `plan.completed`. Recovery repeats the verification against
the exact prefix preceding the completion commit; a valid content binding with
incomplete, future or tampered evidence is corrupt state.

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

The generic Runtime transition entry rejects `FailPlan`. The dedicated Plan
failure command rechecks the same active Goal binding and ledger watermark,
requires at least one Failed Step and no active claims, and then emits
`plan.failed`. This prevents a client, model or generic in-process caller from
terminalizing recoverable or still-owned work.

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

### Bounded Step dispatch driver

Plan dispatch is separate from Q0 recurring schedules. One Runtime tick makes
at most one claim and one start decision:

1. reconstruct Goal and every Plan at one Session watermark;
2. select the unique authoritative Adopted/Running Plan and the first Ready
   Step in Plan declaration order, subject to `max_parallel_ready`;
3. commit `plan.step.claimed` before any model or execution dispatch;
4. pass the exact claimed Step, Plan/Goal bindings and current prefix to a
   constructed start-preparation port;
5. validate the returned installed Agent binding, C6 start batch, frozen
   execution snapshot, exact Tool catalogue digest and exact Safety policy
   revision;
6. atomically commit C6 `turn.started + turn.input + execution.started` with
   `plan.step.started`;
7. only after a new commit, offer the derived `CommittedTurn` to the bounded
   local dispatch queue.

The preparation port cannot choose another Goal, Plan, Step, claim, lease,
attempt, command or Session. It may only resolve the installed Agent/C6 and
execution-posture values that are not owned by the portable Plan. Configuration
is constructor input; no environment discovery occurs.

The local catalogue-backed implementation opens the explicitly constructed
Ledger path, reads the Session's first `session.opened` fact and resolves that
exact definition revision and snapshot in the immutable Runtime Agent
catalogue. It does not fall back to the default Agent, scan configuration or
consult process environment. A missing, ambiguous or mismatched installation
fails closed before C6 start.

Runtime compares the installed Tool catalogue digest and Safety policy revision
returned by preparation with the immutable values frozen in the Plan. Matching
the Session's Agent snapshot digest alone is insufficient; any one of these
three bindings differing fails before C6 or `plan.step.started` can commit.

Step Start precedes model output and any concrete Tool intent. It therefore
must not invent or freeze a Safety decision identity or Prepared-v3 Sandbox
profile. Those values exist only after Runtime prepares an exact Tool call;
F0 durably records them as `safety.decided`, `sandbox.bound` and
`sandbox.preflighted` before effect dispatch. The Plan-owned Execution binding
is later injected into that real F0 request as Goal/Plan references.

A queue-admission failure cannot roll back the start transaction. Startup
recovery re-discovers the durable open Execution. A crash after claim but
before start leaves a fenced claim. A later tick reuses the claim identity,
epoch and clock revision reconstructed from the Ledger rather than requiring a
process-local replay token. The same worker may start it before expiry; after
expiry, a compatible monotonic observation first commits
`plan.step.claim_expired`, and only a later tick may create a replacement
claim. A tick never silently steals a different worker's live claim or starts a
second Execution for an already-started attempt.

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

Runtime derives, rather than accepts, the canonical carry-forward document at
one fixed Session watermark. Before derivation it revalidates the target
Plan's exact non-terminal Goal ID/revision/digest at that watermark. The
replacement commit uses that same expected Session version, so a later Goal
change makes the replacement conflict instead of adopting stale work. Records
are in target step declaration order:

```text
CarryForwardRecordV1 {
  step_id, step_digest, result_digest
  dependency_results: ordered [{step_id, result_digest}]
  step_evidence_digest, criterion_evidence_digest
  terminal_fact_id, terminal_position, terminal_commit_version
}
```

Every digest is re-read from the source revision's unique completed-step fact;
the terminal commit version and position must be inside the verified watermark.
The maximal admitted set is dependency-closed and includes only equal old/new
step digests. Initial adoption accepts only an empty document. Replacement
adoption is one SQLite transaction ordered as old `plan.superseded` then new
`plan.adopted`; both facts share one command and commit version. Recovery
rejects a missing/mismatched counterpart, proposal digest or malformed record.
It also re-derives both source and target step digests, preserves target
declaration order, and resolves each terminal/evidence/dependency result back
to exactly one source completion no later than the replacement commit. A
changed content binding fails even when the enclosing Ledger payload remains
canonical.

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
| `plan.rejected` | old/new state versions, authenticated actor, exact admission-policy revision and stable safe reason |
| `plan.superseded` | old/new state versions, replacement Plan/revision/digest and canonical unresolved-work binding |
| `plan.step.claimed` | old/new state versions, step/digest, claim, worker, positive lease epoch, monotonic clock revision and `[claimed_at_tick, expires_at_tick)` |
| `plan.step.claim_expired` | old/new state versions, step/claim/lease epoch and observed monotonic expiry tick |
| `plan.step.started` | old/new state versions, step/claim/lease epoch, same monotonic clock revision plus pre-expiry observed tick, attempt and Kernel Execution/snapshot binding |
| `plan.step.completed` | old/new state versions, step/attempt/Execution, result digest and canonical step/criterion evidence bindings |
| `plan.step.failed` | old/new state versions, step/attempt/Execution, stable reason, optional evidence and closed retry posture |
| `plan.step.suspended` | old/new state versions, step/attempt/Execution and typed continuation reference |
| `plan.step.resumed` | old/new state versions, step/attempt, prior/fresh Execution and resolved continuation reference |
| `plan.suspended` | old/new state versions and typed Plan-level continuation reference |
| `plan.resumed` | old/new state versions and resolved continuation reference |
| `plan.completed` | old/new state versions and canonical complete reduction evidence |
| `plan.failed` | old/new state versions, stable terminal reason and optional canonical evidence |

For worker-derived Step completion, `step_evidence` is the canonical V1
document below. It binds the reduction to the exact Turn terminal observed at
the fixed prefix:

```json
{
  "contract": "garive.plan-step-evidence",
  "version": 1,
  "terminal_commit_version": 12,
  "terminal_fact_id": "...",
  "terminal_payload_digest": "...",
  "terminal_position": 34,
  "turn_id": "..."
}
```

`terminal_fact_id` identifies the unique `turn.completed`. Runtime also
requires the matching `execution.completed` to share its commit version and
response digest. `result_digest` is that response digest. `criterion_evidence`
is the canonical ordered `GoalEvidenceV1[]` for exactly the Goal criteria named
by the Step; Runtime observes and verifies it rather than accepting it from a
worker, model or client.

Every payload carries `command_id`, `plan_id` and `plan_revision`. Every
non-proposal fact carries contiguous old/new state versions. Digests are
lowercase SHA-256; content uses the L0 exact inline-or-reference binding. Lease
ticks are non-negative integers from one named monotonic clock revision and
`expires_at_tick` must be greater than `claimed_at_tick`. `retry_posture` is
`retry | suspend | replan | fail`. Continuation kind is `interaction |
reconciliation`; it never embeds user text or credentials.

Worker-derived failure evidence uses contract
`garive.plan-step-failure-evidence` V1 and contains the exact execution/Turn
terminal fact IDs and payload digests, their shared commit version, and Turn
ID. Runtime derives the stable reason as `<failed|stopped>_<core_reason>` and
derives retry posture from the frozen V1 classification plus current attempt
bounds; neither value is accepted from the worker caller.

Command receipt and corresponding state facts commit atomically. Claims use
monotonic lease readings; durable facts retain portable recorded-at values
only for audit. Projections validate topology/digest before applying progress.
The public Runtime transition planner rejects a standalone Step Start. Runtime
must compose the Plan mutation with the C6 Turn/Execution start batch; recovery
requires their persisted SQLite commit versions, command identity, Turn,
Execution and snapshot digest to match before accepting the prefix.
For continuation, `plan.resumed`, the C6 `turn.started(kind=continue)` plus
fresh `execution.started`, and `plan.step.resumed` share one SQLite commit.
The suspended claim/attempt remains fenced and is rebound to that fresh
Execution; no unowned continuation Execution may appear between Plan facts.

Runtime derives an `ExecutionWorkBinding` from that atomic prefix. Goal and
Plan references are canonical JSON strings containing exact identity, revision
and definition digest; no model, App, worker factory or policy adapter supplies
them. The local worker injects the derived pair into F0, restart recovery
re-derives it, and `SqliteGovernedEffectPort` independently refuses missing,
extra or changed references before `effect.prepared`. An Execution without a
`plan.step.started` owner must carry neither reference.

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

The Session Plan graph is recovered at one ledger watermark. Each proposal is
first joined to the Goal graph at the proposal's own commit prefix; Runtime
then derives the exact criterion and capability sets from that historical Goal
revision, revalidates the canonical Plan definition, and replays all progress
through the common Session watermark. Duplicate proposals, orphan Plan facts,
unknown revisions, malformed coordinates or a changed Goal binding make the
entire graph corrupt. Host projections consume only this verified graph.

`GET /v1/sessions/{session_id}/plans` returns that graph in stable Plan-ID and
revision order at the same Session watermark. It exposes only lifecycle state,
definition digest, Goal binding, state version and aggregate step/attempt
counts. Step objectives, capability references, input bindings, evidence,
claims, worker references and policy internals are never public fields. Plan
count, scanned facts, safe integers and encoded response bytes fail closed.

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
- stale/terminal Goal, wrong digest/revision, proposal replay and Plan-revision
  bound tests over a real fixed SQLite prefix.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
