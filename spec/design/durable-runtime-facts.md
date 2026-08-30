# C6F — Durable Runtime fact payloads v1

> Reference for Runtime, ledger-adapter, and recovery implementers defining the
> exact schema-v1 payload values C6 commits and consumes across a process or
> storage restart boundary.

## Audience

Engineers writing Runtime fact mapping, Rust/Kotlin ledger validation,
SQLite/PostgreSQL adapters, fault fixtures, or recovery projections.

## Why

L0 requires a versioned payload contract for every new fact kind. A lifecycle
name alone cannot reconstruct content, distinguish uncertainty, or verify that
an observation matches the exact request/effect applied before a crash.

## Status

Accepted C6 companion contract, coordinated with the L0 vocabulary amendment.

## Boundary and encoding

Runtime produces these payloads; ledger adapters validate and persist them;
Runtime recovery consumes them. Every payload is a JSON object encoded by L0
canonical payload v1 with `schema_version = 1` in the outer `DurableFact`.

For v1:

- fields listed as required must occur exactly once;
- `field?` means omitted or present with the declared value, never implicit;
- unknown fields are rejected when applying recovery semantics;
- an unknown fact schema remains readable as opaque audit data;
- IDs are non-empty opaque strings and are also bound by the typed outer fact
  envelope where that envelope has a dedicated identity field;
- `Digest` is exactly 64 lowercase SHA-256 hexadecimal characters;
- counts/durations/positions are non-negative JSON integers in L0 range;
- enums use the lowercase snake-case spelling shown here;
- timestamps are diagnostic outer-envelope metadata, never payload ordering.

## Shared values

```text
ContentBinding {
  digest: Digest
  inline_utf8?: string
  reference?: non-empty opaque string
}

TokenCount = { "kind": "known", "value": u64 }
           | { "kind": "unknown" }

UsageEvidence {
  input_tokens: TokenCount
  output_tokens: TokenCount
  cache_read_tokens?: TokenCount
  cache_write_tokens?: TokenCount
  source: "provider_reported" | "estimated"
}

EffectiveLimits {
  max_iterations: non-zero u64
  max_input_tokens?: non-zero u64
  max_output_tokens?: non-zero u64
  deadline_budget_ms?: non-zero u64
}
```

Exactly one of `inline_utf8` and `reference` is present. The digest is over the
exact UTF-8 bytes or referenced bytes. A reference is admitted only when
Runtime can verify the content-address/digest association before commit. JSON
content is stored as canonical or contract-declared UTF-8 text inside this
binding, so floats or future content grammars do not silently change L0's
integer-only outer canonical payload.

## Turn payloads

```text
turn.started.v1 {
  command_id: CommandId
  kind: "start" | "continue"
  agent_instance_id: AgentInstanceId
  definition_id: AgentDefinitionId
  definition_revision: AgentDefinitionRevision
  snapshot_digest: Digest
  trusted_input_digest: Digest
  prior_suspension_id?: SuspensionId
  expected_session_version?: non-zero u64
}

turn.input.v1 {
  input_kind: "trusted_user" | "trusted_system" | "external_input" |
              "reconciliation" | "resource_ready" | "delegation_result"
  content: ContentBinding
  suspension_id?: SuspensionId
}

turn.cancel_requested.v1 {
  command_id: CommandId
  reason: "user" | "deadline" | "shutdown" | "operator" | "policy"
  requested_through_position: u64
}

turn.suspended.v1 {
  suspension_id: SuspensionId
  execution_id: ExecutionId
  reason: "approval_required" | "external_input_required" |
          "operator_reconciliation" | "resource_unavailable" |
          "partial_output" | "delegation_pending"
  continuation: ContentBinding
  cumulative_usage: UsageEvidence
}

turn.completed.v1 {
  execution_id: ExecutionId
  response: ContentBinding
  cumulative_usage: UsageEvidence
}

turn.stopped.v1 {
  execution_id: ExecutionId
  reason: "iteration_limit" | "token_limit" | "deadline" | "cancelled" |
          "resource_unavailable"
  cumulative_usage: UsageEvidence
  evidence?: ContentBinding
}

turn.failed.v1 {
  execution_id: ExecutionId
  reason: "invalid_input" | "invalid_model_output" |
          "required_capability_unavailable" | "port_failure" |
          "invariant_violation" | "durability_failure" |
          "corrupt_recovery_state"
  cumulative_usage: UsageEvidence
  evidence?: ContentBinding
}
```

