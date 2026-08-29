# Agent execution contract

## Status and scope

Accepted behavioral contract for C0-C5. It defines one disposable Kernel
Execution and its provider-neutral model/tool interaction. Runtime durability
and concrete adapters are specified separately in C6.

## Identities

The following are non-empty opaque values with distinct types:

- `AgentDefinitionId` + `AgentDefinitionRevision`;
- `AgentInstanceId`;
- `SessionId`;
- `TurnId`;
- `ExecutionId` (one Kernel invocation, new after suspension);
- `ModelRequestId` (one logical model request across proven-safe retries);
- `ModelCallId` (untrusted correlation emitted by a model);
- `ToolInvocationId` (Runtime-owned external-effect identity).

C5/C6 additionally use Runtime-owned `CommandId`, `SuspensionId`,
`InteractionId`, `GrantId`, `ReceiptId`, and dispatch-attempt identity. These
remain distinct from Kernel, model, and tool correlation identities.

No implicit conversion exists between identity types.

## AgentTurnRequest

Runtime freezes this immutable input before calling Core:

```text
AgentTurnRequest
  identity:
    session_id, turn_id, execution_id
    agent_instance_id, definition_id, definition_revision
  entry:
    Start(trusted_user_input) | Continue(ResumeInput)
  cursor:
    completed_iterations, last_durable_position
  context:
    trusted instructions + Runtime-selected durable fact references
  capabilities:
    exact model/tool/skill/memory/knowledge/delegation descriptors
  policy:
    model recovery, context projection, governance requirements
  limits:
    max_iterations and optional token/deadline limits
```

`Continue` does not contain an old in-memory state. It carries a typed durable
answer/reconciliation result plus the position Runtime reconstructed. Core
rejects a cursor inconsistent with the entry or frozen capability snapshot.

## AgentExecutionPorts

Ports are frozen for one execution:

- `ContextPort`: derive a purpose-specific surface from exact durable facts;
- `ModelPort`: invoke a provider-neutral request and emit normalized stream facts;
- `ToolCatalogPort`: resolve exact tool definitions/revisions;
- `AuthorizationPort`: return a grant, denial, constrained replacement request,
  or required interaction for one immutable Prepared Call;
- `ExecutionPort`: execute/recover an authorized invocation and return a neutral
  result/receipt classification;
- `InteractionPort`: request typed external input through Runtime;
- optional capability ports for memory, knowledge, skill and delegation.

Absence is represented as unsupported capability, never a null implementation
that silently succeeds. Ports do not mutate Core control state directly.

## Execution control

Core constructs an active control projection from the request cursor:

```text
ExecutionControl {
  completed_iterations,
  limits,
  status: Active | Closed(AgentOutcomeKind)
}
```

There is no `resume()` transition. Starting an iteration checks limits first,
then increments exactly once. Closing produces one outcome and makes the
control immutable. Runtime creates a new control projection for a later
continuation.

## Limits

- `max_iterations` is non-zero and always present.
- token and deadline limits are optional only when the enclosing Runtime has a
  stricter externally enforced bound.
- unknown usage cannot be treated as zero when evaluating a token limit;
  policy must use a conservative estimate or stop with missing evidence.
- cancellation is checked before each external invocation and between stream
  events; it returns `Stopped(Cancelled)` after durable Runtime coordination.

## AgentEvent

Core may emit ordered semantic events during an execution:

- execution/iteration started;
- context derived and model request prepared (metadata, no secret payload);
- model delta/usage progress;
- tool intent prepared, denied, awaiting interaction, or result observed;
- final outcome proposed.

Events contain all relevant typed identities. Runtime assigns delivery/audit
metadata, redacts for clients, persists required facts, and publishes. A live
event is not evidence that its corresponding durable operation committed.

## AgentOutcome

Exactly one outcome closes an execution:

```text
Completed {
  response_items,
  usage_summary
}
Suspended {
  reason: ApprovalRequired | ExternalInputRequired |
          OperatorReconciliation | ResourceUnavailable | PartialOutput,
  continuation_requirement,
  governed_binding?: suspension_id + interaction_id? +
                       invocation_id + prepared_digest
}
Stopped {
  reason: IterationLimit | TokenLimit | Deadline | Cancelled
}
Failed {
  reason: InvalidInput | InvalidModelOutput | RequiredCapabilityUnavailable |
          PortFailure | InvariantViolation
}
```

`Completed` alone is success. `Suspended` keeps the durable Turn open. `Stopped`
and `Failed` close it unless a later product action explicitly creates a new
Turn; they are not disguised suspensions.

Approval, external-input and operator-reconciliation suspensions require the
exact Runtime-owned governed binding returned after the corresponding fact
commits. Core verifies the binding against the portable C5 requirement and
carries it unchanged to the terminal proposal. Runtime rejects a governed
suspension without that binding; it must never derive a second unrelated
Suspension identity during terminal mapping.

