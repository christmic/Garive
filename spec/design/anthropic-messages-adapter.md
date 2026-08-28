# P2 — Anthropic Messages adapter

## Status

Implemented first protocol slice for Rust `adapters/llm-anthropic` and Kotlin
`experiments/engine-kt/provider-anthropic`. The supported subset below is exhaustive.

## Evidence coordinates

Reviewed 2026-08-29:

- official Messages reference: `https://docs.anthropic.com/en/api/messages`;
- official streaming guide:
  `https://docs.anthropic.com/en/api/messages-streaming`;
- official `anthropic-sdk-python` commit
  `009b035305e0724ce108ebd796935f91711fc6e1`
  (`v0.121.0-2-g009b035`);
- inspected create params, message/content-block/usage types, every admitted raw
  stream event/delta, client/error implementation and shared error types.

Official wire types define protocol truth. Sylvander is only an implementation
reference.

## Boundary and composition

- endpoint: `POST /v1/messages`;
- JSON request and ordinary response; SSE when `stream=true`;
- required version header: `anthropic-version: 2023-06-01`;
- token counting, batches, legacy completions and cloud-provider variants are
  outside this slice.

The adapter creates a credential-free request descriptor. Runtime supplies base
URL, `x-api-key` or configured bearer credential, beta policy, timeout and
actual I/O. Secrets and transport configuration never enter `ModelRequest`,
fixtures or public errors.

The composed `AnthropicModelPort` accepts a Runtime-selected maximum-attempt
count. Transport classifies failures as `BeforeDispatch` or `Ambiguous`; only a
proven pre-dispatch failure may retry. Ambiguity immediately returns
`Interrupted(Transport)`. Received retryable errors may honor `Retry-After`.

The current transport returns a complete response body. The adapter validates
SSE bytes, then emits authoritative completed/partial items and usage to the
observer. Token-delta observation and mid-body cancellation require a later
chunk-transport slice and are not claimed here.

## Supported request subset

The adapter renders exactly:

- required `model`, positive `max_tokens`, ordered messages and `stream`;
- user/assistant text messages;
- leading Core System/Developer text as ordered top-level `system` blocks;
- client `tool_result` from a non-empty call correlation and valid JSON result;
  the JSON bytes are carried as official string content, not an invalid object;
- non-strict client tools with name, description and parsed `input_schema`;
- either no metadata or exactly one bounded `user_id` entry.

System/Developer content after conversation start, media, reasoning references,
strict tools and non-plain output modes fail `UnsupportedCapability`. A tool
result wire block is supported, but end-to-end paired tool transcripts remain a
C4/C5 responsibility; this adapter does not invent a preceding assistant
`tool_use` block.

## Response blocks and usage

The ordered `content` array maps:

- `text` to `ModelItem.Text`;
- `thinking` to visible reasoning plus an opaque signature reference when
  supplied;
- `redacted_thinking` to an opaque reasoning reference;
- `tool_use` to `ToolIntent` with ID, name and complete input JSON.

Unknown, citation and provider-server-tool blocks fail
`UnsupportedCapability`. Signatures remain opaque and are never parsed or
presented as ordinary model text. Runtime transport may retain separately
sanitized protocol telemetry; the adapter does not claim unknown-block storage.

Normalized input usage is the checked sum of `input_tokens`,
`cache_creation_input_tokens` and `cache_read_input_tokens`; output and cache
breakdowns remain separately available. Missing cache fields mean zero for the
breakdown. Invalid integers, decreasing cumulative output usage or overflow
fail the adapter invariant.

## SSE lifecycle

`ping` is permitted as liveness and `error` is permitted as a terminal. Every
content/message event otherwise requires exactly one preceding `message_start`:

```text
message_start
  -> (content_block_start -> content_block_delta* -> content_block_stop)*
  -> message_delta
  -> message_stop
```

Block indexes are non-negative and unique. Blocks stop once; delta kind must
match text, thinking or tool-input state. Tool input must parse as one JSON value
at block stop. Output usage cannot decrease. `message_stop` requires a stop
reason and every block closed; it is the only successful stream terminal.

An `error` before content maps only verified authentication, rate-limit or
availability types. Once content exists, it returns
`Interrupted(Transport)` with assembled partial items. EOF returns the same
interruption (with unknown or last reported usage); HTTP 2xx and
`message_delta` alone never complete.

## Terminal and HTTP mapping

| Verified fact | Garive fact |
|---|---|
| `end_turn` | `Completed(EndTurn)` |
| `tool_use` | `Completed(ToolUse)` |
| `stop_sequence` | `Completed(StopSequence)` |
| `pause_turn` | `Completed(PauseTurn)` |
| `refusal` | `Completed(Refusal)` |
| `max_tokens` / `model_context_window_exceeded` after output | `Interrupted(OutputLimit)` |
| observer/Runtime cancellation | `Interrupted(Cancelled)` |
| EOF or ambiguous transport | `Interrupted(Transport)` |
| verified long-prompt/context-window invalid request | `Rejected(ContextOverflow)` |
| HTTP 401/403 or authentication/permission type | `Rejected(Authentication)` |
| exhausted rate-limit response | `Unavailable(RateLimited)` |
| exhausted 500/503/504/529 or API/overload type | `Unavailable(ModelUnavailable)` |

Status 413 alone is not enough evidence for context overflow; the current slice
requires a verified invalid-request type plus bounded long-prompt/context-window
message evidence. Unrecognized HTTP/error combinations fail
`UnsupportedCapability`. Public evidence contains only a bounded error type.

## Fixtures and acceptance

`spec/fixtures/providers/anthropic/messages/` contains reviewed metadata,
ordinary/request/tool-result, complete/truncated/thinking streams, stream error
and HTTP errors. Native tests in both languages synthesize malformed root,
block lifecycle, indexes, deltas, terminal and retry/ambiguity cases.

Acceptance requires identical Rust/Kotlin normalization of shared bytes,
official request/header shape, strict lifecycle and terminal checks,
ambiguity-safe retry, observer cancellation, native tests, and no live request
without an explicitly composed Runtime transport and credentials.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: implemented protocol slice
