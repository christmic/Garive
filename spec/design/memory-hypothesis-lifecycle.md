# M1 — Governed memory hypothesis lifecycle

## Status

Accepted implementation contract extending M0. M0 remains the durability and
compatibility foundation; M1 never silently reinterprets an M0 revision.

## Purpose and ownership

M1 makes durable memory a bounded hypothesis library that can be classified,
recalled, tested against committed outcomes, distilled, promoted, and forgotten.
The architecture research in `docs/architecture/core/memory.md` motivates this
contract; this document is normative where it proposes concrete mechanisms.

- Engine owns values, registry validation, deterministic reduction and budgets.
- Runtime owns authentication, namespaces, clocks, stores, indexes, scheduling,
  erasure propagation and durable effect ordering.
- Knowledge owns published project/organisation truth. Memory can only propose
  promotion.
- Core derive owns context selection. Memory never overrides current input,
  system policy or authority decisions.

Engine reads no environment, database, clock, network, embedding or random state.

## Classification and M0 compatibility

Classification has two independent axes:

```text
MemoryType = Semantic | Episodic | Lesson | Procedural
MemoryRole = Preference | Constraint | Decision | LearnedFact | Summary
```

Type determines lifecycle/recall policy; role preserves M0 content meaning. A
versioned registry admits exact combinations. Unknown strings cannot become
executable configuration.

M0 imports explicitly: Preference/Constraint/Decision/LearnedFact become the
same role under Semantic; Summary becomes Episodic/Summary. Import authority is
`AgentLearned` unless Runtime binds an explicit user-command receipt. Wording
never proves authority.

```text
MemoryTypeDescriptor {
  type, allowed_roles, admitted_authorities
  lifecycle_policy_revision, recall_profile_revision
  retention_policy_revision, surface_kind
}
MemoryTypeRegistry { registry_revision, exactly_one_descriptor_per_type }
```

Adding a type requires a versioned enum, descriptor, policies and fixtures; a
registry row selects admitted code but cannot inject code.

## Authority and scope

```text
MemoryAuthority = UserDeclared | AgentLearned | OrganisationPublished
MemoryScopeClass = Session | AgentInstance | User | Project | Platform
```

`UserDeclared` requires a Runtime-verified user-command receipt. An Agent may
request confirmation but cannot construct that authority. `AgentLearned` is a
hypothesis even after repetition. `OrganisationPublished` requires an external
publication receipt and is never produced by extraction or aggregation.

Scope identifiers remain opaque Runtime-authorized namespaces. Namespace and
scope filtering is mandatory before scoring; Platform additionally requires an
aggregation-policy digest. Authority affects conflict presentation, not truth.
Restricted access retains M0 frozen-grant semantics. Project memory remains a
hypothesis until Knowledge accepts a promotion.

## Hypothesis lifecycle

M0 revision state (`Active`, `Superseded`, `Tombstoned`) governs retrievability.
M1 adds an orthogonal state:

```text
HypothesisState = Candidate | Active | Cold | Archived | Promoted
EvidenceTally { verified: u64, falsified: u64, neutral: u64 }
MemoryLifecycle {
  state, tally, last_observed_position
  promoted_knowledge_receipt_digest?
}
```

Floating confidence is not portable state. A versioned calibration derives a
display score from the exact tally; it is never permission or authority.

- agent-learned entries start Candidate;
- accepted verification may move Candidate to Active;
- explicit policy may move Active → Cold → Archived and reactivate Cold;
- a Knowledge publication receipt alone permits Promoted;
- supersession/tombstone ends transitions for that revision.

Lessons have no time-only tombstone, but remain subject to user forget,
falsification, supersession, corruption handling and legal erasure. Procedural
entries bind a toolchain revision and fall back to Candidate/Cold when it changes.
Every transition checks one exact prior revision and committed observation;
overflow, duplicates, stale revisions and illegal transitions fail closed.

## Four-decision writes

Extraction emits a bounded `MemoryCandidate`, not trusted memory. A maintenance
decision is exactly `Add`, `Update(expected_revision)`, `Delete(exact_revision,
reason=explicit_forget)`, or `Noop(safe_code)`. All decisions are durable. Delete requests
an M0 tombstone; physical erasure is a separate receipt. Noop stores no rejected
sensitive content.

