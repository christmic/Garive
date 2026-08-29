# P2 — Messages-compatible protocol adapter

> Defines the provider-independent Anthropic Messages wire profile implemented
> by Rust and Kotlin. It covers typed JSON, request construction, incremental
> SSE decoding, extension preservation, and conformance evidence.

## Audience

Engineers implementing `adapters/anthropic-messages` or the experimental
Kotlin `adapter-anthropic-messages` module. Provider and Runtime authors consume
this protocol API without adding deployment behavior to it.

## Why

The previous implementation coupled a small Messages subset to Garive outcome
types and executed retry policy in the codec module. It also buffered an entire
SSE body before observation. A reusable protocol adapter must instead expose
the complete portable wire profile independently of current Agent semantics.

## Evidence

Reviewed local official source on 2026-08-29:

| Source | Coordinate | Inspected paths |
|---|---|---|
| `anthropics/anthropic-sdk-python` | `009b035305e0724ce108ebd796935f91711fc6e1` (`v0.121.0-2-g009b035`) | `src/anthropic/types/message_create_params.py`, `message.py`, content block/delta types, raw stream event types, usage and shared errors |

The generated SDK owns wire field names, requiredness, discriminators, and
event shapes. Garive owns only the portable-profile boundary below.

## Module and dependencies

| Language | Module | Allowed dependencies |
|---|---|---|
| Rust | `adapters/anthropic-messages` (`garive-adapter-anthropic-messages`) | serialization, URI/header validation; no `garive-*` crate |
| Kotlin | `experiments/engine-kt/adapter-anthropic-messages` | Kotlin JSON/serialization; no `:llm`, `:core`, or Runtime module |

Neither module implements `ModelPort` or classifies provider availability.

## Public API

```text
MessagesAdapter::new(MessagesAdapterConfig)
MessagesAdapter::prepare(CreateMessageRequest) -> HttpRequest
MessagesAdapter::decode_response(status, headers, body)
  -> Ordinary(Message) | ProtocolError(ErrorEnvelope)
MessagesAdapter::stream_decoder() -> MessagesStreamDecoder
MessagesStreamDecoder::push(bytes) -> StreamEvent*
MessagesStreamDecoder::finish() -> success | TruncatedStream
```

Kotlin exposes the same responsibilities with idiomatic sealed interfaces.

### Configuration

| Field | Requirement |
|---|---|
| `endpoint` | Required absolute `http` or `https` URI; no default host or path. |
| `headers` | Ordered explicit headers supplied by Garive composition. |
| `protocol_version` | Required non-empty value used for the configured version header; no embedded date. |
| `version_header_name` | Explicit validated name so compatible deployments can select their dialect header. |
| `sensitive` | Per-header redaction marker; values do not enter debug or errors. |
| media types | Protocol constants only: JSON request and JSON/SSE response. |

Header CR/LF, invalid names, duplicate singleton headers, and conflicting media
headers are rejected. Authentication headers and beta headers are opaque
constructor values. The adapter performs no environment or credential-store
lookup.

## Create request profile

The typed request supports these official portable fields:

| Area | Fields and variants |
|---|---|
| Identity | required non-empty `model` and non-negative `max_tokens` |
| Messages | ordered `user` and `assistant` turns; string shorthand or block arrays |
| System | string shorthand or ordered text blocks |
| Content | `text`, `image`, `document`, `tool_use`, `tool_result`, `thinking`, and `redacted_thinking` where valid for input |
| Sources | official base64, URL, plain-text, and content-block source shapes for portable image/document input |
| Generation | `stop_sequences`, `temperature`, `top_p`, `top_k` |
| Tools | client tool name, description, input schema, and optional strictness/cache control present in the official shape |
| Tool choice | `auto`, `any`, `tool`, or `none`, including optional parallel-tool suppression |
| Output | JSON Schema output configuration admitted by the standard create shape |
| Thinking | disabled, enabled with budget, or adaptive configuration when present in the pinned SDK |
| Metadata | optional `user_id` protocol metadata |
| Streaming | explicit `stream` |
| Extensions | explicit non-colliding top-level fields admitted by a Provider profile |

