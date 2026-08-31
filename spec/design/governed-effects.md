# C5 — Governed effects and model-visible observations

> Contract for Core/Runtime engineers and safety reviewers defining exact-call
> authorization, interaction, receipt-backed execution, uncertainty recovery,
> and the bounded observation returned to the model.

## Audience

Engineers implementing Core effect reduction, Runtime authorization and
execution ports, interaction continuation, durable facts, or Kotlin semantics.

## Why

An immutable Prepared Call is still untrusted and powerless. The system needs
one explicit chain from authority to dispatch to receipt so a rewrite cannot
inherit approval and a missing result cannot trigger an unsafe replay.

## Status

Accepted implementation contract for C5.

## Responsibility split

C5 turns one C4 Prepared Call into a governed fact for the next model
iteration, or a typed suspension/failure. Core owns deterministic reduction.
Runtime owns actor authority, durable identities/facts, concrete execution,
receipts and recovery.

```text
Core: Prepared Call
  -> Runtime allocates ToolInvocationId and commits effect.prepared
  -> AuthorizationPort decides the exact digest
  -> Runtime commits decision
  -> ExecutionPort starts or recovers only an authorized invocation
  -> Runtime commits receipt/result/uncertainty
  -> Core reduces a governed result
```

No definition, model output, Prepared Call, authorization verdict or grant can
directly execute an effect.

## Distinct values

All identities are non-empty opaque, typed and non-interchangeable.
`ToolInvocationId`, `InteractionId`, `GrantId`, `ReceiptId` and executor
dispatch-attempt identity are Runtime-owned. `ModelCallId` remains untrusted
model correlation. None is derived from or substituted for another.

```text
AuthorizationRequest {
  invocation_id: ToolInvocationId
  prepared_call
  actor_authority_reference
  effective_governance_reference
}

AuthorizationVerdict =
  Approve(InvocationGrant)
  | Deny(Denial)
  | ReplacementRequired(ReplacementProposal)
  | InteractionRequired(InteractionRequest)

InvocationGrant {
  grant_id
  invocation_id
  prepared_digest
  tool_name, tool_revision
  granted_requirements
  constraints
  authority_revision
}
```

For portable C5 reduction, `constraints` is represented by a non-empty
`constraints_digest`. Runtime owns the referenced constraint document,
freshness checks and executor-policy interpretation; Core only verifies that
the digest is present and that every grant binding and granted requirement is
equal to or stricter than the Prepared Call. This avoids copying actor policy,
clocks or product configuration into the cross-language reducer.

Core calls the frozen authorization port with the Prepared Call. The Runtime
port implementation constructs the full request above from authenticated
product state; Core never manufactures or interprets actor authority.

The grant binds the invocation identity, exact C4 digest and revision. Granted
requirements are equal to or stricter than requested requirements. A grant
cannot change arguments, tool, replay class, or widen a limit.

`ReplacementRequired` is not approval. Core may turn the proposal into a new
untrusted ToolIntent, run C4 again, allocate a new invocation identity, and ask
for a new decision. The original invocation becomes denied/replaced and can
never start. The replacement verdict is deliberately not named as approval
because it carries no executable authority.

## Interaction boundary

```text
InteractionRequest {
  interaction_id: InteractionId
  invocation_id
  prepared_digest
  kind: Approval | ExternalInput
  prompt: redacted structured prompt
  response_schema
  expiry_policy
}
```

`response_schema` uses the C4 portable JSON value/schema rules, but may declare
any admitted root type rather than C4's tool-argument object restriction.
`prompt` must match `garive.public-suspension-prompt.v1`: exactly
`schema_version = 1`, non-empty `title_key` and `action_label_key`, plus optional
non-empty `message_text` and `cancel_label_key`, with no unknown fields. Runtime
rejects an authority response that does not satisfy this public boundary before
it can be published or persisted.

Runtime commits `interaction.requested` before publication. Core returns
`Suspended(ApprovalRequired|ExternalInputRequired)`; the current Execution is
terminal. A response commits `interaction.resolved` or
`interaction.cancelled`. Continuation uses a new `ExecutionId`, the same Turn
and snapshot, and a typed response bound to the exact interaction/invocation/
digest. Duplicate equal responses are idempotent; conflicting, expired or
cross-Turn responses fail closed.

An `interaction.resolved` fact does not create authority. Portable reduction
returns the invocation to the Prepared state and requests authorization again;
only a later `Approve(InvocationGrant)` can authorize dispatch. Cancellation
produces a rejected observation bound to the original model call. This rule
applies to both approval and external-input interactions; a response that
proposes changed arguments must become a new ToolIntent and pass C4 again.

The governed port result carries the exact `suspension_id`, `interaction_id`,
`invocation_id` and Prepared digest into the Core terminal proposal.
`execution.suspended` and `turn.suspended` reuse that `suspension_id`; terminal
mapping must not allocate a second identity. Operator-reconciliation carries
the corresponding suspension, invocation and digest binding.

## Effect lifecycle

```text
Prepared
  -> Denied
  -> AwaitingInteraction
  -> Authorized -> Started -> Receipt -> Completed | Failed
                          `-> Uncertain
