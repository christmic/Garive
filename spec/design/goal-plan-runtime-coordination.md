# G2 — Goal/Plan Runtime coordination

## Status

Accepted foundation contract. G2 connects the existing Goal and Plan state
machines to the durable Agent worker. It is required before any adaptive or
self-evolution feature may consume Goal/Plan history.

## Problem

The Goal and Plan engines deliberately expose pure transitions, while C6 owns
Turn execution and durable effects. None of those layers may independently
guess when work starts, suspends or finishes. Runtime therefore needs one
ledger-driven coordinator that owns the cross-aggregate decisions without
moving policy, persistence or model behavior into the engines.

## Ownership and command classes

| Command class | Accepted source | Examples |
|---|---|---|
| Product intent | authenticated Host authority | Create, Revise, Cancel |
| Runtime coordination | coordinator over one verified ledger prefix | Propose, Adopt, Activate, Suspend, Complete/Fail Plan, Succeed/Fail Goal |
| Worker progress | fenced scheduler/worker | Claim, Start, Complete/Fail Step |

Product clients cannot submit Plan references, evidence verification results,
worker identities, leases, attempts or terminal Goal claims. An internal
Runtime caller cannot bypass the same fixed-prefix validation merely because
it is in-process.

## Layering

```text
client intent
    |
    v
Host command authority -----> Goal Create / Revise / Cancel
                                  |
                                  v
                         verified ledger prefix
                                  |
                                  v
                    GoalPlanCoordinator (Runtime)
                       |       |          |
                       v       v          v
                    Goal    Plan DAG    Scheduler/Worker
                     facts    facts       C6/F0 facts
                       \       |          /
                        `------v---------'
                         one Session ledger
```

`engine/goal` and `engine/plan` remain storage-free pure domains. Host remains
transport and product-intent admission. The coordinator is Runtime code and
depends on the domains and ledger; domains never depend on it.

## Coordinator input

Every evaluation freezes one `CoordinationSnapshotV1`:

```text
session_id
through_position
session_version
goal_id, goal_revision, goal_definition_digest, goal_state
authoritative_plan?: plan_id, revision, definition_digest, state, state_version
active_claims and started attempts
open Turn/Execution/suspension bindings
verified criterion evidence available through_position
installed Agent snapshot and admitted policy revisions
```

All values are reconstructed from the same committed Session prefix. Missing,
future, corrupt or mutually inconsistent bindings fail closed. Evaluation
never combines a Goal projection at one watermark with a Plan or Turn
projection from another.

## Deterministic decisions

The coordinator emits exactly one of these bounded decisions:

```text
NoAction
ProposePlan
AdoptPlan
ActivateGoal
DispatchReadyStep
SuspendGoal
CompletePlan
SucceedGoal
FailPlan
FailGoal
NeedsOperator
```

The decision is a proposal, not a mutation. A separate commit step reruns the
relevant domain planner with exact expected Session/Goal/Plan versions. A lost
optimistic race discards the proposal and evaluates the new durable prefix.

Priority is fixed:

1. corruption, uncertain effects and operator reconciliation block progress;
2. authenticated Cancel/Revise already committed by Host takes precedence;
3. recover or terminalize existing started work;
4. suspend when an open resumable Turn requires external input;
5. complete verified Plan and Goal terminals;
6. dispatch already-ready steps;
7. adopt an admitted proposal;
8. request/propose a Plan;
9. otherwise `NoAction`.

Policy may deny a proposed edge but cannot reorder safety, recovery or
terminal-evidence checks.

## Plan and Goal binding

The default local product requires one authoritative adopted Plan before Goal
activation or an effectful Turn. Direct work is an explicit policy capability,
never inferred from absence of a Plan.

Runtime derives references as RFC 8785 canonical JSON from committed facts:

```json
{"definition_digest":"...","goal_id":"...","revision":1}
{"definition_digest":"...","plan_id":"...","revision":1}
```

These are the same values used by the atomic `plan.step.started` plus C6 start
binding. Host bodies, model output and tool arguments cannot supply or override
them.

An adopted Plan may activate only the exact non-terminal Goal revision it
binds. Goal revision makes older proposals stale. At most one
Adopted/Running/Suspended Plan revision is authoritative for a Goal; replacing
it uses PL1's atomic supersede/adopt command.

## Normal path

```text
Goal Draft
  -> Plan Proposed
  -> Plan Adopted
  -> Goal Active (Runtime-derived Plan reference)
  -> Step Claim
  -> atomic Step Start + C6 Execution binding
  -> Turn/effect facts
  -> Step Completed or Failed
  -> ...
  -> Plan Completed with reduction evidence
  -> Goal Succeeded with independently verified criterion evidence