Content-block cache-control values are wire data, not Runtime cache policy.
The adapter validates fixed literals, required identifiers, source unions,
finite numeric values, JSON Schema/input objects, block-role legality, and
extension collisions.

Container reuse, inference geography, service tiers, hosted/server tools,
batches, token-count endpoints, legacy completions, beta resources, and cloud
provider variants are excluded from the portable profile. A Provider may carry
an admitted field through extensions without changing the core adapter.

## Ordinary message profile

`Message` retains:

- `id`, `type`, `role`, `model`;
- ordered output content blocks;
- `stop_reason`, `stop_sequence`, and optional stop details;
- full official usage, including cache and server-tool breakdowns as data;
- unknown non-colliding envelope fields as extensions.

Typed portable output blocks are:

| Discriminator | Required data |
|---|---|
| `text` | text plus lossless citation objects |
| `thinking` | thinking text and signature |
| `redacted_thinking` | opaque data |
| `tool_use` | id, name, JSON input |
| extension | discriminator plus original JSON object |

Hosted server-tool results and future blocks decode as `Extension`. They remain
lossless but acquire no client-tool meaning.

Usage integer fields are non-negative and checked for overflow. The adapter
does not add cache tokens into another field or invent a billing total; Provider
normalization owns any derived neutral usage.

## Error envelope

The official error response is decoded as an outer error object with required
message and type plus extension fields and optional request identifier. Error
type strings are retained even when newer than the pinned SDK.

HTTP status, headers, and error values are returned as protocol facts. Provider
code decides authentication, overload, rate-limit, context-limit, retry, and
sanitization policy.

## Stream events

The portable typed event catalogue is:

```text
message_start
content_block_start
content_block_delta
  text_delta
  input_json_delta
  thinking_delta
  signature_delta
  citations_delta
content_block_stop
message_delta
message_stop
ping
error
```

Unknown event or delta discriminators become lossless extension values.

### Incremental SSE

The decoder:

- accepts arbitrary byte chunks, split UTF-8, LF or CRLF;
- supports comments and repeated `data` lines;
- requires JSON `type` to agree with a present SSE `event` value;
- permits liveness `ping` outside the message lifecycle;
- requires one `message_start` before content or message deltas;
- enforces unique non-negative block indexes, matching delta/block kinds, and
  one stop per block;
- validates partial JSON assembly at `content_block_stop` without rewriting
  the original delta strings;
- allows one `message_delta` terminal update and exactly one `message_stop`;
- treats a protocol `error` as a typed terminal event, not a retry decision;
- emits each event as soon as its complete SSE frame arrives;
- returns `TruncatedStream` for partial frames, open blocks, or a missing
  message terminal at EOF.

Usage snapshots are retained as reported. Monotonic or additive interpretation
belongs to a Provider mapping because the wire fields have event-specific
semantics.

## Acceptance

1. Neither language imports Garive model, Core, Runtime, or Provider types.
2. Neither adapter reads environment variables, user files, or global clients.
3. Every portable request block, response block, error, event, and delta has
   positive and negative native tests in Rust and Kotlin.
4. Shared official-shape fixtures produce equivalent protocol values.
5. Hosted/future extension fixtures round-trip without loss or semantic
   promotion.
6. SSE tests cover every byte split of representative UTF-8, multi-line data,
   block assembly, ping, error, truncation, and terminal sequences.
7. Rust formatting, Clippy, tests, and warning-denied docs pass; Kotlin strict
   explicit API and module tests pass.

## See also

- [`../../docs/architecture/core/provider-adapter.md`](../../docs/architecture/core/provider-adapter.md)
  — ownership and composition boundary.
- [`model-request-stream.md`](model-request-stream.md) — neutral Garive model
  contract consumed later by Providers.
- [`../fixtures/providers/anthropic/messages/`](../fixtures/providers/anthropic/messages/)
  — pinned wire evidence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
