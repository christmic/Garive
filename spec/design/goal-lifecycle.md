# G1 — Durable Goal lifecycle

## Status

Accepted implementation contract for durable work that spans Turns. G1 does
not replace Turn, Session, Memory, Scheduler or Multi-Agent identities.

## Ownership

`engine/goal` owns validated Goal definitions, evidence requirements and pure
state transitions. Runtime owns authenticated commands, identity allocation,
parent-graph lookup, optimistic concurrency, persistence, clocks and public
projection. Core receives a bounded projection and may propose commands; it
cannot commit them. Kotlin implements portable semantics only.

## Identity and revision

`GoalId`, `GoalCommandId`, `GoalRevision` and `GoalEvidenceId` are distinct.
IDs are non-empty opaque strings. Revision is a positive interoperable integer.

Creating a Goal allocates revision 1. Any change to objective, criteria, scope,
bounds, parent or effective capability references creates exactly the next
revision. State-only transitions also advance revision, making one expected
revision the complete optimistic-concurrency boundary. A command identity
replayed with byte-equivalent input returns its original receipt; different
input is `goal_command_conflict`.

## Definition

```text
GoalDefinitionV1 {
  goal_id
  objective: non-empty bounded UTF-8
  criteria: non-empty ordered unique GoalCriterion
  scope: GoalScopeV1
  bounds: GoalBoundsV1
  parent_goal_id?
  capability_references: canonical unique exact revisions
}

GoalScopeV1 {
  session_id?
  workspace_capability_ids: canonical unique opaque references
}

GoalBoundsV1 {
  max_attempts, max_plan_revisions, max_child_goals: non-zero u32
  token_budget?, duration_budget_ms?: optional non-zero u64
}
```

Objective is user/product intent, never instructions that bypass Agent
Definition precedence. At least one scope anchor is required: Session or
workspace capability. A workspace reference is not a path or authority.

Portable admission is byte-bounded before allocation into a durable value:

- Goal, criterion and evidence identities are at most 256 UTF-8 bytes;
- objective text is at most 16 KiB UTF-8;
- Session, Workspace, capability, artifact/fact-kind, evidence and stable
  reason references are at most 512 UTF-8 bytes;
- criteria, Workspace references, capability references and evidence sets
  contain at most 256 members.

All limits count encoded UTF-8 bytes, not scalar values or grapheme clusters.
Canonical decoding re-applies the same limits; an oversized historical value
is corrupt rather than silently truncated. Public presentation may apply a
stricter display bound but cannot weaken durable admission.

Parent and child definitions are individually canonical. Runtime validates the
complete parent graph is acyclic, the parent is non-terminal, and child scope,
capabilities and remaining bounds do not exceed the parent grant.

## Success criteria

```text
GoalCriterion =
  UserAcceptance { criterion_id, response_schema_digest }
  | Artifact { criterion_id, artifact_kind, required_digest? }
  | DurableFact { criterion_id, fact_kind, subject_digest }
  | ChildGoals { criterion_id, child_goal_ids }
```

Criterion IDs are non-empty and unique in declared order. Schemas and expected
digests are immutable references, not inline unbounded content. `ChildGoals`
contains a non-empty canonical unique set and is valid only after those Goals
exist under this parent.

Evidence is typed and exact:

```text
GoalEvidenceV1 {
  evidence_id
  criterion_id
  kind
  durable_reference
  evidence_digest
  observed_at_commit_version
}
```

Runtime resolves the reference at or before the frozen commit version and
verifies kind, ownership, digest and scope. Model assertions, client-local
flags, live events and diagnostic strings are not evidence. A criterion is
satisfied only by its admitted evidence kind. Goal success requires every
criterion, unless a later contract adds an explicit composite criterion.

## Canonical definition digest

`definition_digest` is lowercase SHA-256 over RFC 8785 JSON:

```json
{
  "contract": "garive.goal-definition",
  "version": 1,
  "goal_id": "...",
  "objective": "...",
  "criteria": [],
  "scope": {},
  "bounds": {},
  "parent_goal_id": null,
  "capability_references": []
}
```

Optional values are encoded as explicit JSON null in this digest. Array order
is semantic for criteria and canonical for reference sets. State, revision,
evidence and Runtime actor data are excluded.

## State machine

```text
Draft -> Active -> Succeeded
          |  |----> Failed
          |  |----> Cancelled
          |  `----> Suspended -> Active
          `--------> Revised Draft
Draft -----------------------> Cancelled
Suspended -------------------> Failed | Cancelled | Revised Draft
```