```

Plan completion does not imply Goal success. The Goal verifier independently
resolves every declared criterion against the fixed ledger prefix. Conversely,
a completed Turn does not imply a completed step, Plan or Goal.

## Suspension and continuation

- A C5 interaction, operator reconciliation or supported external-input Turn
  suspension may produce `Goal Suspended` only with the exact committed
  suspension reference.
- Goal suspension does not invent a second continuation identity.
- Continuing the Turn creates a fresh Execution under C6. The coordinator
  resumes the same Goal attempt and authoritative Plan only after the durable
  continuation commit is visible.
- Resource unavailability without a resumable durable reference stays a Turn
  outcome/policy decision; it is not automatically a Goal suspension.

## Failure and cancellation

Failure is explicit and typed. A failed step first applies PL1's frozen retry,
suspend, replan or fail posture. Goal failure occurs only after policy admits a
stable safe code and the coordinator proves no recovery/replan path remains.
Optional evidence is verified as canonical durable references; diagnostic text
is not terminal evidence.

Cancellation is product intent. Once `goal.cancelled` commits, the coordinator
must durably request cancellation of related live Turn/Execution work and stop
new claims. Already-started effects follow C5 uncertainty/reconciliation; a
terminal Goal does not erase or rewrite their facts.

## Idempotency and crash recovery

Every coordinator mutation uses a deterministic command identity derived from
the admitted trigger fact plus target aggregate/revision and transition kind.
Exact retry returns the original commit; changed semantics conflict.

Recovery cases:

- Plan adopted but Goal not activated: re-evaluate and issue the same derived
  Activate command;
- Goal active but no claim: dispatch only a currently Ready step;
- claim committed but not started: PL1 lease recovery decides expiry;
- start committed: C6/C5 recovery decides completion, suspension or
  uncertainty before another attempt;
- Plan completed but Goal open: rerun Goal evidence verification and close only
  when complete;
- Goal cancelled with live work: resume cancellation/reconciliation, never
  reopen the Goal.

No process-local queue or callback is authoritative for these decisions.

## Public surface

V1 public Goal mutation remains limited to Create, Revise and Cancel. Clients
observe Goal/Plan projections and durable activities. Activate, Suspend,
Succeed and Fail are Runtime coordination commands and are not added as generic
public HTTP endpoints. Explicit operator reconciliation continues through the
typed Turn continuation API rather than a Goal-state escape hatch.

## Acceptance evidence

G2 is implemented only when tests prove:

1. one real loopback product intent reaches Plan adoption, Goal activation,
   fenced Step/C6 execution, Plan completion and evidence-verified Goal success;
2. every crash cut above reconstructs from SQLite and produces no duplicate
   transition or effect;
3. stale Session/Goal/Plan versions lose without partial commits;
4. caller-supplied Goal/Plan references are absent or rejected at every public
   and worker boundary;
5. suspension and cancellation stop new work and preserve uncertain-effect
   reconciliation;
6. Kotlin validates the portable Goal/Plan/L0 facts and shared scenarios while
   Rust alone proves concrete SQLite/worker behavior.

Until this evidence is green, G1/PL1 remain partial and self-evolution remains
blocked from using them as an authority source.