The Turn terminal payload and corresponding Execution terminal payload commit
atomically and name the same outcome/reason.

`turn.started.kind=start` forbids both `prior_suspension_id` and
`expected_session_version` and creates an absent Turn. `kind=continue`
requires both, must match the current durable suspension and optimistic
Session version, and reopens that Turn in the same transaction that starts a
fresh Execution. Binding the expected version makes a continuation command's
restart replay semantics complete rather than relying on current projection
state.

## Execution payloads

```text
execution.started.v1 {
  snapshot_digest: Digest
  through_position: u64
  completed_iterations: u64
  limits: EffectiveLimits
  recovery_ordinal: u64
}

execution.iteration_started.v1 {
  iteration: non-zero u64
}

execution.abandoned.v1 {
  reason: "runtime_lost"
  last_safe_position: u64
  recovery_ordinal: non-zero u64
}

execution.completed.v1 {
  response: ContentBinding
  usage: UsageEvidence
  completed_iterations: u64
}

execution.suspended.v1 {
  suspension_id: SuspensionId
  reason: same enum as turn.suspended.v1
  continuation: ContentBinding
  usage: UsageEvidence
  completed_iterations: u64
}

execution.stopped.v1 {
  reason: same enum as turn.stopped.v1
  usage: UsageEvidence
  completed_iterations: u64
  evidence?: ContentBinding
}

execution.failed.v1 {
  reason: same enum as turn.failed.v1
  usage: UsageEvidence
  completed_iterations: u64
  evidence?: ContentBinding
}
```

`execution.abandoned` is Runtime recovery truth, not an `AgentOutcome`. It is
valid only for an active Execution and only when every child invocation is
terminal, pre-dispatch, or durably classified safe/uncertain.

## Model payloads

### Canonical neutral model values

`request_digest` is SHA-256 over L0 canonical JSON for the frozen neutral
request, excluding `request_id` because the outer `ModelRequestId` owns logical
identity. The object contains `target_id`, ordered `required_capabilities`,
ordered `input_items`, ordered `tools`, `output`, and ordered `trace_metadata`.
It uses these exact v1 tags:

| Value | Canonical JSON fields |
|---|---|
| capability | string: `text`, `vision`, `reasoning`, `tools`, `json_output`, or `streaming` |
| message input | `kind=message`, `role`, ordered `content` |
| text content | `kind=text`, `text` |
| media content | `kind=media_reference`, `media_kind`, `reference`, `media_type` |
| tool observation input | `kind=tool_observation`, `model_call_id`, `result_json` |
| reasoning input | `kind=reasoning_reference`, `reference` |
| tool definition | `name`, `description`, `definition_revision`, `input_schema_json`, `strict` |
| plain output | `text_mode={kind:plain}` |
| JSON object output | `text_mode={kind:json_object}` |
| schema output | `text_mode={kind:json_schema,schema_json}` |
| output envelope | `max_output_tokens` (integer or null), `text_mode`, `reasoning_visibility` |
| trace entry | two-element `[key,value]` array |

Roles are `system`, `developer`, `user`, and `assistant`. Media kinds are
`image`, `audio`, `video`, `file`, or `{other:string}`. Ordering is semantic;
Runtime neither sorts nor deduplicates values after Core freezes the request.

Model item content bindings use L0 canonical JSON arrays with these exact
tagged objects: `text`; `refusal`; `reasoning` with `visibility=model_visible |
opaque_reference` and `value`; `tool_intent` with call/name/arguments;
`tool_observation` with call/result; and `media_reference` with media kind and
reference. `inline_utf8` is that canonical JSON and its digest is over the exact
UTF-8 bytes. This representation contains provider-neutral values only.

```text
model.prepared.v1 {
  request_digest: Digest
  capability_target: non-empty string
  deployment_id: non-empty string
  recovery_policy_revision: non-empty string
  max_attempts: non-zero u64
}

model.started.v1 {
  request_digest: Digest
  dispatch_attempt_id: non-empty string
}

model.completed.v1 {
  request_digest: Digest
  stop_reason: "end_turn" | "tool_use" | "stop_sequence" |
               "pause_turn" | "refusal" | "other"
  items: ContentBinding
  usage: UsageEvidence
}

model.rejected.v1 {
  request_digest: Digest
  kind: "context_overflow" | "authentication" | "content_policy"
  evidence?: ContentBinding
}

model.interrupted.v1 {
  request_digest: Digest
  kind: "cancelled" | "output_limit" | "transport"
  partial_items: ContentBinding
  usage: UsageEvidence
}

model.unavailable.v1 {
  request_digest: Digest
  kind: "rate_limited" | "model_unavailable" | "circuit_open"
  retry_after_ms?: u64
}

model.uncertain.v1 {
  request_digest: Digest
  reason: "runtime_lost" | "transport_lost" | "provider_state_unknown"
  evidence?: ContentBinding
}
```

