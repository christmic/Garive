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

`DispatchReadyStep` invokes PL1's bounded Step dispatch driver, not Q0 schedule
recurrence and not the Local Worker's terminal reducer. The coordinator owns
Ready-Step selection and fixed-prefix Goal/Plan authority; a constructed
preparation port owns installed Agent and execution-posture resolution; C6/PL1
planners validate the combined start before one atomic commit. Process-local
queue admission happens afterward and is never the authority that a Step
started.

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

An adopted Plan may activate only the exact Draft Goal revision it anchored.
That activation advances the Goal lifecycle revision, so later Plan work
compares the immutable definition digest and requires `current_revision >=
goal_revision_anchor`, not permanent revision equality. Suspend/resume may
advance lifecycle revision; Revise changes the definition digest and fences
the lineage. At most one Adopted/Running/Suspended Plan revision is
authoritative for a Goal; replacing it uses PL1's atomic supersede/adopt
command.

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

### Completed Turn reduction

For a Plan-owned Execution, the local worker performs one bounded reduction
after the atomic `execution.completed + turn.completed` commit:

1. re-derive the `ExecutionWorkBinding` from the committed Step/C6 start;
2. require one current Active Goal and one authoritative Running Plan with the
   same Goal definition digest;
3. prove the Turn is owned by the Plan's exact active Step claim and attempt;
4. require one `execution.completed` and one `turn.completed` for the exact
   Turn/Execution, in the same commit, with the same response digest;
5. re-observe every completion criterion declared by that Step from the
   current ledger prefix and run the Goal evidence verifier over that subset;
6. commit `plan.step.completed` using only the derived Step, attempt,
   Execution, result digest and evidence bindings.

The worker API accepts none of those reduction values from the model or
caller. A non-Plan Turn has no reduction. Suspension, stop and failure never
enter the completed reduction path. If the process crashes after the Turn
terminal but before Step completion, replay performs the missing reduction;
if the Step terminal already exists, replay is a no-op. A coordination error
does not roll back the already durable Turn terminal and is surfaced as a
stable worker failure for restart/reconciliation.

After a Step terminal, the same bounded driver re-evaluates the new prefix. If
unfinished Steps remain it stops. If every Step is Completed, it re-observes
the complete Goal evidence set, commits `plan.completed`, reopens the newer
prefix, independently verifies the completed Plan reduction, and commits
`goal.succeeded`. These are separate optimistic transactions so each crash gap
is recoverable: replay may add the missing Plan terminal, the missing Goal
terminal, or nothing, but never reruns the model merely to publish a terminal.

After `plan.completed` commits, the coordinator may propose Goal success only
from that unique completed Plan. It reads the Plan's verified reduction set,
re-observes the same durable references at the new current Session version and
runs the Goal verifier again. The client cannot provide or replace this set;
Plan completion evidence is not a boolean shortcut around Goal verification.

## Suspension and continuation

- A C5 interaction, operator reconciliation or supported external-input Turn
  suspension may produce `Goal Suspended` only with the exact committed
  suspension reference.
- Runtime first commits the matching `plan.step.suspended` and
  `plan.suspended`; Goal suspension requires that authoritative Plan posture.
- Public coordination accepts no step, attempt, Execution or continuation
  binding. Runtime derives all four through the exact Goal-owned Plan/Turn
  chain; the lower Plan composition functions are crate-private.
- Goal suspension does not invent a second continuation identity.
- The coordinator proves ownership through the authoritative Plan's exact
  `plan.step.started -> execution.started -> Turn` chain and reconstructs the
  typed suspended Turn. An Open owned Turn, zero resumable Turns or multiple
  resumable continuation identities fails closed; one Goal suspension fact
  cannot stand in for parallel live work.
- Continuing the Turn creates a fresh Execution under C6. The coordinator
  atomically resumes Plan/step state around that C6 continuation, then resumes
  the same Goal attempt only after the durable combined commit is visible.
- Initial activation from an authoritative Plan applies only to Draft. A
  Suspended Goal uses the separate continuation-derived resume path and cannot
  reuse initial activation as a bypass.
- Resource unavailability without a resumable durable reference stays a Turn
  outcome/policy decision; it is not automatically a Goal suspension.

## Failure and cancellation

Failure is explicit and typed. A failed step first applies PL1's frozen retry,
suspend, replan or fail posture. Goal failure occurs only after policy admits a
stable safe code and the coordinator proves no recovery/replan path remains.
Optional evidence is verified as canonical durable references; diagnostic text
is not terminal evidence.

