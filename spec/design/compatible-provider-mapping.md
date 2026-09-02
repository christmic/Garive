# P2-C — Compatible deployment Provider mapping

## Status

Accepted implementation contract derived from the accepted Provider ownership
in `docs/architecture/core/provider-adapter.md`.

## Scope

P2-C maps Garive's neutral `ModelRequest`, protocol terminals and protocol
stream facts for one explicitly selected compatible deployment. It supports
the portable Responses-compatible and Messages-compatible profiles without
assuming who operates the endpoint.

This slice does not own endpoint defaults, credentials, authentication
headers, HTTP execution, retries, failover, persistence or vendor extensions.
Those remain P2-V or Runtime responsibilities.

## Modules and dependency direction

```text
engine/llm <- providers/compatible -> adapters/openai-responses
                              \----> adapters/anthropic-messages
```

The Kotlin experiment uses the equivalent `:provider-compatible` module.
Protocol adapters remain independent of Garive semantics. Core and `engine/llm`
must not import Provider or protocol types.

## Explicit deployment values

Each mapper is constructed with an immutable deployment:

```text
CompatibleDeployment {
  target_id,
  model_id,
  capabilities,
  default_max_output_tokens?,
  media_bindings,
  reasoning_profile,
  error_policy
}
```

- `target_id` must exactly equal the neutral request target;
- `model_id` is the protocol model string and is never inferred from target;
- capabilities are deduplicated and explicitly admitted;
- a Messages deployment requires either request `max_output_tokens` or one
  non-zero constructed default because its protocol requires `max_tokens`;
- the mapper reads no environment, file, credential store or global SDK state.

## Capability admission

Every required neutral capability must be present before mapping:

| Neutral capability | Responses-compatible | Messages-compatible |
|---|---|---|
| Text | portable messages/output | portable messages/output |
| Vision | image only with exact media binding | image/document only with exact media binding |
| Reasoning | requires an explicit Responses reasoning profile | requires an explicit Messages thinking profile |
| Tools | portable client function tools | portable client tools |
| JSON output | JSON object/schema format | exact JSON Schema output format |
| Streaming | `stream=true` | `stream=true` |

An extension discriminator is not a capability. P2-C rejects protocol
extensions rather than silently making them model-visible. P2-V may admit one
only with its own exact profile and tests.

## Request mapping

Mapping first validates the neutral request and target/capability snapshot.
JSON tool schemas, tool observations and JSON output schemas must parse as JSON
objects; duplicate members and Portable Tool Schema semantics remain C4-owned.

Garive tool identity remains provider-neutral and may contain dots. Protocol
names already matching `[A-Za-z0-9_-]+` within 64 bytes pass through. Every
other name maps deterministically to `garive_` plus the first 57 lowercase hex
characters of SHA-256 over its UTF-8 bytes. The request-local map must be
collision-free; an unknown or colliding returned name is a protocol invariant
failure. Normalized `ToolIntent` always restores the exact Garive name before
Core or the ledger sees it.

### Responses-compatible

- neutral message roles map one-to-one;
- text maps to `input_text`;
- image references require a constructed URL or file-ID binding;
- tool observations map to `function_call_output` with exact call ID;
- prior tool intents map to `function_call` with the same call ID, mapped name,
  and exact canonical argument JSON, immediately before their observations;
- reasoning references are unsupported by the portable input profile;
- tools map to strict/non-strict function definitions without revision fields
  leaking into protocol JSON;
- plain/JSON-object/JSON-schema output maps exactly to Responses text formats;
- trace metadata is copied as bounded protocol metadata;
- tool choice is `auto` when tools exist and absent otherwise;
- no sampling, truncation, parallelism or extension value is invented.

### Messages-compatible

- leading System and Developer messages become ordered top-level system text
  blocks without changing their text; either role appearing after the first
  conversational item is rejected because moving it would reorder semantics;
- User and Assistant messages retain order and role;
- tool observations become User `tool_result` blocks with exact call ID;
- prior tool intents become Assistant `tool_use` blocks; consecutive tool uses
  and consecutive tool results are grouped without merging a result into an
  earlier ordinary User message;