`ExecutionReport` carries the outcome, completed-iteration cursor, and one
cumulative `UsageSummary` for every exit path. Each input/output count is
`Known(u64)` or `Unknown`; `estimated=true` means at least one known component
came from the frozen missing-usage estimate. Unknown provider evidence is not
rewritten to zero. For `Completed`, the report summary and the outcome summary
are identical. Runtime uses the report summary for every durable terminal.

## Model request

`ModelRequest` contains a distinct request ID, model capability target, ordered
input items, admitted tool definitions, output constraints, and trace-safe
metadata. It contains neither provider credentials nor HTTP fields.

Every semantically new neutral request receives a fresh `ModelRequestId`.
Context rebuilds, alternate targets, output-limit retries and later Agent
iterations therefore advance an Execution-local request ordinal. Only a
Provider adapter retry proven to preserve the exact logical request may reuse
that ID; changing target, context, tools or output constraints while reusing it
is an invariant violation.

Input/output uses an ordered `ModelItem` sum type:

- `Text`;
- `Refusal`, a valid provider-declared model result;
- `Reasoning` with visibility (`ModelVisible` or `OpaqueReference`);
- `ToolIntent` with untrusted model call ID, tool name and structured arguments;
- `ToolObservation` with call correlation and neutral result/rejection;
- `MediaReference` with media kind, content reference and declared metadata.

Context `RedactedItem` values remain audit placeholders and are not dispatched
as model input; only their non-secret omission is observable to Core.

Unknown item variants from a future wire version are preserved for audit but
are not silently included in model context.

## Usage evidence

Each token count is `Known(u64)` or `Unknown`; unknown is not encoded as zero.
Usage records input, output, optional cache-read/cache-write breakdowns, source
(`ProviderReported` or `Estimated`). Runtime associates the usage with the
`ModelRequestId` and selected model capability when it commits the fact. Cache
fields are breakdowns and are not added again to input+output total. Arithmetic
is checked.

## Model invocation outcome

The adapter returns one fact envelope:

```text
Completed { items, usage, stop_reason }
Rejected {
  kind: ContextOverflow | Authentication | ContentPolicy,
  sanitized_evidence
}
Interrupted {
  kind: Cancelled | OutputLimit | Transport,
  partial_items, usage
}
Unavailable {
  kind: RateLimited | ModelUnavailable | CircuitOpen,
  retry_after
}
```

These are facts, not actions. A frozen `ModelRecoveryPolicy` maps them to a
bounded next action such as rebuild context, use an admitted alternate target,
make a new continuation request, suspend, stop, or fail. Adapters own only
proven-safe retries within the same logical request and never rewrite intent.

## Tool intent and effect boundary

1. Core validates model tool name/arguments against an exact definition.
2. It produces immutable `PreparedToolCall` with definition revision,
   normalized arguments, digest, requirements and replay class.
3. Runtime allocates `ToolInvocationId`, authorizes the exact digest and commits
   lifecycle facts before dispatch.
4. Core receives only a neutral observation, denial, required interaction, or
   operator-reconciliation suspension.

The C3 `execute_model_only` entry remains an explicit no-tool capability. The
C5 `execute_agent` entry receives the full exact C4 definitions plus a
`GovernedEffectPort`; each call carries the source `ModelRequestId` required by
the durable fact binding. The port is single-owner mutable because one Runtime
writer advances one Session ledger. Core never treats a model Tool Intent as an automatic
failure merely because the earlier C3 slice had no effect port.

Every governed port result includes the latest committed Session position.
Core initializes its Execution-local durable watermark from the frozen Context
request rather than the recovered work cursor. It then advances that watermark
monotonically and derives
the next context surface through that position, so a committed tool
observation can enter the next model request during the same disposable
Execution. A regressing watermark is an invariant failure. This watermark is
not resumable in-memory state: a later Execution still reconstructs it from
the ledger.

`ReplacementRequired` is not approval and does not mutate an authorized call.
It rejects the old preparation and causes a new
preparation/digest/invocation/authorization decision.

## Iteration algorithm

```text
check cancellation and limits; begin iteration
derive purpose-specific context surface
assemble provider-neutral ModelRequest
invoke ModelPort and reduce the fact through frozen recovery policy
if completed with no tool intents: return Completed
for each tool intent: prepare -> authorize -> execute/recover -> observe
if external input/reconciliation required: return Suspended
append observations through Runtime-owned durable ports
continue at next bounded iteration
```

Core never assumes persistence succeeded merely because a port call returned a
live event. Runtime adapters expose committed results at durability boundaries.

## Acceptance matrix

- identity types cannot be substituted;
- iteration/token/deadline/cancellation boundaries close exactly once;
- suspend ends one execution; continuation creates a new execution ID with the
  same Turn ID and durable cursor;
- all model envelopes reduce exhaustively under a bounded policy;
- unknown usage never passes a budget check as zero;
- invalid tool output never reaches authorization;
- uncertain external effects never cause automatic replay without proof;
- Rust and Kotlin pass the shared semantic fixtures declared by the
  cross-language contract.
