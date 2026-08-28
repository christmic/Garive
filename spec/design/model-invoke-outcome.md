# Core Agent C1a — model invocation outcome

## Responsibility

Define the provider-neutral facts returned by one model invocation after the
adapter has exhausted only the transport retries proven safe by its contract.
The outcome does not contain HTTP status codes or prescribe Agent/Runtime
recovery policy.

## Interface

`garive-llm` exposes `InvokeOutcome`, an exhaustive enum with nine variants:

| Variant | Evidence carried |
|---|---|
| `Completed` | normalized text and usage |
| `Overflow` | submitted normalized input size and optional accepted-limit evidence |
| `OutputTruncated` | valid prefix and usage observed so far |
| `RateBudgetExhausted` | optional retry delay |
| `PartialCancelled` | valid prefix and usage observed before cancellation |
| `AuthFailure` | sanitized reason |
| `ContentViolation` | sanitized reason and optional violated field/category |
| `ModelUnavailable` | requested model identity |
| `CircuitBreakerOpen` | adapter target/pool identity |

`InvokeOutcome::kind()` returns a payload-free stable category for metrics and
exhaustive dispatch. It is not a wire tag commitment.

## Normalized usage

`ModelUsage` carries input, output, cache-read, and cache-write token counts.
Counts describe context/accounting evidence, not currency or guaranteed
provider billing. Cache counts are breakdown fields and are not added again by
`total_tokens()`, which checks only `input + output`. Unknown values are
represented by the adapter's surrounding evidence in a later slice; C1a values
are observed counts and default to zero.

## Invariants

1. Exactly one outcome variant is returned per invocation.
2. `Completed` is the only success variant.
3. Truncated and cancelled prefixes are not promoted to completed text.
4. An `Overflow` is already a verified provider-specific classification; Core
   never parses an HTTP code.
5. Reasons are sanitized and must not contain credentials or raw response
   bodies.
6. Outcomes report facts only. Retry, failover, prompt revision, suspension,
   and termination are decisions in later Core/Runtime slices.
7. Local request deduplication does not claim provider billing idempotency.

## Errors and compatibility

Construction is infallible because adapters already validate and normalize the
payload. Adding another variant is a source-breaking contract change: all
dispatch matches and contract tests must be updated deliberately.

## Acceptance tests

- all nine variants map to their distinct `InvokeOutcomeKind`;
- only `Completed` reports success;
- partial/truncated values retain their prefix and usage without conversion;
- usage total uses checked addition and returns `None` on overflow.

## Out of scope

- provider request/response encoding and streaming transport;
- model request/context types (C1b/C2);
- retry/AIMD/circuit-breaker implementation;
- durable request identity, receipts, and billing reconciliation.
