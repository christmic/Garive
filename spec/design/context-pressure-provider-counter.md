# C7-B — exact provider counter composition

## Status

Implemented and verified contract. C7 compression remains gated on a live
publication run and measured retention trade-offs.

## Purpose

Connect C7-A's exact assembled `ModelInputItem` values to the normal portable
Messages Provider mapping and the independently admitted P2-VX-ATC token-count
exchange. C7-B proves that evidence uses the request a deployment can actually
submit; it does not add compression behavior.

## Ownership

```text
Core assemble_model_inputs
  -> P2-C map_messages_request
  -> P2-VX-ATC project_token_count_request
  -> injected TokenCountExchangePort
  -> strict P2-VX-ATC response decode
```

- Core owns the sole provider-neutral assembly order.
- P2-C owns neutral-to-portable Messages mapping and capability admission.
- `providers/anthropic` owns the count projection/profile/response.
- `experiments/context-pressure-rs` owns this evidence-only composition and its
  injected exchange port; it is not Runtime or Agent behavior.
- a product/composition boundary resolves an opaque credential reference into
  `SecretValue` and builds `AnthropicTokenCountProfile` before construction.

No credential, credential reference, header value, request content, response
body or stderr enters evidence or the canonical counter configuration.

## Counter configuration

Construction receives only explicit values:

```text
AnthropicProviderCounterConfig {
  counter_revision,
  deployment: MessagesDeployment,
  profile: AnthropicTokenCountProfile,
  projection_max_output_tokens,
  publishable
}

TokenCountExchangePort {
  transport_revision()
  publication_eligible()
  execute(TokenCountHttpRequest) -> bounded response bytes
}
```

The projection output limit is non-zero and exists only to validate/map the
portable create request; P2-VX-ATC removes it from the count body. The counter
infers required Text/Tools/Vision/Reasoning capabilities from exact input items
and lets P2-C reject unsupported deployment/input combinations.

V1 supports text, image/document bindings and tool observations exactly as
P2-C Messages mapping does. A reasoning reference remains unsupported because
P2-C cannot map it. There is no fallback renderer or approximate tokenizer.

## Secret-safe canonical binding

`TokenCounterDescriptor.config_digest` binds canonical JCS over:

- fixed counter identity and counter revision;
- transport revision and publication eligibility;
- target/model IDs, sorted admitted capabilities, configured thinking and
  output limits;
- exact endpoint and ordered non-sensitive header name/value pairs;
- the `x-api-key` header name and a secret placeholder, never its value.

Publication construction rejects missing/duplicate API-key headers, any other
sensitive header, media bindings not represented by the admitted deployment,
an ineligible exchange port, empty identities/revisions and invalid limits.
Changing a bound non-secret value changes the digest.

The counter and request descriptor Debug output must not reveal the credential
or body. Environment variables, argv secrets, plaintext benchmark configuration
and implicit SDK/global clients are forbidden.

## Exchange port

The port represents exactly one bounded attempt. It receives the prepared
vendor request including redacted headers and returns only success-body bytes.
It owns timeout, maximum response bytes, redirects/proxy policy, cancellation
and HTTP status/error-envelope handling. It does not retry. A fake/loopback
port must report `publication_eligible=false`; requesting publication through
such a port fails at construction.

## Acceptance

- all four C7-A corpus classes pass Core assembly and P2-C mapping through a
  deterministic non-publication exchange port;
- late SystemNotice coverage proves no instruction remains inside history;
- exact tool observation and capability admission paths are exercised;
- unsupported reasoning, deployment capability mismatch, malformed response,
  ineligible publication and invalid secret/header profile fail closed;
- digest tests prove secret substitution is invariant while endpoint, model,
  thinking, non-secret header and transport revisions are variant;
- source scans prove no environment/config-file/credential-store lookup;
- Rust focused/full gates pass.

A real eligible port plus an externally resolved credential is required to
write `publishable=true` evidence. C7-B unit/loopback success is not that run.

Repository evidence is `experiments/context-pressure-rs/src/provider_counter.rs`
plus `tests/provider_counter.rs`. It covers all four reference classes, global
instruction assembly, capability and response failures, publication refusal,
secret substitution, and non-secret route configuration binding.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