`other` stop reasons retain a sanitized value inside the `items` content, not
as an unbounded enum string used by recovery. Usage is atomic with completed or
interrupted facts.

## Interaction payloads

```text
interaction.requested.v1 {
  interaction_id: InteractionId
  suspension_id: SuspensionId
  prepared_digest: Digest
  kind: "approval" | "external_input"
  prompt: ContentBinding
  response_schema?: ContentBinding
  response_schema_digest: Digest
  expiry_code: "none" | "turn_deadline" | "policy_deadline"
}

interaction.resolved.v1 {
  interaction_id: InteractionId
  suspension_id: SuspensionId
  prepared_digest: Digest
  response: ContentBinding
}

interaction.cancelled.v1 {
  interaction_id: InteractionId
  suspension_id: SuspensionId
  prepared_digest: Digest
  reason: "user" | "expired" | "turn_cancelled" | "operator"
}
```

`response_schema` is present on newly admitted interaction requests and binds
the exact portable JSON Schema needed for restart-safe client continuation.
Readers accept its absence only for compatibility with earlier v1 facts; such
a pending interaction cannot be continued through a typed public client
because a digest alone is insufficient to validate a response.

The outer fact binds `tool_invocation_id` for all three. Resolution/cancellation
is terminal exactly once and must match the requested digest/schema.

## Effect payloads

```text
tool.preparation_rejected.v1 {
  source_model_request_id: ModelRequestId
  model_call_id: non-empty string
  proposed_tool_name: string
  code: "invalid_tool_name" | "tool_not_admitted" |
        "invalid_arguments_json" | "arguments_schema_mismatch" |
        "non_canonical_value"
  failure_paths: ContentBinding
}

effect.prepared.v1 {
  prepared_digest: Digest
  tool_name: non-empty string
  tool_revision: non-empty string
  replay_class: "read_only" | "idempotent" |
                "receipt_recoverable" | "never_replay"
  model_call_id: non-empty string
}

effect.prepared.v3 {
  prepared_contract_version: 3
  prepared_digest: Digest
  tool_name: non-empty string
  tool_revision: non-empty string
  replay_class: "read_only" | "idempotent" |
                "receipt_recoverable" | "never_replay"
  model_call_id: non-empty string
  arguments: ContentBinding
  access_policy_revision: non-empty string
  access_resolver_revision: non-empty string
  invocation_accesses: ContentBinding
  max_result_bytes: positive u64
  sandbox_requirements: ContentBinding
  sandbox_requirements_digest: Digest
}

safety.decided.v1 {
  request_id: non-empty string
  decision_id: non-empty string
  disposition: "allow" | "deny" | "interaction_required"
  prepared_digest: Digest
  tool_name, tool_revision: non-empty string
  actor_authority_reference: non-empty string
  goal_reference?, plan_reference?: non-empty string
  exact_access_digest: Digest
  sandbox_requirements_digest: Digest
  policy_revision: non-empty string
  constraints_digest?: Digest
  safe_code?: "safety_denied" | "safety_interaction_required"
}

sandbox.bound.v1 {
  binding_id, decision_id: non-empty string
  prepared_digest: Digest
  workspace_capability_id: non-empty string
  executor_id, executor_revision: non-empty string
  policy_revision: non-empty string
  access_scope_digest: Digest
  enforcement_digest: Digest
  effective_limits_digest: Digest
}

sandbox.preflighted.v1 {
  preflight_id, binding_id, decision_id: non-empty string
  prepared_digest: Digest
  grant_id: GrantId
  executor_id, executor_revision: non-empty string
  dispatch_attempt_id: non-empty string
}

effect.authorized.v1 {
  prepared_digest: Digest
  grant_id: GrantId
  authority_revision: non-empty string
  granted_requirements: ContentBinding
}

effect.authorized.v2 {
  prepared_contract_version: 3
  prepared_digest: Digest
  grant_id: GrantId
  authority_revision: non-empty string
  constraints_digest: Digest
  granted_requirements: ContentBinding
}

effect.denied.v1 {
  prepared_digest: Digest
  code: "authorization_denied" | "replacement_required"
  safe_details?: ContentBinding
}

effect.started.v1 {
  prepared_digest: Digest
  grant_id: GrantId
  executor_id: non-empty string
  executor_revision: non-empty string
  dispatch_attempt_id: non-empty string
}

effect.receipt.v1 {
  receipt_id: ReceiptId
  prepared_digest: Digest
  grant_id: GrantId
  executor_id: non-empty string
  executor_revision: non-empty string
  classification: "completed" | "failed"
  result_or_evidence: ContentBinding
}

effect.completed.v1 {
  prepared_digest: Digest
  receipt_id: ReceiptId
  result: ContentBinding
}

effect.failed.v1 {
  prepared_digest: Digest
  receipt_id?: ReceiptId
  code: "timeout" | "cancelled" | "tool_failure" |
        "requirement_unsupported" | "executor_unavailable"
  evidence?: ContentBinding
}

effect.uncertain.v1 {
  prepared_digest: Digest
  reason: "started_without_receipt" | "receipt_invalid" |
          "executor_state_unknown"
  evidence?: ContentBinding
}

effect.reconciled.v1 {
  prepared_digest: Digest
  decision: "completed" | "failed"
  operator_evidence: ContentBinding
  observation: ContentBinding
}

effect.observation.v1 {
  prepared_digest: Digest
  model_call_id: non-empty string
  observation: ContentBinding
}
```