Admitted sources are explicit user command, session-end episode, exit-summary
proposal, and scheduled distillation. Each binds an extractor revision and
bounds. Model text alone is not correctness evidence. User writes still pass
authorization, retention, sensitivity and durability checks.

## Menu and detail recall

Menu and detail are separately bounded, committed products. A menu contains
only record/revision, type, role, authority, state, safe label, content digest
and evidence count. Detail extends M0 `MemoryQuery` with allowed types, roles,
states, selection-policy revision and an optional exploration seed.

Runtime commits exact ordered results before model visibility; restart reuses
them. Menu labels are redacted descriptors, not content. No fixed surface
percentage is normative: effective snapshots carry item, byte and token bounds.
At most one menu and one detail query occur per Kernel iteration.

The baseline selection uses integer relevance/recency/importance scores and
lexical identity tie-breaks. Vector, FTS, RRF and rerank are versioned Runtime
ports. Stochastic selection is forbidden unless the request freezes an algorithm
revision and seed and the committed result records selected identities/draws.

UserDeclared preferences may be mandatory bounded derive candidates but remain
below system/policy/current-turn input. Other memory details require an explicit
request.

## Observation and reality feedback

A model citation `[mem:id]` is an application claim, not verification. Runtime
may open a bounded obligation binding the revision, application fact, expected
outcome, scope and expiry. An observation is Verified, Falsified(in_scope), or
Neutral and must cite committed tool/test/effect/user-correction evidence or an
admitted deterministic verifier. A response, citation or missing error is not
evidence.

In-scope falsification increments the tally. Out-of-scope failure does not; it
may create a narrowed-scope candidate, while the old revision changes only by
explicit supersession. Attribution policy is versioned and cannot be supplied
by the model benefiting from it.

Observation and extraction are asynchronous Runtime branches. Their failure
cannot rewrite or block a completed turn; later visibility still follows
commit-before-context.

## Distillation, promotion, quota and forget

```text
MemoryCandidate {
  candidate_id, namespace_id, extractor_revision
  source: ExplicitUserCommand | SessionEnd | ExitSummary |
          ScheduledDistillation
  intent: Learn {memory_type, role, authority, scope,
                 content, evidence, content_bytes}
          | Forget {record_id, revision_id, authority}
}

AdmissionAssessment {
  generalizable: bool
  stability: Confirmed | Uncertain
  exact_duplicate_revision_id?
  conflicting_active_revision_id?
}

MaintenanceDecision =
  Add {proposal_id}
  | Update {proposal_id, expected_active_revision_id}
  | Delete {command_id, record_id, revision_id, reason=explicit_forget}
  | Noop {code: not_generalizable | unstable_deferred | duplicate}
```

Explicit user Learn/Forget requires `UserDeclared` authority and its receipt.
All three automatic sources require `AgentLearned`; no extraction source can
produce `OrganisationPublished`. Forget alone produces Delete. Learn evaluates
generalizability, then stability, then exact duplicate, then conflict, in that
order; otherwise it produces Add. Duplicate and conflict cannot both be set.
The reducer only produces a decision: M0 authorization and atomic write/tombstone
remain required before state changes.

```text
DistillationWatermark {
  extractor_revision, session_id, through_position, batch_digest
}

MemoryAuditPolicy {
  max_active_records, max_active_bytes, stale_after_positions,
  low_use_threshold, max_report_items
}

MemoryAuditEntry {
  record_id, revision_id, memory_type, state, content_digest,
  content_bytes, use_count, last_verified_position,
  retention_score_basis_points
}
```

One candidate carries at most 64 ordered evidence references. One audit accepts
at most 4096 canonical inventory rows and 4096 canonical contradiction pairs;
all output, including required actions, fits `max_report_items` or fails with
`limit_exceeded`. These protocol constants are identical in Rust and Kotlin.

A watermark advances only within one extractor revision and Session. Repeating
the exact position/digest is replay; changing the digest at that position or
moving backwards is conflict. Audit output deterministically reports duplicate
digests, explicitly supplied contradiction pairs, stale/low-use identities and
the minimum score-ordered Cool proposals needed to satisfy the active hot-set
count/byte quota. Candidate and Promoted entries are never audit-eviction
targets. Active entries may only be proposed Cold; stale Cold entries may only
be proposed Archived.
No audit action deletes, supersedes, promotes, or writes a record.

