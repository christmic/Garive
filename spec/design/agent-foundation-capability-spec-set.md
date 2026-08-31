# Agent foundation capability Spec set

> Traceability and delivery contract for Sandbox, Safety, Goal, Plan and the
> built-in tool baseline.

## Status

Accepted implementation set. The product owner authorized design-to-Spec and
implementation work on 2026-08-31. A child slice is complete only with its
declared fixture, native implementation and verification evidence.

## Boundary decision

This set extends the accepted Agent architecture; it does not replace C4, C5,
C5b, C6, L0 or MA0.

| Concern | Portable owner | Production owner | Explicit non-owner |
|---|---|---|---|
| sandbox requirements | `engine/tools` and `engine/config` | Runtime executor binding | Core |
| safety decision semantics | neutral request/result values | Runtime policy and authenticated authority | model/tool |
| Goal values/transitions | `engine/goal` | Runtime identity, command handling, Ledger | client/Memory |
| Plan values/transitions | `engine/plan` | Runtime adoption, claims, worker recovery | model prose/C5b |
| built-in definitions | `engine/tools` | Runtime concrete executors | Agent Definition alone |

`GoalId`, `GoalRevision`, `PlanId`, `PlanRevision`, `PlanStepId`,
`StepClaimId`, `ToolInvocationId`, `ExecutionId` and C5b plan digest are typed,
non-interchangeable identities.

## Normative slices

| Order | ID | Contract | Depends on | Completion evidence |
|---:|---|---|---|---|
| 1 | F0 | [`sandbox-safety.md`](sandbox-safety.md) | C4/C5/C5b/C6 | shared portable fixture; Runtime preflight/receipt/fault tests |
| 2 | G1 | [`goal-lifecycle.md`](goal-lifecycle.md) | D0/L0/C6 | shared lifecycle/digest fixture; SQLite restart/race tests |
| 3 | PL1 | [`plan-lifecycle.md`](plan-lifecycle.md) | G1/D0/C5/L0 | shared DAG/revision fixture; claim/replan/recovery tests |
| 4 | T1 | [`basic-tools.md`](basic-tools.md) | F0/C4/C5b | catalogue fixtures and real confined executor tests |
| 5 | T2 | [`native-browser-computer-use.md`](native-browser-computer-use.md) | F0/C5/T1 | browser contract suite and native packaged-adapter tests |
| 5a | T2 Attached | [`browser-attached-adapter.md`](browser-attached-adapter.md) | T2 | Native Messaging protocol, extension and explicit-tab grant tests |
| 6 | F1 | Runtime composition and client projection amendment | F0/G1/PL1/T1/T2/H2/H3 | real local end-to-end and restart flow |

The referenced slice Specs must be accepted before their behavior is claimed.
This index fixes their ownership, dependency order and evidence floor.

## End-to-end state flow

```text
goal command --atomic facts--> Goal projection
       |
       v
plan proposal --validate/digest--> adopted Plan revision
       |
       v
ready step --Runtime claim--> bounded Kernel Execution
       |
       v
ToolIntent --C4--> Prepared Call --F0/C5--> authorized sandbox dispatch
       |
       v
receipt/result --atomic facts--> step terminal -> Plan/Goal reduction
```

Plan adoption does not authorize a tool. Step claim does not authorize a tool.
Only C5 authorization for the exact Prepared Call can reach an executor.

## Shared fixture ownership

| Fixture | Required semantics | Consumers |
|---|---|---|
| `spec/fixtures/agent/sandbox-safety-v1.json` | requirement normalization, coverage, decision binding, stable failures | Rust Tools/config, Kotlin Tools/config |
| `spec/fixtures/agent/goal-lifecycle-v1.json` | identity/revision validation, criteria, transitions, evidence, conflicts | Rust/Kotlin Goal |
| `spec/fixtures/agent/plan-lifecycle-v1.json` | topology, canonical digest, readiness, carry-forward and transition failures | Rust/Kotlin Plan |
| `spec/fixtures/agent/basic-tools-v1.json` | definitions, schemas, access resolution, canonical ordering, limits | Rust/Kotlin Tools |
| `spec/fixtures/agent/native-control-v1.json` | snapshots, stale references, action bindings, sensitivity and failures | Rust/Kotlin Tools |