The outer fact binds one `ToolInvocationId` throughout. `effect.reconciled` is
legal only after `effect.uncertain`, while its owning Execution and Turn remain
Suspended for `operator_reconciliation`; it is the only transition that can
close that uncertainty from operator evidence. It commits atomically with an
`effect.observation` carrying the exact same `observation` binding. A caller
that cannot make a conclusive `completed` or `failed` decision leaves the Turn
Suspended and appends nothing.

`effect.observation` follows a denial, ordinary effect terminal, or reconciled
effect and is committed before any later model
request that contains the corresponding LLM `ToolObservation`.

The three F0 facts are also Turn/Execution/Tool-Invocation scoped. An Allow
decision requires `constraints_digest` and forbids `safe_code`; Deny and
InteractionRequired require their exact `safe_code` and forbid constraints.
`sandbox.bound` and `sandbox.preflighted` are legal only after Allow and the
matching `effect.authorized.v2`; v2 repeats the exact Allow constraints digest
and identifies Prepared contract v3. Their prepared, policy, decision, binding, grant,
executor and dispatch identities must match exactly. `effect.started` requires
the matching successful preflight and repeats its prepared/grant/executor/
dispatch bindings. V1/v2 Prepared Calls can never enter this F0 chain.
`tool.preparation_rejected` has no Tool Invocation ID because invalid input
never receives one; it binds the outer Model Request/Execution IDs and is also
committed before the correcting observation enters a later model request.

C6 v1 payloads above remain byte-for-byte stable. The accepted C5b increment
owns additive `effect.prepared.v2` and
`execution.effect_batch_planned.v1` schemas; implementing them requires a
coordinated Ledger catalogue/projection change and does not reinterpret any v1
fact. See
[`deterministic-effect-batches.md`](deterministic-effect-batches.md#durable-execution-protocol).

## Atomicity and idempotency

- facts in one declared boundary transaction receive contiguous positions;
- same `FactId` plus equal canonical payload is idempotent;
- same `FactId` or lifecycle identity plus different binding is a conflict;
- receipt and result bindings cannot change after commit;
- terminal publication occurs only after the corresponding terminal payload
  and projection change commit;
- content-reference failure aborts the entire transaction.

## Required acceptance evidence after approval

- JSON fixtures for every payload, optional-field case and enum terminal;
- rejection fixtures for extra/missing fields, wrong types, malformed digests,
  identity mismatch and forbidden transitions;
- Rust/Kotlin L0 validators consume every semantic case;
- SQLite/PostgreSQL adapters prove atomic binding and opaque preservation of an
  unknown newer schema.

## See also

- [`durable-runtime-turn.md`](durable-runtime-turn.md) — transaction order and
  recovery algorithm consuming these payloads.
- [`durable-ledger.md`](durable-ledger.md) — outer envelope, canonicalization,
  positions and idempotent append.
- [`governed-effects.md`](governed-effects.md) — semantic meaning of effect and
  observation values.
- [`agent-definition-snapshot.md`](agent-definition-snapshot.md) — snapshot
  digest bound by Turn and Execution facts.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