```text
MemoryPromotionPolicy {
  revision, allowed_types, min_verified, max_falsified,
  min_helpful_uses
}

MemoryPromotionRequest {
  request_id, namespace_id, record_id, revision_id, memory_type,
  policy_revision, knowledge_proposal_id, evidence_digest
}

MemoryPromotionReceipt {
  request_id, knowledge_proposal_id, knowledge_record_id,
  knowledge_revision_id, receipt_digest
}
```

Only Active or Cold memory can request promotion. The portable reducer checks
the frozen policy, exact lifecycle tally and committed helpful-use count, then
produces an opaque Knowledge proposal binding; it does not publish Knowledge.
Candidate, Archived and already Promoted memory fail with
`promotion_not_eligible`. Threshold failure has the same safe code and exposes
no content. A promotion receipt is accepted only when its request and proposal
identities match. The receipt and the resulting lifecycle transition commit
atomically; absence or mismatch fails before Promoted becomes visible.

```text
MemoryErasureRequest {
  request_id, namespace_id, record_id, revision_id,
  tombstone_fact, policy_revision,
  targets: [{target_id, kind: PrimaryStore | Projection | Cache | Backup}]
}

MemoryErasureTargetResult {
  target_id,
  status: Erased | NotPresent | PendingBackupRetention | PendingRetry,
  receipt_digest,
  not_before_position? // required only for PendingBackupRetention
}

MemoryErasureReceipt {
  request_id, attempt_id, attempted_at_position,
  results, disposition: Complete | Partial
}
```

An erasure request is valid only after the exact M0 tombstone fact is committed;
Runtime verifies that reference binds the same namespace/record/revision. Target
IDs are canonical and come from explicit Garive configuration. Every attempt
reports every requested target exactly once in target order. `Complete` is
derived only when all targets are Erased or NotPresent. Backup retention and
retryable failure remain `Partial`; a backup-pending result must state a later
position. Erasure receipts never reverse the tombstone and never make content
model-visible again.

- Distillation binds an exact ledger prefix, watermark and extractor revision;
  replay is idempotent.
- Per-namespace/type count and byte quotas are explicit snapshot values.
  Overflow proposes maintenance and never silently deletes.
- Lint emits a bounded audit report and has no write authority.
- Promotion creates a Knowledge proposal. Only its committed publication receipt
  moves memory to Promoted; normal recall then excludes it.
- Forget tombstones retrieval immediately and starts Runtime erasure propagation.
  Completion requires an erasure receipt and reports backup limitations.

Fixed quotas, verification counts, schedules, latency promises, confidence
thresholds and Thompson sampling are non-normative until reproducible evaluation
admits a policy revision.

## Persistence and durable facts

Ledger facts remain semantic SSOT. `memory.db` may be an encrypted, versioned,
rebuildable cross-session projection/index, never the sole owner of a mutation.
External memory services require idempotent receipts bound into ledger facts.
Projection loss rebuilds from admitted prefixes; mismatch fails closed.

M1 reserves versioned fact families for candidate/maintenance decisions,
lifecycle transitions, recall results, obligations/observations, distillation
checkpoints, promotion request/receipt and erasure request/receipt. Exact schemas land with
each behavior slice; older projections treat unknown facts as opaque.

Stable additions are `unknown_memory_type`, `authority_receipt_required`,
`scope_policy_denied`, `invalid_transition`, `duplicate_observation`,
`attribution_unsupported`, `selection_unreplayable`, `projection_stale`, and
`promotion_receipt_required`, and `promotion_not_eligible`.

## Delivery and evidence

1. M1-A registry, classification, authority/scope and M0 import;
2. M1-B lifecycle/tally reducer with shared Rust/Kotlin fixture;
3. M1-C menu/detail selection and replayable exploration;
4. M1-D obligation/observation and scope narrowing;
5. M1-E Runtime facts, SQLite projection, restart and isolation;
6. M1-F distillation, audit, promotion and erasure receipts;
7. M1-G derive integration and pinned recall-quality evaluation.

Portable slices require strict Rust evidence plus Kotlin semantic conformance.
Runtime claims require real SQLite restart/process-kill tests. Quality/latency
claims require a pinned dataset and reproducible configuration.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
