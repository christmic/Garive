# P2 — Anthropic Messages adapter

## Status

Accepted first protocol slice for Rust `adapters/llm-anthropic` and Kotlin
`runtime/server-kt/provider-anthropic`.

## Evidence coordinates

Reviewed 2026-08-29:

- official Messages reference: `https://docs.anthropic.com/en/api/messages`;
- official streaming guide:
  `https://docs.anthropic.com/en/api/messages-streaming`;
- official `anthropic-sdk-python` commit
  `009b035305e0724ce108ebd796935f91711fc6e1`
  (`v0.121.0-2-g009b035`);
- inspected SDK paths: `message_create_params.py`, `message.py`, `usage.py`,
  `raw_message_stream_event.py`, every raw message/content-block event and
  delta type, `resources/messages/messages.py`, `_client.py`, `_exceptions.py`
  and `types/shared/error_*`.

The official docs/SDK define wire truth. Sylvander is an implementation
reference only; disagreement is resolved in favor of these coordinates.

## Boundary

- endpoint: `POST /v1/messages`;
- request/ordinary response media type: JSON;
- streaming: request `stream=true`, response is SSE;
- required version header: `anthropic-version: 2023-06-01` for this slice;
- authentication: `x-api-key` or an explicitly configured bearer credential;
- optional beta features require an explicit, frozen `anthropic-beta` list;
- token counting, batches, legacy completions, Bedrock, Vertex and Foundry are
  outside this slice.

Runtime owns credentials, base URL, version/beta policy and timeouts. Secrets
and transport headers never enter `ModelRequest`, fixtures or public errors.

## Supported request subset

The adapter renders required `model`, `max_tokens` and ordered `messages`.
Messages use only `user` or `assistant`; Core `System` and `Developer` input is
rendered through the top-level `system` content in original relative order.
Interleaving either role after the first user/assistant message is rejected,
because moving it would change meaning. There is no Anthropic `system` role.

Supported content is text, admitted image/media source forms, prior
`tool_use`, and matching `tool_result`. Tools render name, description and
`input_schema`; tool choice and strictness are rendered only when the selected
official contract supports the requested semantics. Optional stop sequences,
temperature, top-p/top-k, service tier, thinking and bounded metadata are sent
only when frozen target policy declares them.

Unknown input blocks, provider server tools, unsupported JSON-output mode or
unrepresentable role ordering fail before dispatch. The adapter never drops or
stringifies them.

## Response blocks

The response `content` array is authoritative and order-sensitive. This slice
normalizes:

- `text` to `ModelItem.Text`;
- `thinking` text plus its opaque signature evidence to `Reasoning`;
- `redacted_thinking` to an opaque `Reasoning` reference;
- `tool_use` id/name/complete input JSON to `ToolIntent`.

Unknown blocks, citations and provider-operated server-tool blocks are kept as
bounded sanitized audit evidence and fail `UnsupportedCapability` unless that
exact target capability was admitted. Signatures are opaque: they are never
parsed, synthesized or logged as user-visible reasoning.

## Usage

Anthropic reports ordinary input tokens and cache input components separately.
Garive maps them without double counting:

| Anthropic | Garive |
|---|---|
| `input_tokens` + `cache_creation_input_tokens` + `cache_read_input_tokens` | known input tokens |
| `output_tokens` | known output tokens |
| `cache_read_input_tokens` | cache-read breakdown |
| `cache_creation_input_tokens` | cache-write breakdown |
| server-tool usage and service tier | provider detail/audit |

All additions use checked `u64` arithmetic. Negative values or overflow fail
the adapter invariant. Cache fields are already included in normalized input
and are never added again by `total_tokens()`.

## SSE sequence

SSE transport can carry `ping` and `error` events in addition to typed message
events. The admitted lifecycle is:

```text
message_start
  -> (content_block_start -> content_block_delta* -> content_block_stop)*
  -> message_delta
  -> message_stop
```

Block indexes are non-negative, unique on start and complete exactly once.
Supported deltas are `text_delta`, `thinking_delta`, `signature_delta` and
`input_json_delta`. Delta kind must match its started block. Incremental tool
JSON is opaque until block completion, then must parse as exactly one JSON
value. A thinking signature must precede its block stop when supplied.

`message_delta` supplies the final stop reason and cumulative output usage.
Usage must not decrease. `message_stop` is the only successful stream
terminal. Duplicate terminal, event after terminal, missing block stop,
unknown meaning-changing delta or mismatched final content fails closed.

`ping` is ignored as transport liveness. An SSE `error` may occur at any point
and terminates factual assembly according to its verified error kind. EOF,
HTTP 2xx or a final `message_delta` alone never completes the request.

## Terminal mapping

| Official fact | Garive fact |
|---|---|
| valid message / `message_stop`, `end_turn` | `Completed(EndTurn)` |
| valid message / `message_stop`, `tool_use` | `Completed(ToolUse)` |
| `stop_sequence` | `Completed(StopSequence)` |
| `pause_turn` | `Completed(PauseTurn)`; Core policy decides continuation |
| `refusal` | `Completed(Refusal)` with a refusal item when supplied |
| `max_tokens` or `model_context_window_exceeded` after output | `Interrupted(OutputLimit)` with partial items |
| verified request context overflow before output | `Rejected(ContextOverflow)` |
| verified authentication/permission rejection | `Rejected(Authentication)` |
| observer cancellation | `Interrupted(Cancelled)` |
| connection/timeout/EOF before terminal | `Interrupted(Transport)` |
| exhausted 429 | `Unavailable(RateLimited, retry_after)` |
| exhausted 5xx/529/model unavailability | `Unavailable(ModelUnavailable)` |

The adapter preserves an explicit refusal as a factual model result; it does
not silently reclassify it as provider transport rejection.

## HTTP errors and retry

Official error responses contain top-level `type=error`, an error object with
typed `type` and `message`, and optional `request_id`. The adapter bounds and
sanitizes these fields, reads the request ID header/body for telemetry only,
and classifies only verified signals. Raw bodies, credentials, headers and user
content never enter public errors.

Runtime supplies retry limits. Retry is permitted only before an externally
ambiguous response and retains the same logical request ID; no provider billing
idempotency guarantee is inferred. Valid `Retry-After` is retained. Status 413,
429, 500/503/504 and 529 remain distinct internal evidence even when multiple
statuses normalize to the same Core envelope.

## Official wire fixtures

`spec/fixtures/providers/anthropic/messages/` contains minimal text, thinking,
redacted thinking, tool use with chunked JSON, cache usage, each stop reason,
ping, stream error, HTTP errors, malformed lifecycle/index/delta/terminal, and
unknown block cases. Each fixture records its official source path and reviewed
commit. Rust and Kotlin consume identical bytes and compare normalized facts.

## Acceptance

- request JSON and headers match the admitted official schema exactly;
- both parsers pass all official-shape fixtures and reject malformed streams;
- chunk boundaries cannot change ordered terminal items;
- only a complete ordinary response or `message_stop` can complete;
- stop reasons, usage and errors follow the mappings above;
- adapter modules depend on the LLM contract, never Core/Runtime policy;
- no live API test runs without explicitly supplied credentials/endpoint.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