- image/document references require exact constructed protocol bindings;
- reasoning references require a protocol-valid constructed prior-thinking
  binding; otherwise they are rejected;
- tools map to client tool definitions;
- plain output has no output format; JSON object uses the exact
  `{ "type": "object" }` schema; JSON schema preserves the supplied object;
- trace metadata is not copied into the narrow portable Messages metadata
  field; unsupported neutral metadata makes mapping fail rather than disappear;
- tool choice is `auto` when tools exist and absent otherwise.

## Terminal normalization

Only a protocol terminal validated by its adapter enters normalization.

Portable output mapping is ordered:

- text -> `ModelItem.Text`;
- protocol refusal -> `ModelItem.Refusal`;
- visible reasoning -> `Reasoning(ModelVisible)`;
- encrypted/redacted reasoning -> `Reasoning(OpaqueReference)`;
- client function/tool use -> `ToolIntent` with canonical JSON arguments;
- an unadmitted extension -> `UnsupportedExtension`.

Usage is `ProviderReported`. Missing Responses usage remains Unknown; Messages
usage is required by its protocol. Cache read/write counts remain breakdowns
and are never added twice.

Terminal reasons map as follows:

| Protocol fact | Neutral fact |
|---|---|
| completed/end turn | `Completed(EndTurn)` |
| tool use | `Completed(ToolUse)` |
| stop sequence | `Completed(StopSequence)` |
| pause turn | `Completed(PauseTurn)` |
| refusal | `Completed(Refusal)` |
| output/max token bound | `Interrupted(OutputLimit, partial items, usage)` |
| cancelled | `Interrupted(Cancelled, partial items, usage)` |
| context window exceeded | `Rejected(ContextOverflow)` |

Queued/in-progress ordinary responses, contradictory terminals, missing
required terminal fields and extensions fail closed as mapping errors.

## Error policy

Protocol adapters preserve HTTP status and typed error envelopes but do not
classify them. A compatible deployment contains an immutable ordered set of
exact rules:

```text
ErrorSignature { status, protocol_type, code? }
  -> Authentication | ContentPolicy | ContextOverflow |
     RateLimited(retry_after?) | ModelUnavailable(retry_after?)
```

Rules match exact values only. Duplicate signatures are invalid. The Provider
may extract a validated Retry-After header supplied by Runtime, but does not
sleep or retry. Error messages never participate in classification and never
become durable evidence. Unknown signatures return `UnclassifiedProtocolError`.

Transport failures occurring after dispatch are Runtime facts and are not
created from a protocol error envelope.

## Stream mapping

The protocol decoder owns SSE framing/lifecycle. Provider stream mapping owns
only semantic conversion:

- item/block start -> `OutputItemStarted`;
- text/refusal/reasoning/tool-argument deltas -> the matching neutral delta;
- item/block terminal -> exactly one `OutputItemCompleted`;
- usage updates -> `UsageUpdated`;
- a protocol terminal -> the same terminal normalizer used for buffered JSON.

Provider stream state preserves output order and model call IDs. Unknown event
or delta extensions fail unless a later P2-V profile explicitly admits them.
EOF and lifecycle errors remain adapter failures; Provider does not fabricate a
terminal.

## Failures

Stable P2-C failures are `invalid_request`, `target_mismatch`,
`unsupported_capability`, `unsupported_input`, `invalid_json_object`,
`missing_output_limit`, `unsupported_metadata`, `unsupported_extension`,
`unclassified_protocol_error`, and `protocol_invariant`.

## Shared fixture and acceptance

`spec/fixtures/providers/compatible-mapping-v1.json` contains both protocol
request mappings, ordinary terminals, exact error rules and every stable
failure class. Rust and Kotlin independently consume every case.

Acceptance requires:

- both protocol mappers validate their resulting typed protocol request;
- ordered input, tools, output mode, items, usage and reasons match fixtures;
- every required capability and target mismatch fails before transport;
- no environment/credential/endpoint dependency exists;
- unknown extensions/errors fail closed;
- buffered and streamed terminal normalization agree;
- strict Rust and Kotlin native/shared tests pass.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
