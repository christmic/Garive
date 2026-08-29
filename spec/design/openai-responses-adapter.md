# P1 — Responses-compatible protocol adapter

> Defines the provider-independent OpenAI Responses wire profile implemented
> by Rust and Kotlin. This contract drives typed JSON, request construction,
> incremental SSE decoding, extension preservation, and conformance tests.

## Audience

Engineers implementing `adapters/openai-responses` or the experimental Kotlin
`adapter-openai-responses` module. Provider and Runtime authors consume this
API but do not add deployment behavior to it.

## Why

The previous implementation normalized a small fixture subset directly into
Garive model outcomes and also owned retries. That made official protocol
fields inaccessible, treated a buffered body as streaming, and coupled a wire
codec to the current Agent model. This adapter instead models the protocol on
its own terms.

## Evidence

Reviewed local official source on 2026-08-29:

| Source | Coordinate | Inspected paths |
|---|---|---|
| `openai/openai-python` | `a1eeab58db02de46717ccebaf1eb83e314fa86ff` (`v3.0.0-1-ga1eeab58`) | `src/openai/types/responses/`, `src/openai/types/shared/error_object.py`, `src/openai/lib/streaming/responses/` |

The generated official SDK types define field names, requiredness,
discriminators, and stream event shapes. Garive owns only the portable-profile
selection and validation described below.

## Module and dependencies

| Language | Module | Allowed dependencies |
|---|---|---|
| Rust | `adapters/openai-responses` (`garive-adapter-openai-responses`) | serialization, URI/header validation; no `garive-*` crate |
| Kotlin | `experiments/engine-kt/adapter-openai-responses` | Kotlin JSON/serialization; no `:llm`, `:core`, or Runtime module |

Both modules expose protocol values and equivalent behavior. They do not
implement `ModelPort`.

## Public API

```text
ResponsesAdapter::new(ResponsesAdapterConfig)
ResponsesAdapter::prepare(CreateResponseRequest) -> HttpRequest
ResponsesAdapter::decode_response(status, headers, body)
  -> Ordinary(Response) | ProtocolError(ErrorEnvelope)
ResponsesAdapter::stream_decoder() -> ResponsesStreamDecoder
ResponsesStreamDecoder::push(bytes) -> StreamEvent*
ResponsesStreamDecoder::finish() -> success | TruncatedStream
```

Kotlin uses idiomatic class and sealed-interface equivalents. Construction is
the only source of endpoint and header configuration.

### Configuration

| Field | Requirement |
|---|---|
| `endpoint` | Required absolute `http` or `https` URI. The adapter supplies no default. |
| `headers` | Ordered explicit headers supplied by Garive composition. CR/LF and invalid names are rejected. |
| `sensitive` | Per-header redaction marker; sensitive values never appear in debug or errors. |
| `request_media_type` | Defaults only to protocol constant `application/json`, never to deployment data. |
| `stream_media_type` | Defaults only to protocol constant `text/event-stream`. |

Duplicate singleton headers and caller attempts to override `content-type` or
`accept` inconsistently are rejected. Authorization syntax is opaque to the
adapter. There is no environment, filesystem, global-client, or model-catalog
lookup.

## Create request profile

The typed request supports these official core fields:

| Area | Fields and variants |
|---|---|
| Identity | required non-empty `model` |
| Input | string input or ordered message/function-call-output items |
| Roles | `system`, `developer`, `user`, `assistant` |
| Content | `input_text`, `input_image`; image URL or file identifier remains an opaque protocol reference |
| Function output | required `call_id`, string or ordered text/image result content, optional status |
| Generation | `max_output_tokens`, `temperature`, `top_p`, `truncation` |
| Tools | client `function` definitions with name, description, parameters, and strictness |
| Tool choice | `none`, `auto`, `required`, or one named function |
| Tool execution | optional `parallel_tool_calls` |
| Text output | plain text, JSON object, or named strict JSON Schema format |
| Reasoning | optional effort and summary controls from the standard create shape |
| Metadata | bounded string map encoded without reordering guarantees |
| Streaming | explicit `stream` and optional `stream_options` core fields |
| Extensions | explicit extra top-level fields admitted by a Provider profile |