```

Normative durability rules:

1. `effect.prepared` commits before authorization or dispatch.
2. approval/denial/interaction commits before external publication/action.
3. `effect.started` commits immediately before the external dispatch boundary.
4. a trustworthy executor receipt commits before `effect.completed` or
   `effect.failed` is reported to Core;
5. model-visible observation and its invocation/result binding commit before
   the next model request is dispatched;
6. absence of receipt/result after `Started` is uncertainty, not failure and
   not proof that no effect occurred.

Each transition is monotonic and idempotent for the same identity/digest.
Reuse with a different digest, revision, grant or terminal result is a conflict.

## Execution and recovery port

```text
ExecutionCommand { prepared_call, invocation_id, grant }

ExecutionFact =
  Completed { receipt, result }
  | Failed { receipt?, failure }
  | Uncertain { evidence }
  | Unsupported { requirement }
```

A trustworthy receipt binds `receipt_id`, invocation ID, Prepared Call digest,
grant ID, executor identity/revision, terminal executor classification and a
result/evidence digest or content reference. Runtime accepts it only from the
selected executor contract and verifies every binding. Arbitrary tool stdout,
exit text, or a model/provider correlation ID is not a receipt.

Before dispatch Runtime verifies identity/digest binding, grant freshness,
constraints and executor enforcement support. `Unsupported` occurs before
`Started`. Timeout/cancellation after `Started` is `Failed` only when the
executor returns trustworthy receipt evidence proving the terminal effect
classification; otherwise it is `Uncertain`.

Recovery is determined by replay class plus executor proof:

| Position | Required decision |
|---|---|
| before `Started` | Revalidate frozen grant; same-ID dispatch is permitted. |
| `Started`, no receipt | `ReadOnly`/`Idempotent` may retry only when the executor proves the class; `ReceiptRecoverable` recovers its journal; otherwise suspend for operator reconciliation. |
| receipt, no result fact | Reconstruct the exact receipt, require the same concrete executor to acknowledge and release receipt-bound recovery state, then reconstruct the terminal; never dispatch again. |
| terminal result fact | Return it idempotently. |

The declaration on a Tool Definition is insufficient proof by itself.

## Core governed result

```text
GovernedToolResult =
  Observation(ToolFeedback)
  | Suspend(SuspensionRequirement)
  | Fail(GovernedEffectFailure)

ToolFeedback =
  PreparationRejected {
    model_call_id, proposed_tool_name, code, failure_paths
  }
  | Governed(GovernedObservation)

GovernedObservation {
  invocation_id, prepared_digest, model_call_id, tool_name
  outcome:
    Succeeded { content_json, truncated }
    | Rejected { code, safe_details? }
    | Failed { code, safe_details?, partial_content_json? }
}
```

JSON content is valid I-JSON and bounded by the effective output limit. The
lossless audit/result blob, when required, remains a Runtime content reference;
it is not automatically inserted into model context. Mapping to the LLM
`ToolObservation` retains `model_call_id` and a stable neutral JSON envelope.
Runtime redacts secret, policy-internal, raw transport and executor diagnostics
before Core sees it.

The v1 model-visible envelopes are respectively
`{"status":"succeeded","content":...,"truncated":false}`,
`{"status":"rejected","code":"...","details":...}` and
`{"status":"failed","code":"...","details":...,"partial":...}`. Optional
`details`/`partial` fields are omitted, never encoded as invented empty values.
Object-key byte order is not semantic because this JSON is model content, not a
digest preimage.

- denial and C4 preparation failures with a valid model call ID become
  observations when policy allows model self-correction;
- an invalid/empty model call ID cannot be correlated and produces
  `Failed(InvalidModelOutput)` rather than invented feedback;
- required interaction ends the Execution as suspended;
- uncertain effect always ends it as operator-reconciliation suspension;
- missing required ports, invalid grants, identity collisions, corrupt facts or
  durability failure produce `Failed`, never a fabricated observation;
- executor unavailability before `Started` follows frozen policy: bounded retry,
  observation, suspension, or failure.

## Multiple intents

Model output order is authoritative. V1 prepares and governs intents
sequentially. Runtime may execute no two effects concurrently in this slice.
If an earlier intent suspends or fails, later intents remain unallocated and
unstarted. Parallel scheduling, dependency graphs and adaptive concurrency are
deferred to a measured later Spec.

## Stable failures

Codes include `authorization_denied`, `replacement_required`,
`interaction_required`, `grant_mismatch`, `grant_stale`,
`requirement_unsupported`, `execution_failed`, `effect_uncertain`,
`invocation_conflict`, `interaction_conflict`, `durability_failure`, and
`corrupt_recovery_state`. Diagnostic text is not a compatibility key.

## Required acceptance evidence after approval

- shared state scenarios for approve, deny, replacement, interaction,
  unsupported enforcement, completed, failed and every uncertain crash state;
- proof that invalid C4 input and unapproved/replaced calls never execute;
- digest/identity/grant collision and duplicate-idempotency properties;
- Rust/Kotlin Core reducer conformance from shared scenarios;
- Rust fake-Runtime transaction ordering tests; concrete executor tests remain
  Runtime-adapter evidence and cannot be replaced by Core mocks.

## See also

- [`prepared-tool-call.md`](prepared-tool-call.md) — authority-free C4 input.
- [`durable-runtime-turn.md`](durable-runtime-turn.md) — orchestration and
  restart decisions.
- [`durable-runtime-facts.md`](durable-runtime-facts.md) — exact v1 effect and
  interaction payloads.
- [`durable-ledger.md`](durable-ledger.md) — accepted L0 transaction semantics.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
