# P2-VX-ATC — Anthropic exact input-token count

## Status

Accepted implementation contract for the first concrete P2-VX capability.

## Purpose

Describe one exact, non-generating Anthropic token-count exchange so C7-A can
measure the same mapped Messages input that Runtime would submit for inference.
This is a vendor capability, not part of the portable Messages-compatible
adapter and not a tokenizer embedded in Agent/Core.

## Evidence coordinates

Reviewed on 2026-08-30:

- official API `POST /v1/messages/count_tokens`, returning `input_tokens`;
- local `anthropics/anthropic-sdk-python` `0.121.0`, revision
  `009b035305e0724ce108ebd796935f91711fc6e1`;
- generated SDK paths `types/message_count_tokens_params.py`,
  `types/message_tokens_count.py`, and `resources/messages/messages.py`.

## Ownership

```text
P2-C Messages mapping -> providers/anthropic token-count projection
                      -> vendor exchange descriptor
                      -> Runtime or evidence-tool HTTP port
```

- `providers/anthropic` owns the capability values, exact projection, response
  decoder, official default endpoint and explicit profile construction.
- `adapters/anthropic-messages` remains unchanged and portable.
- Runtime owns secrets, HTTP, timeout, retry/cancellation and telemetry.
- `experiments/context-pressure-rs` may invoke an explicitly constructed
  exchange through its bounded counter command; it owns no credential lookup.
- Rust is production-first; Kotlin independently mirrors this bounded provider
  contract from the shared fixture and remains experimental.

## Explicit profile

`build_token_count_profile(ConnectionInput)` uses the existing explicit secret,
endpoint-selection and header validation values. The default endpoint is
`https://api.anthropic.com/v1/messages/count_tokens`; an explicit endpoint is
used verbatim after validation, never derived from the create endpoint.

The profile constructs `x-api-key`, `anthropic-version: 2023-06-01`,
`content-type: application/json` and `accept: application/json`. Reserved and
duplicate headers fail before an exchange exists. Debug/string output redacts
the secret. No environment, file, keychain, SDK global, HTTP client or model
catalogue is consulted.

## Exact request projection

Input is one validated portable `CreateMessageRequest` already produced by
P2-C. Projection preserves exactly:

```text
CountTokensRequest {
  model, messages, system, tools, tool_choice, output_config, thinking
}
```

It deliberately omits create-only `max_tokens`, `stream`, stop/sampling fields
and metadata. Provider extensions are rejected because none is admitted for
this endpoint. Empty messages, invalid portable content, duplicate tools or an
invalid thinking shape fail through the existing request validation before
projection.

The descriptor uses `POST`, the profile endpoint and canonical typed JSON. It
does not perform the request. Request bodies and credentials never enter Debug.

## Response

The only success shape is:

```text
TokenCount { input_tokens: positive u64 }
```

Missing, zero, non-integer, overflow, duplicate or unknown fields fail closed.
Non-success HTTP/error-envelope classification remains Runtime/P2-C policy and
must not be guessed from message text.

## Shared fixture

`spec/fixtures/providers/anthropic-token-count-v1.json` freezes:

- the SDK coordinate and official endpoint/version;
- exact create-to-count projection with system, turns, tool, choice, output and
  thinking values;
- exact successful response decoding;
- extension, response-shape and explicit-configuration failures.

Both languages must consume every case independently and produce semantically
equal request objects and stable failure codes. Arbitrary JSON byte equality is
not required.

## Acceptance

- Rust and Kotlin implementations/tests consume every shared case;
- native request validation runs before projection;
- every documented retained field survives and every create-only field is absent;
- descriptor endpoint, method and required headers are exact and redacted;
- malformed/extended responses fail closed;
- source scans prove no environment/config-file/HTTP/Runtime dependency;
- strict Rust and full Kotlin gates pass.

A live successful count with pinned model/config/corpus provenance is separate
publication evidence. Implementing this contract alone does not unlock C7.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted and verified