`GoalState = Draft | Active | Suspended | Succeeded | Failed | Cancelled`.
Succeeded, Failed and Cancelled are terminal for that Goal ID. A new objective
after terminal creates a new Goal rather than reopening history.

Commands:

- `Create` freezes revision 1 in Draft;
- `Activate` requires Draft or Suspended plus an admitted definition/plan
  posture;
- `Revise` requires non-terminal state, exact expected revision and produces
  Draft; any active attempt is durably ended first in the same transaction;
- `Suspend` requires Active and a typed resumable reason/reference;
- `Succeed` requires Active and a complete verified evidence set;
- `Fail` requires Active or Suspended and a stable terminal reason;
- `Cancel` requires any non-terminal state and authenticated actor authority.

No-op transitions are invalid except exact command replay. Attempt and plan
limits are checked before Activate; exceeding a hard bound transitions through
an explicit Fail command, never silent cancellation.

## Runtime facts

Runtime atomically commits one command receipt with the corresponding fact:

| Fact | Required payload |
|---|---|
| `goal.created` | identity, revision 1, definition digest/content binding, actor reference |
| `goal.revised` | old/new revisions and digests, replacement reason |
| `goal.activated` | revision, attempt number, plan reference? |
| `goal.suspended` | revision, stable reason, interaction/reconciliation reference |
| `goal.succeeded` | revision, ordered criterion/evidence bindings |
| `goal.failed` | revision, safe terminal code, evidence references |
| `goal.cancelled` | revision, actor reference, safe reason |

Payloads use L0 canonical content bindings. Objective text and evidence bodies
are bounded content references where required; facts contain no credentials,
paths or private policy diagnostics.

The projection validates contiguous revisions, legal transitions, immutable
definition binding within a revision, unique command/evidence identities and
terminal closure. Corrupt prefixes fail reconstruction instead of skipping a
fact.

## Turn and Plan integration

A Turn may reference zero or one active Goal revision and Plan revision in its
frozen execution input. The Turn terminal does not automatically terminalize
the Goal. Runtime reduces committed step/criterion evidence after the Turn and
issues an explicit Goal command if policy permits.

One Goal can have several immutable Plan revisions but at most one adopted
non-terminal Plan. A Goal may be Active without a Plan only when its admitted
policy explicitly allows direct work; the default local product requires an
adopted Plan before the first effectful step.

## Recovery and concurrency

- committed fact, missing publication: rebuild and publish the projection;
- accepted command without a fact: retry the same command identity;
- concurrent expected revision: exactly one commit wins, loser receives
  `goal_revision_conflict` and must reread;
- evidence reference unavailable/corrupt: do not close Goal; return typed
  durability/evidence failure;
- active Goal after worker loss: reconstruct attempt/Plan/effect positions;
  never infer success from an absent worker.

## Public projection

`GET /v1/sessions/{session_id}/goals` is the complete H2 Goal graph at one
verified Session watermark. It returns stable Goal-ID order, current revision,
state, definition digest, bounded objective display text, optional parent ID,
attempt count and criterion totals. It omits scope, Workspace and capability
grants, actors, reasons and evidence references. Objective truncation is
UTF-8-safe and explicit. Goal count, fact scan and encoded response size are
independently bounded.

The Rust Host client validates the exact Session, API version, stable unique
order, closed state vocabulary, digests, counts, parent existence and graph
acyclicity. H3 remains the public effect/activity stream and does not duplicate
Goal lifecycle state; clients reread H2 after a durable change notification.

## Stable failures

`goal_invalid`, `goal_command_conflict`, `goal_revision_conflict`,
`goal_transition_invalid`, `goal_evidence_insufficient`, `goal_evidence_invalid`,
`goal_scope_exceeded`, `goal_cycle`, `goal_bound_exceeded`, and
`goal_recovery_corrupt` are compatibility codes.

## Acceptance evidence

- shared Rust/Kotlin fixture for validation, canonical digest, every legal and
  illegal transition, evidence completeness and terminal closure;
- properties for monotonic revision, no terminal reopening, idempotent exact
  command replay and parent narrowing;
- Rust L0 payload validation and SQLite migration/projection;
- real SQLite races, restart at every fact/publication boundary, corrupt
  prefix refusal and atomic revise-attempt termination;
- H2 Runtime and Rust-client projection tests proving bounded/redacted state,
  one-watermark reconstruction and parent-graph refusal;
- authoritative Runtime revision-conflict tests for competing commands.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
