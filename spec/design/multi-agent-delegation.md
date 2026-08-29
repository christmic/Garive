# MA0 — Governed Multi-Agent delegation

## Status

Draft implementation contract in the Agent capability review set. Behavior
remains gated until owner acceptance coordinates the required C3/C6F changes.

## Scope and identity boundary

MA0 defines one parent Agent requesting bounded work from one child Agent in
the same durable Session. Engine Multi-Agent owns portable intent, budget and
result reduction. Runtime owns Agent Instance creation/lookup, authorization,
child Turn lifecycle, scheduling, persistence, cancellation and recovery.

A child is a real Agent Instance with its own exact definition revision,
snapshot, Turn, disposable Executions, limits and ports. Internal draft/critic
model calls, Skills, tools and provider roles are not child Agents.

Distinct non-empty typed identities:

- `DelegationId`: one logical parent-to-child work request;
- `DelegationGrantId`: authority for that exact request/budget;
- `DelegationResultId`: one terminal governed observation;
- existing parent/child `AgentInstanceId`, `TurnId`, `ExecutionId`.

No child identity is derived from a model role or substituted for a tool
invocation. Runtime may address an existing admitted child instance or allocate
one bound to the exact requested definition.

## Portable intent

```text
DelegationIntent {
  delegation_id
  parent_agent_instance_id, parent_turn_id, parent_execution_id
  child_requirement:
    Existing { child_agent_instance_id } |
    Definition { definition_id, definition_revision }
  objective: ContentBinding
  input_evidence: ordered FactReference[]
  result_schema: portable JSON schema
  budget: DelegationBudget
  cancellation_policy: Independent | CancelWithParent
  through_position
}

DelegationBudget {
  max_child_turns: non-zero u32
  max_child_executions: non-zero u32
  max_iterations: non-zero u64
  max_input_tokens?: non-zero u64
  max_output_tokens?: non-zero u64
  deadline_budget_ms: non-zero u64
  max_depth: non-zero u32
}
```

`FactReference` is CF0's exact fact binding. Objective/evidence are bounded and
redacted before the child sees them. The requested budget may only narrow the
parent snapshot's delegation policy and remaining aggregate budget.
`result_schema` uses C4's portable JSON Schema subset, with the C5 interaction
rule that any admitted root type is allowed rather than only an argument object.

`intent_digest` is lowercase SHA-256 over L0 canonical JSON containing contract
`garive.delegation-intent`, version `1`, every field above except delegation
identity. Reusing the identity with a changed digest conflicts.

## Authority and budget escrow

```text
DelegationDecision =
  Authorize { grant_id, intent_digest, reserved_budget, authority_revision }
  | Deny { code }
```

An intent has no authority. Runtime checks actor authority, child definition,
depth, concurrency and remaining budget, then commits a grant before child
creation/start. Grants cannot widen the request or transfer parent credentials,
tool grants, Memory namespaces or Knowledge source access.

Authorization atomically reserves the maximum child budget from the parent's
delegation allowance. Known child usage is charged exactly. Unknown token usage
consumes the corresponding full reservation. Only a committed child terminal
may release unused reservation; process loss or missing usage never creates
budget. Checked arithmetic failure rejects before child start.

V1 permits one active delegation per parent Turn and one child Turn per
delegation. Parallel fan-out, DAGs, swarms, voting and recursive aggregation are
future measured slices. A child may delegate only when its own snapshot admits
it and `current_depth < max_depth`.

## Parent suspension and child lifecycle

After `delegation.requested` and `delegation.authorized` commit, Runtime creates
or verifies the child instance and atomically commits `delegation.child_started`
with the child `turn.started` transaction. The parent Kernel Execution closes
as `Suspended(DelegationPending)` using the same Runtime-owned suspension ID.

This requires coordinated acceptance changes:

- add `DelegationPending` to `SuspensionReason` and C6F snake-case enum;
- add `delegation_result` to continuation input kinds;
- carry delegation/suspension/result bindings through reconstruction.

The current parent Execution is never kept alive waiting for the child. The
child may complete, stop, fail or remain suspended for its own governed input.
Only a child terminal creates a parent result. Child interaction is addressed
to the child; it does not silently ask or authorize through the parent.

## Governed result

```text
DelegationResult {
  result_id, delegation_id, grant_id
  child_agent_instance_id, child_turn_id
  child_snapshot_digest
  outcome:
    Completed { content: ContentBinding, evidence: FactReference[] } |
    Stopped { reason } |
    Failed { code }
  usage
  consumption: child_turns, child_executions, iterations, elapsed_ms
}
```

Runtime commits the child terminal and `delegation.child_terminal`, then commits
one bounded/redacted `delegation.observed` for the parent. Raw child reasoning,
tool grants, credentials and private facts are never inherited. Parent
continuation uses a new Execution and exact suspension/result binding; duplicate
equal result is idempotent and conflicting/cross-parent result fails closed.

A child completion is evidence, not parent success. The parent model may reduce
it in a later iteration. Stopped/failed child outcomes follow the frozen parent
delegation policy: bounded observation, parent failure or explicit new
delegation; they do not trigger an implicit retry.

V1 delegation authorization is an immediate authorize/deny decision. A human
interaction during delegation admission is deferred until a generic governed
subject interaction contract replaces C5's tool-invocation-specific binding;
MA0 does not forge a ToolInvocationId to reuse that API.

## Cancellation and recovery

`CancelWithParent` commits a child cancel request after the parent cancel fact;
success does not claim the child stopped. `Independent` leaves an already
started child running, but the parent may no longer consume its result after a
terminal parent cancellation unless a new authorized Turn references it.

Recovery rules:

- authorized, no child start: same-ID child transaction may commit;
- child start committed: reconstruct child Turn; never allocate another child;
- child terminal, no observation: derive observation from the terminal/result;
- observation committed: continue parent idempotently;
- ambiguous identity, budget or authority state: fail/suspend for operator
  reconciliation, never guess or duplicate work.

## Durable facts

CF0/L0 must admit exact payloads for `delegation.requested`,
`delegation.authorized`, `delegation.denied`, `delegation.child_started`,
`delegation.child_terminal` and `delegation.observed`. Parent suspension,
child start and result continuation must satisfy existing C6 atomic terminal
and fixed-prefix rules.

## Stable failures

`invalid_delegation`, `child_not_found`, `child_revision_mismatch`,
`authority_denied`, `budget_exhausted`, `budget_overflow`, `depth_exceeded`,
`concurrency_exceeded`, `result_schema_mismatch`, `delegation_conflict`,
`child_state_corrupt`, `durability_failure`, and `corrupt_delegation_state`.

## Acceptance evidence

- shared Rust/Kotlin intent/budget/result/reducer fixtures;
- property tests for no budget creation, overflow and depth/concurrency bounds;
- fake Runtime proves no child start before grant/durable facts;
- SQLite process-kill tests across request, grant, child start/terminal,
  observation and parent continuation;
- cancellation and parent/child namespace/authority isolation tests;
- no Runtime, scheduler, model, tool executor or mailbox implementation in
  Engine Multi-Agent.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: draft
