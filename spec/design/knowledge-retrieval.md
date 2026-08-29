# K0 — Attributed Knowledge retrieval

## Status

Accepted implementation contract in the Agent capability set.

## Scope and distinction from Memory

K0 defines exact source descriptors, bounded retrieval requests, evidence
chunks and citations. Engine owns portable validation and reduction. Runtime
owns connectors, indexes, credentials, network policy, caching, freshness and
durable request/result facts.

Knowledge is externally sourced evidence. It is not automatically true, does
not become Memory, and grants no authority to execute a tool or access a
connector. Memory records product-authorized retained statements; Knowledge
records what an exact source returned for one request.

## Exact source descriptor

```text
KnowledgeSourceDescriptor {
  source_id, source_revision
  kind: Repository | Documentation | Dataset | SearchIndex | Service
  content_domain
  trust_class: Curated | FirstParty | ThirdParty | Untrusted
  supported_query_modes: non-empty set of Keyword | Semantic | Structured
  freshness_policy_digest
  citation_scheme: UriFragment | DocumentOffset | RecordKey | OpaqueLocator
  capability_metadata_digest
}
```

Descriptors contain no endpoint, credential, client, local path or secret.
They are resolved exactly into the effective Agent snapshot. Runtime binds an
authorized connector and enforcement policy to each descriptor.

## Retrieval request

All IDs are non-empty typed opaque values. `KnowledgeRequestId` is distinct
from model and tool request identities.

```text
KnowledgeRequest {
  request_id
  source_id, source_revision
  mode
  query: ContentBinding
  filters: ordered portable equality/range filters
  through_position
  max_chunks: non-zero u32
  max_total_bytes: non-zero u64
  deadline_budget_ms: non-zero u64
  freshness_requirement: CachedAllowed | Revalidate | ExactSnapshot(digest)
}

KnowledgeFilter {
  field: non-empty bounded string
  operator: Equal | LessThan | LessThanOrEqual | GreaterThan |
            GreaterThanOrEqual
  value: null | bool | integer | bounded string
}
```

Filters use a strict integer/string/bool/null I-JSON subset with unique keys;
unknown operators fail closed. A deadline bounds waiting but does not prove
whether a remote request ran.

Filters are encoded as one L0-canonical JSON array in request order.
`request_digest` is lowercase SHA-256 over canonical JSON containing contract
`garive.knowledge-request`, version `1`, source ID/revision, mode, query,
filters, through-position, all bounds and freshness requirement. Request ID is
excluded because its typed outer identity owns idempotency; changed semantics
under the same ID conflict.

## Evidence and citation

```text
KnowledgeEvidence {
  evidence_id
  source_id, source_revision
  source_snapshot_digest?
  content: ContentBinding
  citation: Citation
  retrieved_at_utc
  freshness: Fresh | Cached | Stale
  trust_class
  rank_basis_points: 0..10000
}

Citation {
  locator_kind
  locator: non-empty string
  title?
  canonical_uri?
  content_digest
}
```

Runtime sanitizes locators and verifies that the citation content digest binds
the returned content. Credentials, request headers, private connector errors
and unrestricted local paths never enter Engine or model context. A citation
is attribution, not proof that a claim is correct.

`KnowledgeResult` is `Completed { ordered_evidence, truncated }`,
`Unavailable { retry_after_ms? }`, `Rejected { code }`, `Unsupported`, or
`Uncertain { code }`. Completed may be empty. Result order is connector order
only when the exact descriptor declares it stable; otherwise Runtime normalizes
by descending connector rank basis points, citation locator and evidence ID.

## Durability and crash behavior

Runtime commits `knowledge.requested` before crossing a connector boundary.
It commits exactly one terminal `knowledge.completed` or `knowledge.failed`
before evidence enters the next model request. Completed binds the exact
ordered evidence IDs, content/citation digests, freshness, bounds and
truncation.

After a crash:

- terminal result: return it idempotently;
- prepared but not dispatched: same-ID dispatch is permitted after policy
  revalidation;
- dispatched with no trustworthy result: classify `Uncertain`; retry only when
  the connector proves read-only/idempotent semantics and policy allows it;
- stale cache never satisfies `Revalidate` or `ExactSnapshot` silently.

The model request that consumes evidence binds the committed knowledge result
position through the existing C6 fixed-prefix rules.

## Context and output integration

At most one request per source and a frozen total request count may occur per
Kernel iteration. Returned evidence becomes optional attributed C2 candidates.
Each candidate carries source/evidence/citation identities so later response
assembly can preserve citations. The model may omit or misstate citations;
Runtime/UI must distinguish cited evidence from verified product truth.

K0 does not define an autonomous browsing loop. Model-proposed arbitrary URLs
or queries must pass an admitted tool/governance path or a separately frozen
Knowledge query policy.

## Durable facts

The coordinated C6F amendment must define:

- `knowledge.requested`: request/source identity, canonical request digest,
  fixed prefix, bounds and freshness requirement;
- `knowledge.completed`: ordered evidence/citation bindings, snapshot/freshness
  data and truncation;
- `knowledge.failed`: stable class, ambiguity flag and retry hint.

Request and terminal identities cannot be reused with changed query, source,
filter, freshness or bounds.

## Stable failures

`invalid_query`, `source_not_found`, `source_revision_mismatch`,
`source_denied`, `filter_unsupported`, `freshness_unavailable`,
`connector_unavailable`, `connector_rejected`, `retrieval_uncertain`,
`citation_invalid`, `content_digest_mismatch`, `limit_exceeded`,
`durability_failure`, and `corrupt_knowledge_state`.

## Acceptance evidence

- shared Rust/Kotlin descriptor/query/evidence/order/failure fixtures;
- unknown filter, citation and digest negative matrices;
- fake Runtime connector proves request-before-dispatch and
  result-before-model ordering;
- real SQLite restart tests at prepared/dispatched/terminal positions;
- credential/redaction and namespace/source isolation tests;
- no HTTP, filesystem, credential or index implementation in Engine Knowledge.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
