# C1 — provider-neutral model facts

## Status

Accepted focused contract under `agent-execution-contract.md`.

## Responsibility

Represent ordered model items, usage evidence, and the factual result of one
logical model request after only adapter-proven-safe transport retries.

## Usage

`TokenCount` is `Known(u64)` or `Unknown`. `ModelUsage` contains input, output,
optional cache-read/cache-write breakdowns, and source (`ProviderReported` or
`Estimated`). `total_tokens()` returns:

- `Known(input + output)` using checked arithmetic;
- `Unknown` if either component is unknown;
- `Overflow` when known arithmetic exceeds `u64`.

Cache fields are breakdowns and are not added to the total. Unknown never
defaults to zero.

## Ordered items

`ModelItem` is an ordered sum type:

- `Text { text }`;
- `Refusal { text }`, a provider-declared refusal returned as valid model
  output rather than a transport or policy rejection;
- `Reasoning { content }`, where content is visible text or an opaque reference;
- `ToolIntent { model_call_id, tool_name, arguments }`;
- `ToolObservation { model_call_id, result }`;
- `MediaReference { media_kind, reference }`.

Structured arguments/results use a validated portable value in C4. Until C4,
C1 preserves them only as opaque JSON text and makes no validation or
canonicalization claim. They must not enter tool preparation, authorization,
or a digest before C4 validation.

## Fact envelopes

`InvokeOutcome` is exhaustive over four envelopes:

- `Completed { items, usage, stop_reason }`;
- `Rejected { kind, sanitized_evidence }`, with kind
  `ContextOverflow`, `Authentication`, or `ContentPolicy`;
- `Interrupted { kind, partial_items, usage }`, with kind `Cancelled`,
  `OutputLimit`, or `Transport`;
- `Unavailable { kind, retry_after }`, with kind `RateLimited`,
  `ModelUnavailable`, or `CircuitOpen`.

`Completed` alone is success. `Interrupted` alone is partial. Outcome values do
not prescribe retry, failover, prompt rewriting, suspension, stopping, or
failure. A frozen Core recovery policy owns that mapping.

`ModelStopReason` is `EndTurn`, `ToolUse`, `StopSequence`, `PauseTurn`,
`Refusal`, or bounded provider-neutral `Other`. `PauseTurn` and `Refusal` are
factual successful terminals; Core policy may suspend or stop after observing
them, but adapters cannot rewrite either as an error.

## Validation and safety

- adapter reasons/evidence are sanitized and bounded before construction;
- partial items are never promoted to completed items;
- HTTP/provider SDK values never cross this boundary;
- request dedup does not claim provider billing idempotency;
- item order is preserved exactly;
- opaque reasoning/media references are references, not assumed accessible
  bytes.

## Shared semantic fixture

`spec/fixtures/agent/model-outcome.json` covers known/unknown/overflow totals,
all envelope and reason kinds, ordered item preservation, and partial/success
classification. Rust and Kotlin consume every case independently.

## Acceptance

- exhaustive kind mapping for all envelopes/reasons;
- only Completed reports success and only Interrupted reports partial;
- unknown and overflowing usage remain distinct;
- cache counts are not double-counted;
- all item variants and order survive normalization;
- Rust and Kotlin native and shared-fixture tests pass.