Fixture equality is canonical only for bytes explicitly named by the child
Spec. Runtime sandbox behavior is capability evidence and cannot be proved by
two pure implementations agreeing.

## Ledger amendments

The implementation adds versioned Runtime fact payloads, not generic JSON
events:

- `goal.created`, `goal.revised`, `goal.activated`, `goal.suspended`,
  `goal.succeeded`, `goal.failed`, `goal.cancelled`;
- `plan.proposed`, `plan.adopted`, `plan.superseded`, `plan.step.claimed`,
  `plan.step.started`, `plan.step.completed`, `plan.step.failed`,
  `plan.step.suspended`;
- `safety.decided`, `sandbox.bound`, `sandbox.preflighted` where the existing
  C5 fact cannot carry the required revision binding.

Facts use L0 canonical payload and atomic append semantics. The child Specs
must minimize payloads and prefer references/digests over paths, secrets,
policy internals or unbounded outputs. Schema migrations and recovery
classifiers land with their first persisted fact, not later.

## Agent Definition and snapshot amendments

An Agent Definition may reference admitted Goal/Plan capability versions,
built-in tool revisions and maximum portable bounds. Runtime resolves these
references into the immutable effective snapshot. It also freezes concrete
policy, workspace and executor bindings outside the portable definition.

Absence means unsupported. It never means unrestricted. Unknown capability
versions, tool revisions or safety requirement classes fail snapshot
resolution.

## Core amendment

Core receives only a bounded Goal/Plan context projection and neutral proposal
ports. It may:

- propose Goal revision or Plan revision values;
- request progress/evidence observations;
- prepare tool calls already admitted by the snapshot;
- reduce a committed step observation for the next iteration.

Core may not create durable identity, adopt a plan, close a Goal, claim a step,
authorize an effect, select a sandbox or interpret actor credentials.

## Client amendment

H2/H3 expose redacted Goal and Plan projections with stable IDs, revisions,
states, step dependencies, evidence summaries and required interactions.
Mutations use explicit command IDs and expected revisions. Clients never infer
success from stream completion and never retain hidden executable plan state.

## Failure taxonomy

Stable classes are grouped by owner:

- F0: `sandbox_requirement_invalid`, `sandbox_enforcement_unsupported`,
  `sandbox_binding_stale`, `safety_denied`, `safety_interaction_required`,
  `safety_decision_conflict`;
- G1: `goal_invalid`, `goal_revision_conflict`, `goal_transition_invalid`,
  `goal_evidence_insufficient`, `goal_cycle`;
- PL1: `plan_invalid`, `plan_cycle`, `plan_revision_conflict`,
  `plan_binding_stale`, `step_not_ready`, `step_claim_conflict`,
  `step_evidence_conflict`;
- T1 uses existing C4/C5 codes plus tool-specific safe terminal codes.
- T2: `native_snapshot_stale`, `native_node_stale`,
  `native_permission_required`, `native_action_uncertain` and focused
  browser/native failure classes.

Diagnostic text is not a compatibility key. Public failures contain no raw
path, command, environment, credential, policy rule or executor diagnostic.

## Required fault boundaries

Runtime tests interrupt at least:

1. command accepted but no fact committed;
2. Goal/Plan fact transaction committed but not published;
3. step claimed but Kernel Execution not started;
4. sandbox preflight committed but effect not started;
5. effect started with no receipt;
6. receipt committed but step result absent;
7. step terminal committed but Plan/Goal projection not published.

Each position has exactly one recovery classification: retry, reconstruct,
supersede, or typed operator reconciliation. Missing output alone never proves
that a mutation or process did not occur.

## Delivery gates

Every slice runs focused native tests, strict documentation/lint gates for
changed crates, architecture dependency checks and the relevant shared fixture
consumers. F1 additionally runs one real SQLite local Runtime flow through
Goal creation, Plan adoption, read-only tool completion, restart projection
and Goal success evidence.

Production dependencies and SDKs use the repository's latest admitted stable
versions and are pinned through the owning build manifest/lockfile. Runtime
configuration is passed through constructors or explicit configuration
records; no new module reads process environment implicitly.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
