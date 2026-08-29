# CR-B — publication-grade creativity evidence runner

## Status

Implemented and verified contract. CR-B makes the CR-A paired experiment
executable against explicitly configured external model deployments. It does
not admit production `engine/creativity` behavior, choose a creativity policy,
or declare that an unreviewed run is representative.

## Purpose

CR-A proves the paired corpus, blind evaluation and exact reduction route with
non-publishable command fixtures. CR-B closes the remaining implementation gap:

```text
strict corpus
  -> protocol-neutral generator requests
  -> normal compatible Provider mapping
  -> portable protocol adapter
  -> bounded Runtime HTTP transport
  -> strict candidate decoding
  -> arm-blind protocol-neutral evaluator requests
  -> same Provider/adapter/transport route
  -> CR-A validation and pure reduction
  -> clean-revision publication evidence v2
```

The external generator and evaluator are experiment dependencies. Neither is
an Agent, receives tools, reads Garive Memory/Knowledge, executes effects,
persists facts, or owns final-answer authority.

## Protocol and deployment boundary

Each model endpoint is tagged as exactly one portable dialect:

- `responses_compatible`;
- `messages_compatible`.

CR-B constructs the existing neutral `ModelRequest`, uses the existing
`providers/compatible` mapping and the existing protocol adapter, and executes
through the Runtime-owned no-proxy/no-redirect/no-retry HTTP transport. It must
not hand-build protocol request/response JSON, infer a vendor from a URL, add a
vendor allowlist, or add protocol fields to Agent/Core values.

The Runtime transport therefore exposes compatible-dialect constructors that
accept already validated adapter configuration. Official vendor profiles may
continue to wrap those constructors, but CR-B is not tied to OpenAI or
Anthropic names, endpoints, authentication schemes, or special APIs.

## Explicit model configuration

One strict non-secret run document supplies, separately for generator and
evaluator:

```text
ModelEndpointV1 {
  protocol
  target_id
  model_id
  model_revision
  endpoint
  credential_ref
  credential_header_name
  credential_header_prefix
  non_secret_headers[]
  messages_version_header_name?       # messages only
  messages_protocol_version?          # messages only
  max_output_tokens
  connect_timeout_ms
  request_timeout_ms
  max_response_bytes
}
```

All values enter through constructors. Environment variables, default
endpoints, default model names, ambient proxy settings and implicit credential
loaders are forbidden. Duplicate/reserved headers, control characters, zero
bounds, contradictory dialect fields and endpoint userinfo/query/fragment fail
before credential resolution or HTTP.

Only an opaque `credential_ref` is stored in the document. The shipping
resolver reads the OS credential store service `com.garive.creativity`; tests
inject a fake resolver. The credential is inserted into exactly the declared
sensitive header after non-secret validation. It is excluded from diagnostics,
canonical digests and evidence.

## Frozen request templates

The generator template revision is `creativity-generator-json-v1`. It receives
the task ID, arm, seed, exact candidate bounds and generator prompt. It requests
one JSON object matching the CR-A `GeneratedArm` response. The control arm
requires one candidate; the alternatives arm requires two through the corpus
maximum. The generator never receives the evaluator rubric.

The evaluator template revision is `creativity-evaluator-json-v1`. It receives
the task ID, rubric and candidate IDs/content in one JSON request and requests
one JSON object matching the CR-A verdict response. It never receives arm,
selected candidate, generator coordinates or selection rationale.

Each task/arm performs exactly one generator call and one evaluator call. Only
a completed text-only terminal containing one strict JSON object is accepted.
Tool intents, refusals, reasoning output, multiple text items, interrupted /
rejected / unavailable outcomes, malformed JSON and extra response fields are
infrastructure failures. There is no retry, repair prompt, fallback, best-of
rerun or hidden model call.

## Publication eligibility and provenance

A model endpoint is publication-eligible only when:

- its endpoint is public HTTPS with no userinfo, query or fragment and is not a
  localhost/loopback address;
- target, model and asserted model revision are non-empty and bounded;
- its complete non-secret configuration and template revision have a canonical
  SHA-256 digest;
- configuration validation succeeds before resolving the credential.

The run is publishable only when both endpoint descriptors are eligible,
`dirty=false`, and bounded Git attestation proves the configured repository's
exact clean `HEAD`. Evidence v2 binds the Git executable digest and attestation
configuration digest. Loopback tests and CR-A command ports remain permanently
non-publishable.

The endpoint descriptor is transparent evidence, not proof that a remote
service honored an asserted model revision. Reviewers must decide whether the
deployment provides sufficient immutable model identity for a representative
baseline; the runner does not upgrade a mutable alias into stronger evidence.

## Evidence v2

The non-overwriting sink writes:

- contract/version, `publishable`, exact Garive and runner revisions;
- corpus ID/revision/digest and deterministic seed;
- generator/evaluator protocol, target, model and asserted revision;
- their non-secret canonical configuration digests and template revisions;
- Git executable/configuration digests;
- source-ordered numeric pair evidence and exact global/class reductions.

Evidence excludes credentials, header values marked sensitive, prompts,
rubrics, candidates, selected candidate IDs, reasoning and raw provider bodies.
Failures are content-free stable codes and create no evidence file.

## Acceptance

- strict config mutation tests prove dialect/header/bound/endpoint validation
  occurs before credential or network access;
- secret-invariance and every-non-secret-variant tests cover descriptor digests;
- real loopback tests for both compatible protocols prove normal Provider and
  adapter request shapes, strict terminal decoding, arm blindness and exactly
  one request per task/arm/role;
- redirect, timeout, status, size, malformed protocol and non-completed outcome
  tests fail closed without retry;
- publication tests require two eligible endpoints plus exact clean Git
  attestation and write content-free non-overwriting evidence v2;
- source scans prohibit environment loading and protocol JSON construction in
  the CR-B composition;
- focused and full Rust gates pass; Kotlin protocol/Provider gates stay green.

A real credentialed run and review remain external evidence after this slice is
implemented. Only that review may admit a separate production Creativity Spec.

Repository evidence is the compatible-dialect Runtime constructors, shared
experiment Git attestation crate, CR-B model ports/configuration/OS credential
resolver/publication sink and CLI under `experiments/creativity-baseline-rs`.
Native tests cover both protocol routes, blind request shapes, every strict
failure boundary, non-secret digests, exact clean Git composition and
content-free evidence v2. `engine/creativity` remains empty.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