For the V1 local worker, Runtime derives Step failure only from an atomic,
same-reason `execution.failed + turn.failed` or `execution.stopped +
turn.stopped` pair owned by the active Plan attempt. The canonical failure
evidence binds both fact IDs, payload digests, Turn and commit version.
`invalid_model_output`, `port_failure` and `resource_unavailable` request Retry;
PL1 admits it only while both Step and Plan attempt bounds remain. Invalid
input, missing capability, invariant failure, iteration/token/deadline stop, or
an exhausted retry records `retry_posture=fail`. Cancellation is excluded from
this path.

Retry is atomic with `plan.step.failed` and returns the Step to readiness; it
does not silently dispatch. A non-retryable Step first commits Failed, then a
dedicated fixed-prefix Runtime command commits `plan.failed` only when no
active claim remains. The coordinator derives `goal.failed` from that unique
failed Plan and its stable reason. Separate transactions make crash gaps
replayable without another model invocation.

Cancellation is product intent. Once `goal.cancelled` commits, the coordinator
must durably request cancellation of related live Turn/Execution work and stop
new claims. Already-started effects follow C5 uncertainty/reconciliation; a
terminal Goal does not erase or rewrite their facts.

Propagation walks only ledger-proven `Plan step -> Execution -> Turn`
ownership. It selects one still-open uncancelled Turn in stable identity order,
derives a command identity from the Goal cancellation fact and Turn, commits
`turn.cancel_requested`, then re-evaluates the new prefix. A crash therefore
resumes at the next missing Turn; terminal or already-requested Turns are not
rewritten, and no in-memory fan-out list is authoritative.

The Plan commit boundary revalidates the exact current Goal binding before any
Adopt/Resume/Claim/Start fact can commit. This fence also applies when a caller
reconstructs a Plan after cancellation; pure readiness alone is not authority.
Exact facts committed before cancellation remain replayable, while changed or
new work cannot use replay handling to cross the terminal Goal boundary.

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
- completed Plan-owned Turn but no Step terminal: re-derive and commit the
  exact Step reduction without another model call;
- completed Plan-owned Turn and Step terminal: treat worker replay as already
  terminal and do not duplicate the reduction;
- Plan completed but Goal open: rerun Goal evidence verification and close only
  when complete;
- Goal cancelled with live work: resume cancellation/reconciliation, never
  reopen the Goal.

No process-local queue or callback is authoritative for these decisions.

### Landed bounded coordinator slice

Runtime reconstructs the Goal and complete Plan graph at one shared watermark
before returning one `GoalPlanDecision`. It rejects mixed projection versions,
multiple authoritative non-terminal Plans and multiple terminal authorities.
It can currently advance the policy-independent Activate, Ready-Step dispatch,
Plan completion, Goal success and Goal failure decisions exactly once. Command
identities for non-worker decisions are derived from the frozen Goal/Session
prefix and transition class; every domain planner and commit boundary still
revalidates the prefix.

Ready-Step dispatch includes recovery of a declaration-ordered active claim
whose attempt has not started. Such a claim remains a dispatch decision so the
same worker can re-enter PL1 preparation after a crash using the claim identity,
epoch and clock revision reconstructed from the Ledger. An expired claim first
commits expiry and is replaced only on a later tick. A claim with a started
attempt yields no new dispatch decision; C6/Worker recovery owns it. Treating
every active claim as `NoAction` would strand the admitted
claim-before-preparation crash cut and is forbidden.

Desktop installs the coordinator, catalogue-backed preparation and local queue
consumer as one explicitly bounded `drive_goal` pump. Every iteration obtains
constructed worker/clock identities, evaluates at most one fixed-prefix
decision and consumes at most one queued Execution. The caller supplies a
positive bound capped at 64. Queue rejection after durable Step/C6 start closes
new work behind startup recovery; it is never reported as an uncommitted start.
The pump stops on no-action, claim ownership or an admitted policy boundary.

A Goal with no Plan requests `ProposePlan`. One exact proposed Plan produces an
`AdmitProposedPlan { plan_id, plan_revision }` decision. Without a constructed
admission-policy port it remains `AwaitingPolicy`; multiple proposals fail
closed as `ambiguous_plan_proposals`. Suspended work and failed Steps likewise
stop at explicit continuation/failure-policy reasons. Suspension continuation
and failed-Step retry/replan selection remain explicit policy gaps.

Plan admission may Adopt, Reject or Defer one exact proposal. Adopt records the
policy revision in `plan.adopted`; Reject records the same revision plus the
authenticated Runtime actor and a stable reason in `plan.rejected`. Both
commands re-read the complete Goal prefix before planning and again at commit.
No policy receives a write-capable Ledger, and no model-supplied policy identity
is authoritative. Runtime supplies the policy only a fixed-prefix read-only
Goal/Plan digest input, then independently reopens and commits the exact
proposal. Desktop accepts this policy only as an explicitly constructed Host
configuration value; absence is the default deny posture.

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
