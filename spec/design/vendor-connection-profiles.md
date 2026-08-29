# P2-V0 — Official vendor connection profiles

## Status

Accepted implementation contract derived from P2-C and the accepted
Provider/Runtime ownership split.

## Scope

P2-V0 supplies explicit official OpenAI Responses and Anthropic Messages
connection profiles. A profile turns Runtime-supplied connection values into:

1. one protocol adapter configuration; and
2. one exact default protocol-error policy for P2-C.

It does not execute HTTP, load configuration, resolve credentials, refresh
tokens, retry, persist, select a model, or admit hosted/vendor extension
capabilities. Hosted tools and special APIs remain P2-VX slices, one accepted
capability Spec at a time.

## Evidence coordinates

Reviewed local official SDK source on 2026-08-29:

| Profile | Repository revision | Inspected paths |
|---|---|---|
| OpenAI | `openai/openai-python` `a1eeab58db02de46717ccebaf1eb83e314fa86ff` | `src/openai/_client.py`, `src/openai/resources/responses/responses.py`, `src/openai/_utils/_logs.py` |
| Anthropic | `anthropics/anthropic-sdk-python` `009b035305e0724ce108ebd796935f91711fc6e1` | `src/anthropic/_client.py`, `src/anthropic/resources/messages/messages.py`, `src/anthropic/lib/credentials/_auth.py` |

The SDKs are evidence for official endpoint paths, authentication header
schemes and the Anthropic protocol version. Garive deliberately does not copy
their environment, credential-chain, HTTP-client or retry behavior.

## Modules

```text
providers/profile       explicit Runtime-supplied connection value types
providers/openai        OpenAI Responses connection/error profile
providers/anthropic     Anthropic Messages connection/error profile

experiments/engine-kt/:provider-profile
experiments/engine-kt/:provider-openai
experiments/engine-kt/:provider-anthropic
```

Vendor constants live in the owning vendor module's `constants.rs` or
`Constants.kt`; they do not enter protocol adapter `wire` vocabularies.

## Runtime-supplied values

```text
ConnectionInput {
  endpoint: Default | Explicit(absolute_http_uri),
  credential: SecretValue,
  extra_headers: [ExplicitHeader]
}
```

- `SecretValue` must be non-empty, rejects CR/LF/NUL, and redacts Debug/string
  output. It has no environment/file/keychain constructor.
- `ExplicitHeader` validates name/value, carries an explicit sensitivity bit,
  and preserves input order.
- an endpoint override is an explicit Runtime choice; empty/relative or
  non-HTTP(S) values fail before an adapter exists;
- duplicate header names and profile-reserved headers fail rather than using
  precedence;
- the returned adapter configuration may contain the secret header only as a
  redacted protocol `Header`; the error policy and compatible deployment never
  contain credential material.

## OpenAI profile

Pinned official values:

| Value | Exact profile constant |
|---|---|
| default endpoint | `https://api.openai.com/v1/responses` |
| credential header | `authorization` |
| credential value | `Bearer {credential}` |

`authorization`, `content-type` and `accept` are reserved. The profile returns
a Responses adapter configuration plus this exact default error policy:

| Status | Type | Code | Neutral disposition |
|---|---|---|---|
| 400 | `invalid_request_error` | `context_length_exceeded` | `ContextOverflow` |
| 401 | `invalid_request_error` | `invalid_api_key` | `Authentication` |
| 429 | `rate_limit_error` | `rate_limit_exceeded` | `RateLimited` |
| 503 | `server_error` | `server_error` | `ModelUnavailable` |

## Anthropic profile

Pinned official values:

| Value | Exact profile constant |
|---|---|
| default endpoint | `https://api.anthropic.com/v1/messages` |
| credential header | `x-api-key` |
| version header | `anthropic-version` |
| protocol version | `2023-06-01` |

`x-api-key`, `authorization`, `anthropic-version`, `content-type` and `accept`
are reserved. P2-V0 uses API-key authentication only; OAuth/federated bearer
credentials require a later Runtime credential-provider contract and are not
silently treated as API keys.

The profile returns a Messages adapter configuration plus this exact default
error policy:

| Status | Type | Code | Neutral disposition |
|---|---|---|---|
| 401 | `authentication_error` | absent | `Authentication` |
| 429 | `rate_limit_error` | absent | `RateLimited` |
| 529 | `overloaded_error` | absent | `ModelUnavailable` |

The captured Anthropic context-overflow envelope has no exact code and is
therefore intentionally unclassified: P2-C forbids guessing from its message.

## Composition

The profile does not duplicate P2-C request/outcome/stream mapping. Runtime
constructs a P2-C deployment with the selected target, model, capabilities,
media/reasoning settings and the profile's error policy, then constructs the
protocol adapter from the profile's adapter configuration.

```text
Runtime configuration + secret port
  -> vendor profile
       -> adapter config
       -> exact error policy
  -> P2-C compatible deployment
  -> Runtime HTTP transport
```

Model identity and capability sets remain explicit Runtime/Agent Definition
values. No vendor model catalogue or capability guess is embedded in P2-V0.

## Failures

Stable P2-V0 failures are `empty_credential`, `invalid_credential`,
`invalid_endpoint`, `invalid_header`, `duplicate_header`, `reserved_header`,
and `profile_invariant`.

Neither adapter exception text nor secret material is exposed through these
failures.

## Shared fixture and acceptance

`spec/fixtures/providers/vendor-connection-profiles-v1.json` freezes default
and explicit endpoints, redacted authentication headers, protocol-version
values, exact error rules and every stable failure.

Acceptance requires:

- Rust and Kotlin consume every shared case independently;
- constructed adapter configurations pass their native validation;
- sensitive headers redact diagnostics and retain the exact transport value;
- default and explicit endpoints are distinguishable;
- every reserved/duplicate/invalid value fails before transport;
- exact error matrices match P2-C dispositions without message inspection;
- source scans prove no environment/config-file/credential-store/HTTP/Runtime
  dependency in any profile module;
- strict Rust and full Kotlin gates pass.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
