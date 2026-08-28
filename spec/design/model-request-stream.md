# C1b — provider-neutral model request and stream

## Status

Accepted focused contract under `agent-execution-contract.md` and
`model-invoke-outcome.md`.

## Responsibility

Represent one immutable logical model request, normalized live stream facts,
observer cancellation and an async/suspending Model port. Provider HTTP/SSE
types are adapter-owned and never implement this contract directly.

## Identities and target

- `ModelRequestId`: non-empty opaque identity for one logical request across
  only proven-safe adapter retries.
- `ModelTargetId`: Runtime-selected, non-empty target opaque to Core. Runtime
  resolves it for the selected adapter; its value may be a provider deployment
  or model identifier, but never contains credentials.
- `ModelCapability`: declared requirement such as text, vision, reasoning,
  tools, JSON output or streaming.

Runtime freezes the exact target and capability snapshot for one Kernel
Execution. Unsupported or unknown capability fails before dispatch.

## Input values

`ModelInputItem` is ordered and exhaustive for this slice:

- `Message { role, content }`, where role is `System`, `Developer`, `User`, or
  `Assistant`;
- `ToolObservation { model_call_id, result_json }`;
- `ReasoningReference { reference }` for an opaque, adapter-approved encrypted
  or persisted reasoning continuation.

`ModelInputContent` is `Text` or `MediaReference`. A media reference carries a
kind, opaque content reference and declared media type; Core never reads bytes
from the reference.

Tool observations and tool schemas contain opaque JSON text until C4 validates
them. C1b preserves their bytes and order but forbids their use in a digest,
authorization decision or execution.

## Tool descriptors

An admitted model-visible tool contains:

- stable name and description;
- exact definition revision;
- input-schema JSON text;
- strictness declaration.

This descriptor lets an adapter render the provider schema. It is not an
authorized executable call. C4 owns validation and Prepared Calls.

## ModelRequest

```text
ModelRequest {
  request_id,
  target_id,
  required_capabilities,
  input_items,
  tools,
  output: {
    max_output_tokens,
    text_mode: Plain | JsonObject | JsonSchema,
    reasoning_visibility,
  },
  trace_metadata,
}
```

- request, target and tool names are non-empty;
- `max_output_tokens`, when present, is non-zero;
- capability requirements and tools are deduplicated;
- metadata keys/values are bounded and contain no secrets;
- item and tool order is preserved exactly.

## Stream facts

`ModelStreamEvent` contains only normalized facts:

- `OutputItemStarted { output_index, kind }`;
- `TextDelta { output_index, delta }`;
- `RefusalDelta { output_index, delta }`;
- `ReasoningDelta { output_index, delta }` for model-visible reasoning only;
- `ToolArgumentsDelta { output_index, model_call_id, delta }`;
- `OutputItemCompleted { output_index, item }`;
- `UsageUpdated { usage }`.

Output indexes are zero-based, monotonic when first observed, and identify one
item. Deltas for an item occur only between its start and completion. A
completed item is emitted once. Provider lifecycle/error events reduce to the
final `InvokeOutcome` or adapter telemetry; they are not fabricated as model
content.

Live stream events are best-effort observations. The terminal outcome carries
the authoritative ordered items and normalized usage that Runtime commits.

## Observer

The observer receives events in order and returns:

- `Continue`; or
- `Cancel`.

After `Cancel`, the adapter stops reading/dispatching as promptly as its
transport permits and returns `Interrupted(Cancelled, partial_items, usage)`.
The observer cannot mutate the request, change Runtime policy or turn a partial
item into a completed item.

In Rust the observer is `Send`, because `ModelPort` returns a `Send` future and
may retain the mutable observer across transport awaits. Kotlin's suspending
port provides the equivalent structured-concurrency ownership without a marker
interface.

## Model port

```text
suspend/async invoke(request, observer, cancellation) 
  -> Result<InvokeOutcome, ModelPortFailure>
```

`ModelPortFailure` is limited to failures that prevented a trustworthy
`InvokeOutcome`: invalid frozen request, unsupported capability, adapter
invariant failure, or required port failure. Authentication, context rejection,
transport interruption and availability are factual `InvokeOutcome` values.

The port performs no product failover, approval, prompt rewrite or persistence.

## Shared semantic fixture

`spec/fixtures/agent/model-request-stream.json` covers:

- validation of identities, bounds and duplicate capabilities/tools;
- exact input/tool ordering;
- valid interleaved item streams;
- delta before start, duplicate completion and non-monotonic item start;
- observer cancellation and partial-item preservation;
- known/unknown usage updates.

Rust and Kotlin consume every case independently. The fixture is semantic test
data, not an OpenAI or Anthropic wire shape.

## Acceptance

- request values contain no provider transport or credential fields;
- invalid stream sequences fail closed without inventing completed content;
- observer cancellation produces one interrupted fact;
- terminal items preserve provider order independently of delta chunking;
- adapters can map request/events without importing Core or Runtime;
- Rust and Kotlin native/shared tests pass.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
