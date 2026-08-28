# Provider Adapter — failure handling + transport

> Once `derive → assemble → model.invoke` is on the wire, the
> loop is talking to a **provider**. From there on, everything
> is provider-side: transport, auth, streaming, failure,
> recovery. This document is the design for everything
> **below `model.invoke`** — the layer the loop skeleton
> doesn't see directly, but knows about.

`loop.md` covers the skeleton (`agent_loop` → `agent_turn` →
`derive` → `assemble` → `governance.judge` → `executor.run`).
This doc covers the **adapter layer** that wraps the
provider's wire API and translates its failure modes into
events the loop skeleton can react to.

## Scope split

| Layer | Lives in | Failure policy |
|-------|----------|----------------|
| `agent_loop` / `agent_turn` skeleton | `loop.md` | Cannot fix anything; observes events only |
| `derive` / `assemble` | `loop.md` | Pure functions; no I/O |
| **`model.invoke` boundary** | **this doc** | **Retries transient failures locally; surfaces semantic failures upward** |
| Provider transport (HTTPS / SSE / WebSocket) | this doc | Resilient; auth refresh; connection pool |
| Ledger / governance / executor | `loop.md` + `ledger.md` | Receives events from `model.invoke` |

The **boundary rule** is the load-bearing invariant:

> **Retries are local to `model.invoke`. Semantic failures
> bubble up to the loop skeleton as a single event.**

A retry never escapes `model.invoke` as a "the model said
something different". A semantic failure never gets
silently retried by the loop — the loop gets exactly one
notification per `model.invoke` call.

## Three engineering layers

### Layer A — Theoretical foundations

The failure-handling code rests on five standard primitives:

| Primitive | What it does | When it applies |
|----------|-------------|-----------------|
| **Exponential backoff + jitter** | Retry-after time grows exponentially; jitter decorrelates retries across callers. | All transient HTTP failures (5xx, 429, connection reset). |
| **Circuit breaker** | After N consecutive failures, stop trying and fail fast; periodically probe to recover. | Provider down for an extended period. Avoids hammering a dead endpoint. |
| **End-to-end principle** | Higher layers don't re-derive what the lower layer already knows; each layer has a single source of truth. | The loop doesn't re-track the request state — the adapter does. |
| **Idempotency + retry safety** | A retry retains one stable Runtime `request_id` and identical request digest. | Use a provider idempotency key only when officially supported; otherwise do not assume a started call is safe to replay. |
| **AIMD** (Additive-Increase / Multiplicative-Decrease) | Concurrency grows slowly; on 429 / rate-limit, drops by half. | Provider rate limits. |

These five are the **toolbox**, not the policy. The
policy below is *how the toolbox is used*.

### Layer B — Transport layer

The transport layer hides **provider-specific protocol
details** from the loop:

- **Streaming** — Anthropic SSE / OpenAI stream / Gemini
  stream. The adapter normalises a unified `Stream<Token>`
  for the loop to consume (if the projection asks for it).
- **Provider dialect** — Anthropic uses `cache_control`
  markers; OpenAI uses auto-cache; Gemini uses
  `cachedContentTokenCount`. The adapter translates each
  provider's response into the loop's unified `ModelUsage`
  schema (per `ledger.md` "10. telemetry").
- **Connection management** — keep-alive, pool sizing,
  graceful close on idle. Configurable per-provider.
- **Auth refresh** — bearer tokens with TTL refresh;
  rotation under a single in-flight request is the
  provider's problem, not the loop's.
- **Per-provider feature adapter** — extended thinking
  (`reasoning.thought`), tool choice rationale
  (`reasoning.tool_selection`), prompt caching — each is a
  **per-provider plug-in** that translates the provider's
  shape into the unified kind catalog.

The transport's invariant: **the loop never sees a
provider-specific name** (`anthropic.system`, `openai.developer`,
`gemini.system_instruction`). It sees only kinds.

### Layer C — Failure handling (the policy)

The loop skeleton observes **two** categories of failure
from `model.invoke`:

1. **Retry** — transient; the loop never knows.
2. **Recover** — semantic; the loop sees a single event and
   decides.

The boundary is hard: a retry never escapes `model.invoke`
as "the model returned a different result"; a recovery
never happens silently inside `model.invoke`.

#### Retry (transient) — local to `model.invoke`

