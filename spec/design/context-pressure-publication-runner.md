# C7-C — publication-grade context-pressure runner

## Status

Implemented and verified contract. This slice makes a real C7 baseline
executable; it does not claim that a credentialed run has occurred and does not
admit compression behavior or numeric thresholds.

## Purpose

C7-A can write evidence and C7-B composes the exact provider request, but the
repository still lacks a shipping exchange and secret-safe composition root.
C7-C closes that implementation gap so the only remaining baseline dependency
is executing the checked-in corpus with an installed credential and reviewing
the resulting measurements.

## Ownership and route

```text
strict non-secret run document
  -> injected credential-reference resolver
  -> SecretValue + AnthropicTokenCountProfile
  -> AnthropicProviderCounter
  -> ReqwestTokenCountExchangePort
  -> existing C7-A evidence writer
```

- the experiment CLI owns the run document, evidence path and composition;
- a resolver owns the opaque-reference-to-secret boundary;
- P2-C and P2-VX-ATC continue to own mapping and count protocol semantics;
- the exchange port owns one bounded HTTP attempt, never retry or recovery;
- Runtime, Engine, adapters and Provider profiles do not load configuration or
  credentials.

This evidence-only Rust composition is not a new portable Agent semantic
contract, so no Kotlin copy is admitted. Kotlin continues to cover the shared
P2-C and P2-VX-ATC protocol semantics.

## Strict run configuration

The existing schema-v1 run document gains a tagged counter union. `command`
preserves the explicit development counter and is permanently non-publishable.
`anthropic_messages_exact` contains only non-secret values:

```text
counter_revision
credential_ref
endpoint?                 # absent means the pinned profile default
target_id, model_id
capabilities              # unique explicit portable names
projection_max_output_tokens
extra_headers             # non-sensitive, validated, never reserved
http {
  connect_timeout_ms
  request_timeout_ms
  max_response_bytes
}
publishable
```

The document rejects unknown/duplicate fields and capability names, duplicate
capabilities, empty identities, zero bounds, unsupported media bindings and
plaintext credential fields. The credential reference is redacted from all
diagnostics and is not included in evidence. Its resolved value exists only in
`SecretValue` and sensitive request headers.

The shipping resolver uses the OS credential store under one Garive-owned
service identity. Tests inject a resolver and never modify a user's credential
store. There is no environment-variable, argv-secret, stdin-secret or config
fallback.

## Clean-revision attestation

`dirty` and `garive_revision` are evidence outputs, not trusted caller claims.
A publication request must also provide explicit bounded Git attestation values:

```text
git {
  executable
  repository_path
  timeout_ms
  max_stdout_bytes
  max_stderr_bytes
}
```

The runner launches no shell, clears the child environment, verifies the exact
full `HEAD` equals `garive_revision`, and requires `git status --porcelain=v1
--untracked-files=all` to be empty. Timeout, non-zero exit, excess output,
missing attestation or any worktree entry fails before corpus loading,
credential resolution, HTTP or evidence creation. Non-publication development
runs may omit Git attestation.

## HTTP exchange

`ReqwestTokenCountExchangePort` is constructed from the exact endpoint and
non-zero limits. It uses a client with proxy discovery disabled, redirects
disabled, a connect timeout and a whole-request timeout. It performs exactly
one POST, copies the already prepared headers/body, rejects non-success status,
and reads at most `max_response_bytes + 1` bytes before failing closed.

The port's frozen transport revision and endpoint are part of the C7-B counter
configuration binding. Execution rejects any request whose endpoint differs
from the constructed endpoint.

Publication eligibility additionally requires:

- an `https` endpoint with no username, password, query or fragment;
- a non-loopback, non-localhost host;
- the shipping bounded transport implementation;
- `dirty=false` and `publishable=true` in the strict run document.
- successful clean-revision attestation for the exact evidence revision.

HTTP loopback remains valid for integration tests but is permanently
non-publishable. Status text, response bodies, request content and secrets never
enter errors or evidence.

## Evidence and failure behavior

The existing non-overwriting evidence schema remains the SSOT. A provider run
uses `AnthropicProviderCounter.descriptor()` directly, so evidence binds the
endpoint, model, capabilities, headers, thinking/media policy, projection
limit, transport revision and publication eligibility rather than merely a
child executable path.

Stable CLI failures distinguish invalid configuration, unavailable credential,
invalid counter, failed measurement and evidence-write failure without
including external content. A partial or failed run writes no evidence file.

## Acceptance

- a real loopback server receives the exact projected count body and headers;
- redirect, timeout, non-success, oversized response and endpoint mismatch fail
  closed without retry;
- injected credential resolution proves only the opaque reference crosses the
  resolver boundary and no secret appears in evidence or diagnostics;
- fake/loopback routes cannot request publication, while a strict public HTTPS
  configuration is eligible before external execution;
- command and provider counter configurations are strict and their evidence
  descriptors bind the correct non-secret configuration;
- arbitrary command counters cannot publish, and a forged clean/revision claim
  fails against bounded Git attestation before secret or network access;
- source scans prove no environment lookup, plaintext credential field, proxy
  discovery or Provider-owned transport;
- focused and full Rust gates pass; existing Kotlin P2-C/P2-VX-ATC gates remain
  green.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
