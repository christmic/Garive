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

## Architecture vocabulary reconciliation

The architecture deep-dive predates parts of this contract. The following
mapping is exact; aliases do not add states, permissions, schedules, or hidden
read paths:

| Architecture phrase | M1 meaning |
|---|---|
| hot capture | asynchronous `ExitSummary` Candidate proposal |
| explicit remember | authorized `ExplicitUserCommand`; wording alone proves no authority |
| session-end memory | bounded `SessionEnd` Candidate or durable Noop |
| dream | `ScheduledDistillation` over one exact prefix and watermark |
| confidence | exact `EvidenceTally`; optional versioned display calibration only |
| graduated | `Promoted` after an exact Knowledge publication receipt |
| retired | no M1 state; use explicit Cold/Archived policy or M0 supersession/tombstone |
| vector / FTS / recency | replaceable Runtime candidate ports before deterministic selection |
| risk-action recall | outside M1 until a Governance × Memory Spec admits the full contract |

Fixed clocks, percentages, fusion weights, confidence thresholds, automatic
scope rewrites, and Thompson-style exploration are research. Stochastic
exploration is permitted only under the frozen-request and committed-result
rules below; no particular algorithm is selected here.

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

The committed-result adapter is defined by C2. It binds the recall fact and
turn window, validates product/lifecycle/content invariants, and emits one
optional atomic Memory candidate. Direct insertion after C2 is prohibited:
Memory must be visible in C2 retained/dropped references and charged against
the same item and UTF-8 budgets as durable history. Provider assembly may move
an admitted Memory envelope ahead of ordinary history/current input solely to
preserve instruction hierarchy; its durable audit order remains unchanged.

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

### Exact recall/application/outcome chain

The architecture aliases `recall.event`, `recall.apply`, and
`recall.outcome` do not introduce another event store. They map exactly to:

```text
memory.recall_recorded
  -> memory.obligation_opened
  -> memory.observation_recorded + memory.lifecycle_transitioned
```

The application edge adds these mandatory bindings to `MemoryObligation`:

```text
recall_fact: DurableFactReference
selection_id: non-empty opaque identity
```

`recall_fact.position < application_fact.position < expires_at_position`.
Runtime resolves `recall_fact` from the fixed Session prefix, verifies its kind,
payload digest, namespace, selection identity, Turn/Execution ownership, and
that the exact record/revision occurs once in its selected items. The portable
constructor validates identity, fact order and bounds; it does not read Ledger.

- A recall fact freezes one selection request, ordered revision identities,
  integer score components, selection kinds and truncation. It proves exposure,
  not use or correctness.
- An obligation binds one selected record/revision and recall fact to a committed application
  fact, expected-outcome digest, application-scope digest, attribution-policy
  revision and expiry. A model citation or generated reference cannot open an
  obligation unless Runtime verifies its exact committed application fact and
  membership in the recalled product visible to that Execution.
- An observation binds the obligation to ordered typed durable reality evidence
  and one verifier revision. Observation and lifecycle transition commit
  atomically. An expired, duplicated, cross-namespace, unselected, mismatched or
  unsupported-attribution chain fails closed.

Exposure without an obligation and an open obligation without conclusive
reality evidence do not change `verified`, `falsified`, or lifecycle state.
Runtime may report them as pending/expired audit projections; those projections
are not observations. An admitted `Neutral` observation increments only the
exact `neutral` audit tally and never `verified` or `falsified`.

Conflict between two recalled revisions is not evidence against either one.
Each revision requires separately attributable reality evidence; there is no
"penalize both" or "higher confidence wins" reducer.

### Error versus applicability mismatch

The frozen attribution policy classifies a conclusive negative outcome:

| Classification | Portable reduction |
|---|---|
| Failure is within the declared application scope | `Falsified {in_scope:true}`; increment `falsified`. |
| Failure is outside the declared application scope | `Falsified {in_scope:false, observed_scope_digest}`; increment `neutral` and emit an optional `ScopeNarrowingCandidate`. |
| Scope or causality cannot be established | `Neutral {safe_reason}`; increment only `neutral`. |

A narrowing Candidate binds the source revision, original and observed scope
digests, and exact evidence. It has no write authority. The old immutable
revision is unchanged until normal M0/M1 admission explicitly supersedes it.
Similarity scores, thresholds and model-generated scope labels cannot perform
this attribution by themselves.

### Quality and calibration evidence

Production chains and pinned suites are complementary evidence. Any proposed
recall/calibration policy must publish a content-free, non-overwriting evidence
record binding:

- policy, candidate-port, attribution and verifier revisions;
- exact fixed ledger/repository prefix or pinned corpus digest;
- eligible exposure/application/outcome counts and exclusion reasons;
- integer numerators and denominators for recall, precision, application and
  verified-outcome ratios, with zero denominators represented as absent;
- replay result, namespace/redaction checks and configuration digest.

Floating scores may be a versioned display or candidate-ranking projection.
They are not portable state, truth, authority, lifecycle permission, or a
substitute for `EvidenceTally`. Beta/Bernoulli priors, RRF, rerankers, query
expansion, freshness equations, Platt/isotonic calibration, ranker weights,
thresholds and schedules remain evaluation-gated policies. None is a default
until a focused Spec freezes its exact arithmetic, tie-breaks, inputs, failure
behavior, recovery and cross-language fixtures.

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

An erasure request is valid only with the exact M0 tombstone fact; Runtime
verifies that the fact binds the same record/revision and that the authorized
M0 projection binds the namespace. A user-forget planner emits the tombstone
and first erasure request as one indivisible batch; the standalone tombstone
planner rejects `UserRequest`. Target IDs are canonical and come from explicit
Garive configuration. Every attempt
reports every requested target exactly once in target order. `Complete` is
derived only when all targets are Erased or NotPresent. Backup retention and
retryable failure remain `Partial`; a backup-pending result must state a later
position. Erasure receipts never reverse the tombstone and never make content
model-visible again.

One request contains 1–64 targets. The bound is a protocol constant shared by
Rust and Kotlin; larger configured target sets fail before any erasure attempt.

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
7. M1-G derive integration and pinned recall-quality evaluation;
8. M1-H recall-bound application membership, restart validation and exact
   attributable feedback-quality reduction.

Portable slices require strict Rust evidence plus Kotlin semantic conformance.
Runtime claims require real SQLite restart/process-kill tests. Quality/latency
claims require a pinned dataset and reproducible configuration.

M1-G pins a small synthetic semantic regression suite in
`memory-recall-quality-v1.json`. It measures exact rational recall and
precision over expected identities, forbidden-admission count, ordered replay
mismatch and invalid-case count. Passing proves cross-language selector and
derive semantics only. It is not an empirical user-quality, latency, model, or
production threshold; such a claim remains gated on a representative versioned
dataset and frozen retriever/model configuration.

The admitted v1 synthetic suite contains four selector-linked cases. Its pinned
aggregate is recall `6/7`, precision `6/8`, zero forbidden admissions and zero
ordered replay mismatches. These unreduced fractions are regression evidence,
not a production quality target.

M1-H adds `memory-recall-feedback-v1.json`. Rust and Kotlin independently
reduce the same content-free chain rows into exact exposure, application,
censored, pending, verified, falsified and neutral counts plus unreduced
application and verified-outcome ratios. Runtime planning verifies recall and
application fact identity, namespace, Turn/Execution owner, selection identity
and exact revision membership. SQLite reconstruction repeats those checks and
fails closed on a forged selection after restart.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