Validation rejects empty required identifiers, non-finite numeric controls,
negative token limits, malformed JSON Schema objects, duplicate metadata keys,
and extension keys that collide with typed fields.

The portable adapter does not create hosted tools, remote conversations,
prompt-template references, background jobs, storage policy, compaction, or
service-tier routing. Those official fields may be carried only through an
explicit Provider-admitted extension map.

## Ordinary response profile

`Response` retains the official core envelope:

- `id`, `object`, `created_at`, `model`, and `status`;
- optional `error` and `incomplete_details`;
- ordered `output` items;
- output text configuration and tool-choice facts when returned;
- full input/output/total token usage and official detail breakdowns;
- unknown non-colliding fields in an extension map.

Typed output items are:

| Discriminator | Required data |
|---|---|
| `message` | id, role, status, ordered `output_text`/`refusal` content |
| `function_call` | id, call id, name, arguments, status |
| `reasoning` | id, ordered summary/content parts, optional encrypted content, status |
| extension | discriminator plus original JSON object |

Text annotations remain lossless JSON because citation and hosted-file shapes
are service extensions. An excluded hosted-tool item decodes as `Extension`;
it is not rejected and is not normalized as text or a client function call.

The decoder validates fixed literals, required fields, integer ranges, and
checked usage arithmetic. It does not reinterpret an HTTP status as retry or
authentication policy.

## Error envelope

Non-success JSON is decoded into the official error envelope with optional
`code`, `message`, `param`, and `type`, plus extension fields. The adapter also
returns status and response headers unchanged through non-sensitive protocol
metadata. Provider code decides classification, sanitization, and retryability.

Malformed JSON, an error response with no error object, or a success response
with an incompatible media type is a typed adapter error.

## Stream events

The typed portable event catalogue is exhaustive for core response items:

```text
response.created
response.queued
response.in_progress
response.completed
response.failed
response.incomplete
response.error | error
response.output_item.added | response.output_item.done
response.content_part.added | response.content_part.done
response.output_text.delta | response.output_text.done
response.refusal.delta | response.refusal.done
response.function_call_arguments.delta
response.function_call_arguments.done
response.reasoning_summary_part.added
response.reasoning_summary_part.done
response.reasoning_summary_text.delta
response.reasoning_summary_text.done
response.reasoning_text.delta
response.reasoning_text.done
response.output_text.annotation.added
```

Every other discriminator becomes `ExtensionEvent` with its original object.
This includes official hosted-tool events and future events.

### Incremental SSE

The decoder:

- accepts arbitrary byte chunks, including split UTF-8 code points and CRLF;
- supports comments, blank dispatch lines, repeated `data` lines, and optional
  `event`/`id`/`retry` fields;
- requires JSON `type` to agree with a present SSE `event` field;
- validates non-negative, strictly increasing `sequence_number` when present;
- permits exactly one response root and one terminal;
- validates item/content indexes, identities, delta/done assembly, and late
  events;
- emits protocol events as soon as one frame is complete;
- returns `TruncatedStream` when EOF leaves an SSE frame or response lifecycle
  incomplete.

`[DONE]` is accepted only as an optional transport sentinel after a protocol
terminal; it cannot replace `response.completed`, `failed`, or `incomplete`.

## Acceptance

1. No source in either adapter imports Garive model, Core, Runtime, or Provider
   types.
2. No source reads environment variables or credential files.
3. Request, response, error, and every listed event have positive and negative
   native tests in both languages.
4. Shared official-shape fixtures decode to equivalent protocol values.
5. Extension fixtures round-trip without losing fields.
6. SSE chunk-boundary tests cover every byte split of representative UTF-8 and
   multi-line frames.
7. Rust formatting, Clippy, tests, and warning-denied docs pass; Kotlin strict
   explicit API and module tests pass.

## See also

- [`../../docs/architecture/core/provider-adapter.md`](../../docs/architecture/core/provider-adapter.md)
  — ownership and composition boundary.
- [`model-request-stream.md`](model-request-stream.md) — neutral Garive model
  contract consumed later by Providers.
- [`../fixtures/providers/openai/responses/`](../fixtures/providers/openai/responses/)
  — pinned wire evidence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
