# Model protocol adapters and providers

> Separates protocol adapters, Provider composition, Runtime policy, and the
> neutral model contract. Engineers implementing an LLM integration use this
> ownership map before selecting types, configuration, or transport behavior.

## Audience

Engineers changing `engine/llm/`, `adapters/`, `providers/`, or Runtime
model composition in any supported language.

## Why

An LLM request crosses four independent boundaries:

1. Garive expresses a provider-neutral model request.
2. A Provider selects a deployment and maps neutral values to a protocol.
3. A protocol adapter encodes and decodes that protocol.
4. Runtime governs credentials, attempts, cancellation, and recovery.

Combining these owners makes a protocol implementation incomplete and makes a
deployment impossible to replace. It also encourages hidden environment
loading, vendor defaults, and retry behavior that Runtime cannot audit.

## Ownership

```text
Agent -> engine/llm neutral port
              |
              v
        Provider composition
        - deployment and model selection
        - neutral/protocol mapping
        - capability and error policy
              |
              v
        Protocol adapter
        - official wire types
        - request validation and JSON
        - response and error JSON
        - incremental SSE state machine
              |
              v
        Runtime HTTP transport
        - credentials and endpoint config
        - timeout, cancellation, attempts
        - durable recovery and telemetry
```

| Owner | Owns | Must not own |
|---|---|---|
| Model contract | Neutral request, observation, usage, and outcome values. | Protocol JSON, HTTP, credentials, or deployments. |
| Provider | Deployment identity, model routing, capability negotiation, neutral/protocol mapping, and verified error classification. | Environment discovery inside a protocol adapter or Agent policy. |
| Protocol adapter | One documented wire protocol and its standard HTTP exchange description. | Garive model types, vendor accounts, model catalogues, retries, failover, or recovery. |
| Runtime | Validated system configuration, secrets, HTTP execution, attempt policy, cancellation, durability, and recovery. | Protocol field invention or model-visible semantic guessing. |

## Protocol adapter contract

The first adapters implement the standard, portable profiles of:

- OpenAI Responses-compatible create requests and response streams;
- Anthropic Messages-compatible create requests and response streams.

The protocol names identify wire dialects, not a deployment vendor. A service
implemented by another company may use either adapter when it implements the
declared profile.

Each language exposes equivalent responsibilities:

```text
Adapter::new(AdapterConfig)
  -> encode typed request
  -> describe one HTTP exchange
  -> decode ordinary response or protocol error
  -> incrementally decode SSE frames into typed events
```

The adapter returns protocol values. It does not implement Garive `ModelPort`
and does not normalize directly into `InvokeOutcome`.

### Construction

Every deployment-dependent value is explicit constructor input:

| Input | Rule |
|---|---|
| Endpoint | Required absolute HTTP(S) URI; no vendor default. |
| Headers | Supplied by Garive composition; names and values validated. |
| Protocol version | Explicit when the dialect requires a version header. |
| Media policy | Explicit accepted request/response media types. |
| Transport | Injected by Runtime or represented as a request descriptor. |

Adapters never read process environment, user directories, global SDK state,
or platform credential stores. Sensitive constructor values use redacted
debug/string representations and never enter protocol errors.

### One exchange

A protocol adapter describes or executes one attempt. It may:

- validate a typed protocol request;
- add protocol-required media headers;
- parse an HTTP response by status and media type;
- accept arbitrary byte chunk boundaries;
- emit each complete SSE event once;
- report syntax, lifecycle, and unsupported-profile errors.

It must not retry, sleep, apply backoff, open a circuit breaker, switch a model,
refresh credentials, or infer crash recovery. Provider and Runtime decide what
an error means outside the wire protocol.

## Portable profiles

Official SDKs contain the standard endpoint plus vendor-hosted services. The
adapters fully type and validate the portable profile while preserving an
explicit extension envelope for other documented or future discriminators.

### Responses-compatible

The portable profile includes:

- create request controls common to model inference;
- text/image messages and function call outputs;
- client function tools and tool choice;
- plain, JSON object, and JSON Schema text output;
- message, refusal, reasoning, and function-call response items;
- usage and incomplete/error details;
- every lifecycle and delta event for those item kinds.

Hosted search, hosted files, computer control, code execution, image
generation, shell/apply-patch, MCP hosting, conversations, background jobs,
compaction, retrieval, deletion, and vendor prompt objects are extensions.

### Messages-compatible

The portable profile includes:

- create request controls common to model inference;
- system, user, and assistant text/image/document blocks;
- client tool definitions, choices, uses, and results;
- output configuration, stop controls, thinking blocks, and usage;
- message/content-block lifecycle and delta events;
- standard protocol error envelopes.

Server tools, hosted web/file/code execution, containers, batches, token-count
endpoints, cloud-provider variants, and beta endpoints are extensions.

### Extensions

An unknown or excluded discriminator decodes as a typed `Extension` containing
its discriminator and original JSON object. This provides forward-compatible,
lossless protocol handling without claiming that Garive can execute the
extension. Provider capability mapping must explicitly admit an extension
before it becomes model-visible behavior.

Invalid JSON, a missing discriminator, a duplicate terminal, illegal event
order, index reuse, or a mismatched done value remains an error; it is not an
extension.

## Provider composition

A Provider depends on `engine/llm` and one protocol adapter. It owns:

- mapping `ModelRequest` to the portable protocol profile;
- mapping protocol events and terminals to neutral facts;
- selected deployment/model identity and admitted capabilities;
- capability admission for protocol extensions;
- provider-specific error evidence and sanitization.

Official-vendor profiles may define documented endpoint defaults,
authentication/header schemes, and special capabilities, but Runtime supplies
and freezes the concrete endpoint and credential values. Those policies do not
move into the reusable protocol adapter. Compatible deployments have no vendor
defaults.

## Runtime composition

Runtime parses and validates Garive configuration, constructs the Provider and
adapter, then owns the attempt lifecycle. No model request discovers endpoint
or credentials dynamically. A configuration revision is frozen before a
durable execution dispatches.

Runtime records whether dispatch was known not to start, started, completed,
or became ambiguous. Only Runtime has enough durable evidence to retry or
require reconciliation.

## Conformance

Rust and Kotlin implementations are checked independently against pinned local
official SDK source coordinates and shared official-shape fixtures.

| Dimension | Required evidence |
|---|---|
| Types | Every portable request, response, error, and event discriminator is catalogued. |
| JSON | Decode/encode semantic equality, required-field rejection, and extension preservation. |
| SSE | Arbitrary chunk boundaries, multi-line data, UTF-8 splits, lifecycle, terminal, and EOF behavior. |
| Configuration | Constructor validation, redaction, and source scan proving no environment reads. |
| Cross-language | Same fixtures produce equivalent protocol values and errors. |

Matching Rust and Kotlin output is not evidence when both disagree with the
pinned SDK. The SDK coordinate and inspected paths remain part of each adapter
Spec.

## See also

- [`../system.md`](../system.md) — product ownership and dependency direction.
- [`../../../spec/design/openai-responses-adapter.md`](../../../spec/design/openai-responses-adapter.md)
  — Responses-compatible implementation contract.
- [`../../../spec/design/anthropic-messages-adapter.md`](../../../spec/design/anthropic-messages-adapter.md)
  — Messages-compatible implementation contract.
- [`../../../spec/design/compatible-provider-mapping.md`](../../../spec/design/compatible-provider-mapping.md)
  — compatible deployment request/outcome/error/stream mapping contract.
- [`.agents/testing.md`](../../../.agents/testing.md) — protocol evidence gates.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