```
match: {http_status: [500, 502, 503, 504]}
       {network_error: connection_reset, dns_failure, timeout}
       {http_status: 429}            # rate-limited
action:
  backoff: exponential
  base_ms:  500
  factor:   2.0
  jitter:   uniform(0, 500)         # decorrelates clients
  max_attempts: 5                   # 5xx
  max_attempts: 3                   # 429
  budget_per_attempt_ms: 2000       # 5xx
  budget_per_attempt_ms: 5000       # 429
  circuit_breaker: open_after(5 consecutive failures in 60s)
```

Retry is **scoped to `model.invoke`**. The loop never sees
the retry; if all retries fail, `model.invoke` returns a
single "transient" event for the loop to react to (which
itself goes through the recovery table — usually "halt the
round, surface to the user").

#### Recover (semantic) — loop skeleton observes

| Failure | HTTP / provider signal | Action |
|---------|------------------------|--------|
| **Input/context overflow** | Provider-specific status/body/finish signal mapped by the adapter | Return `Overflow` with verified limit evidence; Core decides whether to derive a smaller request. Do not assume every provider uses HTTP 413. |
| **max_tokens exceeded (output truncation)** | `truncated: true`, `usage.completion_tokens == max_tokens` | Send a **continue** request with the prior response as the prefix. Max 3 continues; if the third still truncates, **terminate** the round and emit `output.truncated` entry. |
| **Auth failure** | `401`, `403` | Halt the round. Emit `governance.approval_request(question="auth failed: reauth?")`. Do **not** retry — auth failures don't recover via backoff. |
| **Content policy violation** | Verified provider-specific signal | Return `ContentViolation`. The adapter does not rewrite prompts or intents; Agent/Runtime policy decides whether another request is allowed. |
| **Model unavailable** | 5xx exhausted; circuit-breaker open; model down for >30 s | **Failover to backup model**. The loop records the failover in `model.usage` (the failed-model entry is kept alongside the backup's success). The user's session continues with reduced capability. |

The recovery table is **declarative**: each entry has a
`match`, an `action`, and (optionally) a `fallback`. The
implementation reads this table and dispatches — no
ad-hoc per-call error handling in code.

## State machine — `model.invoke`

```
                ┌──────┐
                │ READY│
                └──┬───┘
                   │  call
                   ▼
              ┌──────────┐
        ┌────►│ INVOKING │
        │     └────┬─────┘
        │          │
        │   retry  │  5xx/429
        │          ▼
        │     ┌───────┐
        │     │BACKOFF│
        │     └───┬───┘
        │         │
        │         ▼
        │    (back to INVOKING)
        │
        │  2xx
        ▼
   ┌───────────┐
   │ RESPONDING│
   └─────┬─────┘
         │
    ┌────┴────────────────────────────────────────────┐
    │                                                  │
    ▼                                                  ▼
┌────────┐                                       ┌──────────┐
│  DONE  │                                       │ TRUNCATED│
└───┬────┘                                       └────┬─────┘
    │                                                 │
    │ ok                                              │ continue x 3
    ▼                                                 ▼
 success                                          (continue
                                                    loop)
    │
    │  4xx/443
    ▼
 ┌─────────┐
 │ ERROR_* │──► declarative policy table
 └─────────┘
```

The transport state machine is **internal to `model.invoke`**. At that layer,
requests finish in one of three broad classes:

1. **success** — `model.usage` row + `loop.receipt` row
2. **transient_exhausted** — backoff retries failed; the
   loop sees a single "transient" event (which the recovery
   table handles)
3. **classified_failure** — a verified provider signal is normalized to one
   of the outcome kinds below; the Runtime policy dispatches it

## AIMD — concurrency control

```
default concurrency: 4
on success:           concurrency += 1   # additive increase
on 429 / 5xx:        concurrency /= 2   # multiplicative decrease
clamp(1, 32)
```

The AIMD grows concurrency slowly (additive) but cuts it
fast (multiplicative). TCP's congestion control is the same
shape; it works because **the network can absorb a small
increase but is fragile to a large one**.

The runtime measures `in_flight` requests; when it crosses
the `concurrency` ceiling, new calls queue. AIMD adjusts
the ceiling based on success / rate-limit signals.

## Idempotency — the safety net

Every `model.invoke` carries a stable Runtime **`request_id`**
(per `ledger.md` "Dedup"). On retry:

- The adapter reuses the same `request_id` only for retries proven safe by the
  provider contract and an identical request digest.
- When a pinned provider contract explicitly supports an idempotency key, the
  adapter passes `request_id` through using that contract.
- Otherwise, Runtime deduplication only prevents a duplicate that has already
  reached a terminal durable result. A request left in `Started` is uncertain
  and is not replayed automatically.

Local deduplication does **not** prove that a provider avoided duplicate work
or billing. Billing reconciliation remains provider-specific evidence.
that is the load-bearing contract.

## Recovery strategy — declarative policy table

```yaml
# provider-adapter policy — declarative form
# One entry per failure mode. The adapter reads this and
# dispatches; no ad-hoc per-call error handling.
policies:
  retry:
    transient_5xx:
      match: {http_status: [500, 502, 503, 504]}
      action:
        backoff:     exponential
        base_ms:     500
        factor:      2.0
        jitter:      uniform(0, 500)
        max_attempts: 5
        budget_per_attempt_ms: 2000

    transient_429:
      match: {http_status: 429}
      action:
        backoff:     exponential
        base_ms:     1000
        factor:      2.0
        jitter:      uniform(0, 750)
        max_attempts: 3
        budget_per_attempt_ms: 5000

  recover:
    input_overflow:
      match: {normalized_outcome: Overflow}
      action:
        - record_overserved_max        # to ledger_meta
        - rebuild_surface             # re-derive, smaller surface
        - retry_once
        - if_fail: halt_round
        - emit_governance.approval_request(
            question: "output overflow; abort or split?")

    max_tokens_exceeded:
      match: {truncated: true,
              usage.completion_tokens == max_tokens}
      action:
        - send_continue_request
        - max_continues: 3
        - if_exceeded: terminate_round

    auth_failure:
      match: {http_status: [401, 403]}
      action:
        - halt_round
        - emit_governance.approval_request(
            question: "auth failed; reauth?")

    content_violation:
      match: {policy_violation: true}
      action:
        - emit_entry(kind=content.violation)
        - halt_round                   # policy may authorize a new request

    model_unavailable:
      match: {http_status: 5xx, retries_exhausted: true}
      action:
        - failover_to_backup_model
        - record_failed_model_in_model_usage
        - continue_round_with_backup

    circuit_breaker_open:
      match: {circuit_breaker_state: open,
              duration_open_s: > 30}
      action:
        - failover_to_backup_model
        - emit_ops_log(op=provider.cb_open)
```

The table is **runtime-readable**. Adding a policy for an existing normalized
outcome is a configuration change; adding a new outcome kind changes the typed
boundary and requires code and conformance tests.

## Cross-references

- `loop.md` "Derive in Detail (common path)" — `model.invoke`
  is the call-out from derive/assemble to the adapter.
- `loop.md` "The Turn State" — `phase = AwaitingApproval` for
  governance.approval_request emissions.
- `ledger.md` "Dedup" — stable Runtime request identity and digest.
- `ledger.md` "ledger_meta" — `overserved_max` recorded here.
- `compression.md` "Layer 3 — Overflow (`overserved_max`)" —
  the recovery action records to the same field the trigger
  reads.
- `assemble-testing.md` "Dim 1c — Real API smoke" — these
  failure modes are exercised by the smoke test.

## Outcome kinds — what `model.invoke` returns

`model.invoke` returns **one of these outcome kinds** to the
loop. The kind is the entire contract; the loop does not
parse the HTTP response, does not know the provider, and
does not see retry state.

| Outcome kind | Trigger | Carries | What the loop does |
|--------------|---------|---------|---------------------|
| `Completed(replay)` | HTTP 2xx, response not truncated, no auth/content issue | `text`, `reasoning.*`, `media.*`, `usage` | Normal — write `model.usage` + `loop.receipt` |
| `Overflow(learn_max)` | Verified provider-specific context/input-limit signal | `request_size`, optional verified limit evidence | Return the neutral outcome; Runtime records evidence and Core may rebuild a smaller surface. |
| `OutputTruncated(continuable)` | `truncated == true` AND `usage.completion_tokens == max_tokens` | `prefix_so_far`, `tokens_used` | Send a **continue** request with the prefix as the prior message. Max 3 continues; if the third still truncates, **terminate** the round and emit `output.truncated` entry. |
| `RateBudgetExhausted` | 429 + AIMD has dropped concurrency to 1 + still rate-limited | `retry_after_ms` (if header) | **Suspend** the round (`state.phase = AwaitingApproval`); emit `governance.approval_request("rate budget exhausted; wait or downgrade model?")`. The user / runtime picks. **Or** failover to a backup model immediately (configurable). |
| `PartialCancelled(reason, prefix_so_far)` | User / loop / harness cancels **mid-stream** | `prefix_so_far` (bytes already received) | **Partial result goes to the ledger** as a `provider.partial` entry. The round's state is suspended with the partial as the resume baseline. On resume: discard the partial and re-issue from the last clean anchor, **or** continue from the partial (configurable). |
| `AuthFailure(provider, reason)` | HTTP 401, 403 | `provider`, `reason` | Halt the round. Emit `governance.approval_request("auth failed: reauth?")`. Do **not** retry — auth failures don't recover via backoff. |
| `ContentViolation(reason)` | Verified provider-specific content-policy signal | `reason`, `violated_field` | Return the neutral outcome. Prompt reformulation, retry, or halt is an Agent/Runtime policy decision. |
| `ModelUnavailable(circuit_open_s, last_5xx)` | 5xx exhausted; CB open > 30 s | `last_model_id`, `circuit_open_s` | **Failover to backup model**. The failed-model row goes to `model.usage` (audit-visible); the backup's success is the round's actual response. |
| `CircuitBreakerOpen` | CB opened transiently, not exhausted | `provider`, `opened_at` | Failover to backup model **without retrying the original**. Same record pattern as `ModelUnavailable`. |

### Outcome as a sealed type

The outcome kinds above are **the runtime's contract** —
each is a constructor of an enum-like sealed type. There
are no ad-hoc fields; the kind's *tag* tells the loop how to
proceed, and the kind's *data* is exactly the payload the
loop needs.

```python
class InvokeOutcome(Enum):
    Completed          = auto()  # text, reasoning, media, usage
    Overflow           = auto()  # request_size, overserved_max
    OutputTruncated    = auto()  # prefix_so_far, tokens_used
    RateBudgetExhausted= auto()  # retry_after_ms
    PartialCancelled   = auto()  # reason, prefix_so_far
    AuthFailure        = auto()  # provider, reason
    ContentViolation   = auto()  # reason, violated_field
    ModelUnavailable   = auto()  # last_model_id, circuit_open_s
    CircuitBreakerOpen = auto()  # provider, opened_at
```

The loop's `match invoke_outcome:` is exhaustive over these **nine** kinds.
Adding a tenth is a `non_exhaustive_match`
warning in Rust / `@when` fall-through in Kotlin — the loop
catches new failure modes without missing a case.

### Why each kind is distinct

- `Completed` ≠ `OutputTruncated`. Truncation **looks like**
  success — the response is 200, the body is valid — but
  `truncated == true`. Treating it as success would silently
  drop the rest of the model's answer.
- `Overflow` ≠ `RateBudgetExhausted`. The former is a normalized input/context
  limit signal; 429 is commonly **rate limiting**
  (too many requests in a window). Different recovery.
- `ModelUnavailable` ≠ `CircuitBreakerOpen`. The CB can be
  open transiently (single 5xx wave); `ModelUnavailable` is
  the exhausted state. Different times to retry.
- `PartialCancelled` ≠ any failure kind. It's **a normal
  outcome** with a particular property: the response is
  partial. The loop should suspend rather than fail.

## Cancellation semantics — partial results ledger-挂账

When the user, the loop, or the harness **cancels** a
`model.invoke` mid-stream (e.g. user pressed stop, loop
hit a step timeout, harness aborted):

1. **Stop the request** at the transport layer (close the
   connection / cancel the streaming iterator).
2. **The bytes already received go to the ledger** as a
   `provider.partial` entry, **not discarded**:

   ```
   kind: provider.partial
   body: {
       ref:           round_id              # → loop.receipt
       reason:        user_cancel | timeout | harness_abort
       bytes:         <the partial bytes received so far>
       cancel_at_seq: <last_seq from the request>
       wall_ts:       <when cancelled>
   }
   ```

3. **The round's state** transitions to
   `phase = AwaitingConfirm` with the partial as the
   baseline. The next round either:
   - **Discards** the partial and re-issues from the last
     clean anchor (cheap, may lose work), or
   - **Continues from the partial** (preserves work, may
     drift from the original intent).
4. **Audit** — the partial entry is a permanent ledger row.
   A future "why did round N end mid-stream?" query is one
   read away.

### Why ledger-挂账, not in-memory

The partial is a **fact about the round** — what happened,
when, how much was received. In-memory would lose it on
crash; the ledger doesn't. The partial also feeds the
`memory_watermark` walk: a future dream pass can treat
recurring partial-cancellation patterns as a fact worth
extracting ("user cancels this round type after 2 s on
average").

### End-to-end principle

The provider doesn't assume client state; the client doesn't assume provider
state. **All resumable product state lives in Runtime durable storage.**
The `provider.partial` entry is the canonical record of a
mid-stream cancellation; on resume, the loop reads the
ledger, not the in-memory state, to decide what to do.

### Re-attachment (the "continue from partial" branch)

If the loop decides to continue from the partial:

1. Construct a **follow-up request** with the partial as the
   prior message:

   ```
   request = [
       ...prior messages...,
       model_reply_so_far,     # from provider.partial.bytes
       user: "continue"
   ]
   ```

2. This is a new request, not a transport-level continuation guarantee. The
   prior partial is included only when it forms a valid, policy-approved
   message and the adapter can encode it without inventing provider state.
3. The new response is appended to `provider.partial`'s `ref`
   chain (or a new `loop.receipt` carries both).
4. **`overserved_max` is NOT updated from a continue** —
   continue doesn't push the input past the model's ceiling.
   Only `Completed` and `Overflow` update it.

This follow-up branch is one policy option after a mid-stream cancellation;
the policy may instead halt, ask for approval, or begin a fresh round.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: **draft** — boundary and outcome categories are active; exact
  provider mappings, retry safety, and partial handling require evidence.

## Summary — current `model.invoke` shape

This doc covers everything between `derive → assemble` and
the **provider's wire API**. The ownership shape is settled; provider details
remain open until verified against pinned official sources:

### Already in this doc (8 layers)

1. **Two protocols** — event stream (lossy realtime for UI /
   monitor) + ledger stream (lossless durable records). The
   names are **design properties**, not a specific transport.
2. **Retry vs Recover** — retries are local to `model.invoke`;
   semantic failures bubble up to the loop skeleton as a
   single outcome event.
3. **InvokeOutcome** — 9 sealed-type outcome kinds
   (`Completed`, `Overflow`, `OutputTruncated`,
   `RateBudgetExhausted`, `PartialCancelled`, `AuthFailure`,
   `ContentViolation`, `ModelUnavailable`,
   `CircuitBreakerOpen`).
4. **Cancellation semantics** — mid-stream cancel writes
   `provider.partial` to the ledger (the bridge between the
   two protocols).
5. **AIMD + circuit breaker** — per-pool concurrency control.
6. **Idempotency** — stable `request_id` + digest deduplicate Runtime facts;
   provider replay still depends on official idempotency support.
7. **Dead-flow detection** — TTFT / inter-chunk / call-budget
   timers; **suspending recovery** (user leaves) + **liveness
   probe** (provider down).
8. **Multi-model dispatch + per-provider concurrency pools** —
   `role → (provider, model_id)` table; per-pool AIMD;
   global cap + priority queue; **`request_id`** ties
   `model.usage` + `loop.receipt` + provider log + telemetry.

### Round-trips between providers

Provider failure kinds are split into **two distinct
boundaries**:

- **Transport/provider failures** (provider-specific overload or overflow
  signals, truncation, cancellation, model unavailability) live in
  **`provider-adapter.md`** and
  are recovered by the **5-strategy declarative policy
  table**. The recovery is **declarative** (YAML) and
  **runtime-readable**.
- **Semantic output failures** (model returns **invalid
  arguments** to a tool call, schema mismatch, missing
  fields) are **not** in this doc — they are recovered at
  the **effect layer** by either `governance.judge`
  rejecting the intent or by feeding the error back to the
  model so it regenerates. The boundary is load-bearing:
  `provider-adapter` does **not** retry on bad model output,
  because the model is the one that produced the bad output
  — re-issuing the same call would reproduce the bug. The
  effect layer is the right place to make the model own its
  output.

### Design shape (one sentence)

> **`model.invoke` is a streaming caller (transport) with
> liveness + budget discipline, backed by a declarative
> recovery-policy table (resilience), dispatching per
> `role → (provider, model_id)` (economy), and threading a
> single `request_id` (trace) through every tier.**

The shape is:

- **Transport** — the streaming caller + the 9 outcome kinds
- **Resilience** — the 5-strategy recovery policy table
- **Economy** — the per-role dispatch table + AIMD pools
- **Trace** — `request_id` across provider, ledger, telemetry

### Test impact

The `assemble-testing.md` "Dim 1c — Real API smoke" exercises
the transport half. The role dispatch table + concurrency
pool tests live alongside it (forthcoming — same harness).
The `loop.md` "Two protocols" design asserts the
event-stream / ledger-stream split; `provider-adapter.md`
provides the implementation contract.
