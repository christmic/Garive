# C3 — bounded model-only Kernel Execution

## Status

Accepted focused contract under `agent-execution-contract.md`,
`model-request-stream.md`, and `context-surface.md`.

## Responsibility

Execute one disposable, bounded Kernel Execution using immutable input and
frozen Context/Model ports. C3 can complete, suspend, stop or fail exactly once.
It does not persist a Session, execute tools or recover an old in-memory loop.

## AgentTurnRequest subset

C3 implements the complete identity envelope and the model-only subset:

```text
AgentTurnRequest {
  session_id, turn_id, execution_id,
  agent_instance_id, definition_id, definition_revision,
  entry: Start(trusted_input) | Continue(resume_input),
  cursor: completed_iterations + last_durable_position,
  context_policy, capability_context_candidates,
  model_target + recovery_policy,
  limits,
}
```

Every identity/revision is non-empty. `Start` requires a zero reconstructed
cursor. `Continue` requires a non-zero durable position and a typed Runtime
resume input. The request contains no credentials, database handles or provider
wire values.

The cursor describes recovered work before this disposable Execution. The
separate frozen `context_request.through_position` is the initial ledger
watermark visible to this Execution and therefore includes its already
committed start transaction. Core must not replace that watermark with the
zero Start cursor.

## Frozen ports

- `ContextPort.read_candidates(request, rebuild_attempt) -> ordered candidates`;
- `ModelPort.invoke(request, observer, cancellation) -> InvokeOutcome`;
- `EventSink.emit(AgentEvent)` for ordered semantic progress.

Port implementations are selected before execution and cannot be replaced by
Core. The context port performs only the frozen Runtime read; Core merges its
result with `capability_context_candidates` and invokes C2. A port error becomes
`Failed(PortFailure)` unless a more specific required capability failure
applies.

Rust `EventSink` is `Send` because the forwarding observer is retained across
the `ModelPort`'s `Send` future. Kotlin preserves the equivalent single-owner
suspending call without a marker interface.

## Recovery policy

`ModelRecoveryPolicy` is immutable and bounded:

- maximum context rebuild attempts per execution;
- action for `Rejected(ContextOverflow)`: rebuild or fail;
- action for `Interrupted(OutputLimit)`: complete partial, make a bounded new
  request, suspend, stop or fail;
- action for `Interrupted(Transport)`: suspend, stop or fail;
- action for `Unavailable`: bounded alternate target, suspend, stop or fail;
- missing usage policy: conservative estimate or token-limit stop.

Each action that invokes the model again consumes a new iteration and a new
`ModelRequestId`. No action recursively calls the loop outside
`ExecutionControl`.

## Algorithm

```text
construct ExecutionControl from request cursor
repeat:
  check cancellation
  begin iteration or return Stopped(IterationLimit)
  emit IterationStarted
  read ordered candidates from frozen through-position
  merge frozen committed capability candidates by FactRef
  derive context in Core
  if required context exceeds budget: return Stopped(TokenLimit)
  create immutable ModelRequest with new request identity
  emit ModelRequestPrepared
  invoke ModelPort and forward normalized live events
  reduce InvokeOutcome through frozen bounded recovery policy
  if Completed without tool intents: return Completed
  if Completed contains tool intents: return Failed(RequiredCapabilityUnavailable)
  if policy requires durable external wait: return Suspended
  if policy stops/fails: return that terminal
  otherwise continue
```

Tool intents are valid C1 facts but C3 has no tool capability. They cannot be
ignored or presented as a final answer.

## Usage and limits

- iteration is counted before each context/model attempt;
- known usage is accumulated with checked arithmetic;
- unknown usage never contributes zero;
- a conservative estimate is recorded as estimated evidence when policy allows;
- cancellation is checked before derive, before invoke, between stream events
  and before a retry/rebuild;
- a deadline is sampled from the frozen Runtime clock port, not wall-clock calls
  hidden inside Core.

## Events

C3 emits ordered events with Session/Turn/Execution identity:

- `ExecutionStarted`;
- `IterationStarted`;
- `ContextDerived` with counts/references only;
- `ModelRequestPrepared` with request/target IDs;
- normalized model stream events;
- `OutcomeProposed` exactly once.

Events do not contain credentials, raw provider errors, hidden reasoning,
unredacted durable content or a claim that Runtime persisted the event.

## Outcomes

- `Completed`: ordered non-tool response items and usage summary;
- `Suspended`: typed continuation requirement and last durable position;
- `Stopped`: iteration/token/deadline/cancelled reason;
- `Failed`: invalid input/output, required capability, port or invariant reason.

Suspension closes the current `ExecutionControl`. Runtime must commit the
outcome and construct a new `ExecutionId` for continuation.

## Shared capability scenarios

`spec/fixtures/agent/model-only-execution.json` covers:

- one-iteration text completion;
- same Turn continuation with a new Execution ID;
- context overflow followed by one bounded rebuild;
- Skill, Memory and Knowledge are charged by the same C2 item/byte budgets and
  appear in retained/dropped audit references before model assembly;
- duplicate or out-of-order base/capability candidates fail before model use;
- output-limit partial suspension;
- rate-limited/resource-unavailable suspension;
- cancellation before invoke and during stream;
- iteration and token limits;
- unknown usage under conservative and stop policies;
- tool intent without C4 capability;
- context/model/event port failure;
- attempted double terminal and post-suspension reuse.

Fixtures describe scripted fake-port results and normalized events. They are
not provider protocol fixtures.

## Acceptance

- Rust and Kotlin produce semantically equal terminal/events for every shared
  scenario;
- all loops are bounded by `ExecutionControl` and policy counters;
- completion is the only success;
- suspension never leaves an active reusable control object;
- no provider, SQL, Runtime implementation or App dependency enters Core;
- fake ports prove each external call and its ordering.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
