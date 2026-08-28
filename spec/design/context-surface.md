# C2 — deterministic context surface

## Status

Accepted focused contract under `agent-execution-contract.md`.

## Responsibility

Derive one purpose-specific, bounded, provider-neutral context surface from a
strictly ordered set of Runtime-selected durable candidates. C2 performs no
storage I/O, tokenization, summarization, provider rendering or persistence.

## Ubiquitous values

- `DurablePosition`: non-zero, monotonically increasing position inside one
  Session fact stream.
- `FactRef`: Session identity plus durable position; content-addressed payloads
  may add an opaque reference but do not change ordering.
- `ContextPurpose`: `Inference`, `Governance`, `ToolPreparation`, or
  `Summarization`.
- `Retention`: `Required` or `Optional`.
- `Visibility`: `Visible`, `Redacted`, or a non-empty set of allowed purposes.
- `ContextCandidate`: reference, kind, retention, visibility and ordered
  provider-neutral input items.

Candidate kinds are `Instruction`, `UserInput`, `ModelOutput`,
`ToolObservation`, `Approval`, `Summary`, and `SystemNotice`. Kind names are
semantic facts, not database table names.

## ContextRequest

```text
ContextRequest {
  session_id,
  turn_id,
  purpose,
  after_position,
  through_position,
  max_items,
  max_utf8_bytes,
}
```

- identities are non-empty;
- `max_items` and `max_utf8_bytes` are non-zero;
- `after_position` is an exclusive lower bound when present;
- `through_position` is non-zero and is greater than `after_position` when the
  latter is present;
- the request contains no SQL, provider model or transport values.

UTF-8 byte length is the C2 cross-language budget. It is not a token estimate.
Provider token budgeting is applied later by assembly using an admitted
provider counter.

## Candidate admission

Input candidates must be strictly increasing by `DurablePosition` and unique.
A candidate at or below `after_position` is ignored and reported as filtered.
Malformed order, duplicate reference, empty required content or an unknown kind
fails the derivation; C2 never silently repairs storage order.
Any supplied candidate beyond the frozen `through_position`, or belonging to a
different Session, is a port/invariant failure rather than concurrent data C2
may ignore.

Purpose filtering occurs before budgeting:

- `Visible` is eligible for every purpose;
- purpose-limited content is eligible only for a named purpose;
- `Redacted` emits a reference-only `RedactedItem` and never exposes the old
  content or redaction reason to the model.

A `RedactedItem` consumes one item and zero content bytes. For an ordinary
`ModelInputItem`, UTF-8 cost is the sum of every string payload field, excluding
enum tags and collection structure; an opaque JSON string therefore includes
its own punctuation bytes. A custom `MediaKind.Other` name is a payload field.

The result records filtered references so Runtime can explain omissions without
placing hidden content in the model surface.

## Deterministic budget algorithm

1. Validate the request and strict candidate order.
2. Apply `after_position` and purpose visibility filtering.
3. Convert redacted candidates to zero-content reference placeholders.
4. Compute each eligible candidate's item count and exact UTF-8 byte cost.
5. Admit every `Required` candidate.
6. If required candidates exceed either limit, return
   `RequiredFactsExceedBudget` with required totals; do not truncate them.
7. Traverse optional candidates newest to oldest, admitting a candidate only
   when all its items fit both remaining limits. A candidate is atomic.
8. Sort admitted candidates back into ascending durable order.
9. Return admitted items, retained/dropped/filtered references and exact totals.

All reference lists are returned in ascending durable-position order.

Skipping one oversized optional candidate does not stop examination of older,
smaller candidates. The complete dropped-reference list makes this behavior
auditable.

## ContextSurface

```text
ContextSurface {
  purpose,
  from_position,
  through_position,
  items,
  retained_refs,
  dropped_refs,
  filtered_refs,
  item_count,
  utf8_bytes,
}
```

Items retain their candidate and intra-candidate order. Empty surfaces are
valid only when no required candidate was eligible. The surface is immutable
and does not carry raw ledger rows.
`from_position` is one when `after_position` is absent, otherwise
`after_position + 1`; `through_position` is copied from the frozen request.

## Minimal ledger read port

Runtime implements:

```text
read_context_candidates(session_id, after_position, through_position)
  -> ordered ContextCandidate stream
```

The port returns exact immutable facts or typed storage/corruption errors. Core
does not issue SQL, choose indexes, page by wall clock, or ask the store to
apply model-purpose policy. Runtime freezes `through_position` before execution
so concurrent appends do not change one derivation.

## Shared semantic fixture

`spec/fixtures/agent/context-surface.json` covers:

- every candidate kind, including required instructions, while preserving
  durable and intra-candidate order;
- all four context purposes through positive admission and exclusion cases;
- strict ordering and duplicate rejection;
- exclusive lower-bound filtering;
- purpose inclusion/exclusion and redaction;
- required-over-budget failure;
- optional newest-first retention with chronological output;
- atomic multi-item candidates and UTF-8 byte accounting;
- empty eligible surface and exact audit reference lists.

Native boundary tests additionally cover every invalid request field, Session
mismatch, zero/beyond-surface positions, empty required instructions, empty
purpose sets, and byte accounting for text, media, tool observations, and
reasoning references. Rust evidence lives in
`engine/core/tests/context_surface.rs`; Kotlin evidence lives in
`experiments/engine-kt/core/src/test/kotlin/com/garive/eng/kt/core/`.

Rust and Kotlin must consume every case. Canonical serialized bytes are not a
C2 contract; normalized semantic fields are compared.

## Properties

For every valid request/candidate set:

- output references are unique and strictly increasing;
- output totals never exceed limits;
- every eligible required reference is retained;
- retained, dropped and filtered reference sets are disjoint;
- splitting/combining text content inside the same input item does not change
  its admission, audit references or byte cost; the preserved item structure
  itself remains input-owned;
- re-running with identical values produces an equal result;
- at fixed limits, no dropped optional candidate can replace a retained older
  candidate without violating the newest-first traversal or a limit.

The retained-reference set is intentionally not monotonic as a budget grows:
a newly fitting, newer atomic candidate may displace an older one. Required
references remain monotonic because they are always admitted or derivation
fails before optional selection.

## Acceptance

- Rust/Kotlin fixture and native property tests pass;
- C2 has no SQL/HTTP/provider dependency;
- unknown or malformed input fails closed;
- required content is never silently truncated;
- the surface can be used by C3 without reading Runtime state directly.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
