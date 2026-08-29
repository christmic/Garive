# H1-T — Runtime-owned model HTTP transport

## Status

Accepted implementation contract for the first real model transport used by
H1. Protocol and Provider semantics remain in P1/P2; this slice owns network
attempts only.

## Ownership

H1-T consumes an already constructed P2-C deployment and P2-V0 adapter
configuration. It owns:

- construction of one HTTP client from explicit timeout/size values;
- one POST attempt after the C6 `model.started` commit;
- response status/header/body collection or incremental byte delivery;
- cooperative cancellation while receiving a stream;
- transport-boundary classification where evidence is sufficient.

It does not load endpoint/credential values, select a model, map neutral
semantics, parse protocol JSON/SSE, retry, persist lifecycle state, or choose a
Core recovery action.

## Explicit construction

```text
RuntimeHttpLimits {
  connect_timeout_ms: non-zero,
  request_timeout_ms: non-zero,
  max_response_bytes: non-zero,
}

RuntimeModelTransport =
  Responses { deployment, adapter, client, limits }
  | Messages { deployment, adapter, client, limits }
```

Redirects are disabled so credentials never cross to another origin. Proxy,
root certificate and DNS behavior are supplied through the constructed client;
there is no environment-driven proxy discovery in the admitted configuration.
Debug/string output must not expose sensitive request headers.

## Preflight and dispatch ordering

`ModelPort` gains a side-effect-free `preflight(request)` operation. The
durable Runtime bridge executes this order:

1. validate the neutral request and exact deployment admission;
2. map and encode the protocol request during `preflight`;
3. commit `model.prepared`;
4. commit `model.started` with the one dispatch-attempt identity;
5. execute exactly one HTTP attempt;
6. commit the normalized terminal or explicit uncertainty.

A preflight failure sends no bytes and creates no model lifecycle child. It is
a typed port failure handled by Core. After step 4, a network failure without a
typed protocol terminal returns `ModelPortFailure::RequiredPortFailure`, causing
C6 to persist `model.uncertain`; it must not be guessed as an interrupted or
unavailable provider outcome.

## Buffered exchange

The transport forwards the adapter-produced method, URI, ordered headers and
body exactly. It converts response headers to validated adapter headers,
enforces `max_response_bytes`, then delegates JSON/protocol decoding and P2-C
normalization.

A typed protocol error is classified only by the exact P2-V0 error policy.
`Retry-After` is admitted only as non-negative delta seconds; malformed/date
values are ignored in v1. Unclassified protocol errors are adapter invariants,
not message-text guesses.

## Streaming exchange

For a request whose mapped protocol form enables streaming, response bytes are
fed incrementally to the P1 decoder and P2-C stateful mapper. Each normalized
event is delivered to `ModelObserver` in order. `ObserverDecision::Cancel` or a
sampled cancellation signal stops reading and returns a normalized cancelled
interruption with only adapter-validated partial values.

EOF before a typed protocol terminal is a transport/adapter failure and becomes
durable uncertainty. Total received bytes, including SSE framing, are bounded
by `max_response_bytes`.

## Stable local failures

| Condition | Result before/after dispatch |
|---|---|
| invalid limit or client construction | constructor failure |
| neutral/deployment/protocol mapping failure | preflight `InvalidRequest`, `UnsupportedCapability`, or `AdapterInvariant` |
| request build/header failure | preflight `AdapterInvariant` |
| connect, TLS, timeout, body read, oversized body | post-start `RequiredPortFailure` |
| malformed protocol JSON/SSE/lifecycle | post-start `AdapterInvariant` |
| exact classified protocol error | normalized `InvokeOutcome` |
| unclassified protocol error | post-start `AdapterInvariant` |

No failure text includes URI query values, credentials, request body, response
body or provider message text.

## Acceptance

- both Responses and Messages buffered success/error matrices run against a
  real loopback HTTP server;
- both stream decoders receive fragmented loopback response bytes and preserve
  normalized event/terminal equality;
- preflight failures prove the server received zero requests;
- redirect, timeout, oversized response, truncated stream and cancellation
  cases fail exactly as specified;
- C6 tests prove preflight occurs before `model.prepared`/`model.started` and
  ambiguous post-start failures produce `model.uncertain`;
- source scans prove no environment/config-file access and no retry loop;
- strict Rust gates pass. Kotlin has no Runtime transport parity claim because
  `experiments/engine-kt` is not a production Runtime.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
