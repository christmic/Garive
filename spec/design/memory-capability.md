# M0 — Governed durable Memory capability

## Status

Draft implementation contract in the Agent capability review set.

## Scope and ownership

M0 defines portable memory proposals, immutable records, bounded retrieval and
supersession/tombstone semantics. Engine owns validation and deterministic
reduction. Runtime owns namespace authorization, persistence, encryption,
retention, cross-Session access and storage receipts.

Memory is selected evidence for later context, not a serialized Agent mind and
not a second Session ledger. A model statement is never automatically trusted
or persisted as memory.

## Identities and scopes

All identities are non-empty opaque typed values:

- `MemoryNamespaceId`: Runtime-authorized privacy/retention boundary;
- `MemoryRecordId`: stable logical record identity;
- `MemoryRevisionId`: one immutable revision;
- `MemoryQueryId`: one logical bounded retrieval;
- `MemoryProposalId`: one candidate write decision.

`MemoryScope` is `Session`, `AgentInstance`, or `Namespace`. Session and Agent
Instance scope require their exact typed owner. Namespace scope never exposes
an authenticated user identifier to Engine; Runtime supplies only an opaque
authorized namespace.

```text
DurableFactReference {
  session_id, position, fact_id, payload_digest
}
```

Runtime verifies all four fields against the fixed durable prefix. Cross-Session
references require namespace authority and never imply that one Session owns
another Session's facts.

The D0 capability descriptor binds supported scopes, record kinds, maximum
record/query bounds and an exact `retriever_revision`. Runtime may replace an
index implementation only when it preserves that revision's scoring contract;
otherwise a new Turn snapshot is required.

## Immutable record

```text
MemoryRecord {
  record_id, revision_id, namespace_id, scope
  kind: Preference | Constraint | Decision | LearnedFact | Summary
  content: ContentBinding
  evidence: non-empty ordered DurableFactReference[]
  status: Active | Superseded | Tombstoned
  sensitivity: Ordinary | Restricted
  confidence_basis_points: 0..10000
  valid_from_position
  supersedes_revision_id?
  expires_at_utc?
}
```

`ContentBinding` uses C6F digest plus exact inline/reference rules. Evidence
references identify an existing Session fact and position; Runtime verifies
ownership and digest before commit. Confidence is provenance metadata, not a
truth probability and never overrides policy or contradictory current input.

Revisions are append-only. Supersession names the exact active prior revision.
A tombstone removes content from future retrieval while preserving a redacted
audit fact; physical erasure and backup propagation are Runtime privacy
operations outside portable M0.

## Write proposal and authority

```text
MemoryProposal {
  proposal_id, namespace_id, scope, kind
  content, evidence, sensitivity, confidence_basis_points
  expected_active_revision_id?
}

MemoryWriteDecision =
  Commit { record_id, revision_id, retention_policy_digest }
  | Reject { code }
  | RequireInteraction { requirement }
```

Core may propose only from facts included through its fixed durable position.
Runtime validates actor/namespace authority, retention, sensitivity, evidence
ownership, size and optimistic revision. A proposal has no authority. Required
interaction follows C5/C6 suspension and cannot be converted into an implicit
approval.

M0 v1 stores committed record truth as Runtime durable facts. A future external
memory service must add receipt-backed persistence; a successful HTTP response
alone cannot replace the facts.

## Retrieval

```text
MemoryQuery {
  query_id, namespace_id
  allowed_scopes: non-empty set
  purpose: Context | Planning | ConflictCheck
  retriever_revision
  query: ContentBinding
  through_position
  max_results: non-zero u32
  max_total_bytes: non-zero u64
  include_restricted: bool
}

MemoryMatch {
  record_id, revision_id, kind, content, evidence
  relevance_basis_points: 0..10000
  sensitivity
}

MemoryResult = Completed { ordered_matches } | Unsupported | Failed { code }
```

`query_digest` is lowercase SHA-256 over L0 canonical JSON containing contract
`garive.memory-query`, version `1`, namespace, canonical allowed scopes,
purpose, retriever revision, query ContentBinding, through-position, both
bounds and restricted flag. `query_id` is excluded because the outer typed
identity owns idempotency; changing any semantic field while reusing it is a
conflict.

Runtime returns only active, unexpired, authorized revisions visible through
the fixed query position. Restricted records require an explicit frozen grant;
`include_restricted=true` is a request, not authority.

Portable ordering is descending relevance, then descending
`valid_from_position`, then lexical record/revision identity. Results are
truncated before return to satisfy both limits. Equal input and fixed durable
prefix under one retriever revision produce equal scores and ordering. The
retrieval implementation need not expose its index representation, but an
embedding/model/network-backed retriever is a separate Runtime port with
explicit configuration and cannot be discovered inside Engine.

## Context integration

At most one bounded Memory query is issued per Kernel iteration in v1. Runtime
commits `memory.retrieval_recorded` with query digest, returned revision IDs,
content digests, fixed through-position and truncation before any returned
content enters a model request. C2 then treats each result as an optional
attributed context candidate. Current trusted input, required system facts and
explicit policy constraints outrank Memory.

A restart reuses the committed retrieval result when the exact query digest
matches. It does not silently rerun a changed index and claim the same model
request semantics.

## Durable facts

The coordinated C6F amendment must define:

- `memory.proposed`: proposal ID, namespace, scope, content/evidence digest;
- `memory.committed`: record/revision, full content binding, evidence,
  sensitivity, confidence, retention digest;
- `memory.rejected`: proposal ID and stable rejection code;
- `memory.superseded`: exact old/new revision binding;
- `memory.tombstoned`: record/revision and safe reason;
- `memory.retrieval_recorded`: query digest, ordered returned bindings,
  fixed prefix, bounds and truncation.

A direct proposal/commit or proposal/reject decision is atomic. A proposal that
requires interaction commits with the C5 request/suspension and reaches a later
decision only through an exact continuation. Supersession commits atomically
with the new revision. No model-visible memory content exists only in an
in-memory cache.

## Stable failures

`invalid_memory`, `namespace_denied`, `evidence_not_found`,
`evidence_mismatch`, `revision_conflict`, `retention_rejected`,
`sensitivity_denied`, `limit_exceeded`, `unsupported`,
`durability_failure`, and `corrupt_memory_state`.

Diagnostic text and retrieval scores are not compatibility keys.

## Acceptance evidence

- shared Rust/Kotlin validation, ordering, supersession and failure fixtures;
- property tests for deterministic bounds and no tombstoned/expired leakage;
- Runtime tests for namespace isolation and restricted access;
- SQLite restart tests proving commit-before-context and exact retrieval replay;
- conflicting proposal/revision tests commit no partial facts;
- no environment, network, database or embedding dependency in Engine Memory.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: draft
