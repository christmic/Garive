# P1 — OpenAI Responses adapter

## Status

Implemented first protocol slice for Rust `adapters/llm-openai` and Kotlin
`experiments/engine-kt/provider-openai`. The supported subset below is exhaustive;
anything not named is rejected before dispatch or fails closed while parsing.

## Evidence coordinates

Reviewed 2026-08-29:

- official create reference:
  `https://developers.openai.com/api/reference/resources/responses/methods/create`;
- official streaming guide:
  `https://developers.openai.com/api/docs/guides/streaming-responses`;
- official `openai-python` commit
  `a1eeab58db02de46717ccebaf1eb83e314fa86ff`
  (`v3.0.0-1-ga1eeab58`);
- inspected SDK request/response, output-item, stream-event, usage, function-call
  and error types under `src/openai/types/responses/` and `shared/error_object.py`.

Official wire types define protocol truth. Sylvander is only an implementation
reference.

## Boundary and composition

- endpoint: `POST /v1/responses`;
- JSON request and ordinary response; SSE when `stream=true`;
- Chat Completions, Realtime, WebSocket, background mode and remote response
  retrieval/cancellation are outside this slice.

The adapter creates a credential-free request descriptor containing method,
path, media headers and body. A Runtime-owned transport supplies base URL,
bearer credential, organization/project routing, timeout and actual I/O. Those
values never enter `ModelRequest`, fixtures or public errors.

The composed `OpenAiModelPort` accepts a Runtime-selected maximum-attempt count.
Transport must classify failure as `BeforeDispatch` or `Ambiguous`. Only the
former may be retried; ambiguity immediately returns
`Interrupted(Transport)`. A received retryable HTTP error may use a valid
`Retry-After`. No provider billing idempotency is inferred.

The current transport boundary returns one complete response body. SSE bytes
are validated as a stream, then authoritative completed/partial items and usage
are sent to the observer. This slice does not claim token-delta delivery or
mid-body cancellation; those require a later chunk-transport contract.

## Supported request subset

The adapter renders exactly:

- `model` from the Runtime-resolved `ModelTargetId`;
- ordered message input with system/developer/user/assistant roles;
- `input_text` and image `input_image` whose reference has already been
  resolved by Runtime to an accepted `image_url` value;
- optional non-zero `max_output_tokens`;
- function tools with name, description, parsed parameters and strictness;
- plain, `json_object`, or strict named `json_schema` text format;
- at most 16 bounded metadata entries;
- `stream` as requested and `store=false`.

`ToolObservation`, `ReasoningReference`, non-image media, instructions,
parallel-tool policy and reasoning configuration are not represented by the
current portable request and therefore fail or are omitted rather than guessed.

## Response and usage

The ordered `output` array maps:

- message `output_text` to `ModelItem.Text`;
- message `refusal` to `ModelItem.Refusal`;
- reasoning summary/text to model-visible reasoning and `encrypted_content` to
  an opaque reasoning reference;
- `function_call` to `ToolIntent` with call ID, name and complete argument text.

Unknown output item or content kinds fail `UnsupportedCapability`. They are not
claimed to be retained by this adapter; sanitized protocol telemetry belongs to
the Runtime transport.

`input_tokens`, `output_tokens`, cache-read and optional cache-write breakdowns
map to `ModelUsage`. `total_tokens` must equal checked input plus output. A
negative/non-integer value or overflow is an adapter invariant failure. Cache
and reasoning breakdowns are not added to the total again.

## SSE lifecycle

Every event requires a non-negative, strictly increasing `sequence_number`.
The first semantic event is exactly one `response.created`; supported item
events then precede one terminal:

```text
response.created
  -> response.in_progress / response.queued*
  -> output_item/content_part/delta/done events*
  -> response.completed | response.incomplete | response.failed
```

The parser validates output-item identity/index, content-part identity/index,
delta kind, final done value, one item completion and equality with terminal
response content. It supports output text, refusal, reasoning summary/text and
function-call argument events. `response.output_text.annotation.added` is
non-assembling and ignored. Every other unknown semantic event fails
`UnsupportedCapability`.

Only a valid `response.completed` creates `Completed`. A supported
`response.incomplete` with `max_output_tokens` creates
`Interrupted(OutputLimit)`. EOF before a terminal creates
`Interrupted(Transport)` with assembled partial items and unknown usage.
`response.failed` currently fails closed as an adapter invariant; no factual
mapping is claimed until its error union is specified and fixture-covered.

## Terminal and HTTP mapping

| Verified fact | Garive fact |
|---|---|
| completed text | `Completed(EndTurn)` |
| completed function call | `Completed(ToolUse)` |
| completed refusal item | `Completed(Refusal)` |
| incomplete `max_output_tokens` | `Interrupted(OutputLimit)` |
| incomplete `content_filter` ordinary response | `Rejected(ContentPolicy)` |
| observer/Runtime cancellation | `Interrupted(Cancelled)` |
| EOF or ambiguous transport | `Interrupted(Transport)` |
| error code `context_length_exceeded` | `Rejected(ContextOverflow)` |
| HTTP 401/403 or `invalid_api_key` | `Rejected(Authentication)` |
| exhausted HTTP 429 | `Unavailable(RateLimited)` |
| exhausted HTTP 5xx | `Unavailable(ModelUnavailable)` |

Unrecognized HTTP/error combinations fail `UnsupportedCapability`; raw bodies,
headers, credentials and user content never enter public evidence.

## Fixtures and acceptance

`spec/fixtures/providers/openai/responses/` contains reviewed metadata, request,
ordinary, complete, composite reasoning/text/tool stream, incomplete, truncated,
content-filter, refusal and HTTP-error bytes. Native tests in both languages
also synthesize malformed sequence, missing-root, identity, done-value,
duplicate/late-terminal and unknown-event cases.

Acceptance requires identical Rust/Kotlin normalization of shared bytes,
official request shape, strict lifecycle/terminal checks, ambiguity-safe retry,
observer cancellation, native tests, and no live request without an explicitly
composed Runtime transport and credentials.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: implemented protocol slice
