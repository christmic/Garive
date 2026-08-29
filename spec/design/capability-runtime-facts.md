# CF0 — Capability Runtime fact payloads v1

## Status

Accepted companion contract for S0/M0/K0/Q0/MA0. These kinds enter L0 through
the coordinated validators/fixtures delivered with their behavior slices.

## Boundary and common encoding

Runtime produces these payloads; Ledger adapters validate/persist them; Runtime
recovery consumes them. Every payload is an exact JSON object under C6F/L0
canonical payload v1 with outer `schema_version = 1`.

Common C6F rules apply: unknown fields fail semantic application; unknown
schema versions remain opaque; IDs are non-empty typed values; digests are 64
lowercase SHA-256 hex; counts/positions/durations are non-negative integers;
timestamps are canonical RFC 3339 UTC strings and never ordering truth.

```text
Scope = { "kind": "session", "owner_id": SessionId }
      | { "kind": "agent_instance", "owner_id": AgentInstanceId }
      | { "kind": "namespace" }

FactReference {
  session_id: SessionId
  position: non-zero u64
  fact_id: FactId
  payload_digest: Digest
}

ContentBinding = C6F ContentBinding
```

All lists are ordered and may be empty only where stated. Set-valued lists are
unique and use the focused Spec's canonical order.

## Skill facts

```text
skill.activated.v1 {
  activation_id: SkillActivationId
  request_digest: Digest
  mode: "explicit" | "tagged"
  through_position: u64
  skills: [
    {
      skill_id: SkillId
      skill_revision: SkillRevision
      definition_digest: Digest
      instruction_digest: Digest
      reason: "explicit" | "tag_match"
    }
  ]
  truncated: bool
}
```

The outer fact is Turn/Execution scoped. `skills` may be empty only for tagged
activation. Explicit activation requires exactly one item and
`truncated=false`. Equal `activation_id` binds equal request and result.

## Memory facts

```text
memory.proposed.v1 {
  proposal_id: MemoryProposalId
  namespace_id: MemoryNamespaceId
  scope: Scope
  kind: "preference" | "constraint" | "decision" |
        "learned_fact" | "summary"
  content: ContentBinding
  evidence: non-empty FactReference[]
  sensitivity: "ordinary" | "restricted"
  confidence_basis_points: 0..10000
  expected_active_revision_id?: MemoryRevisionId
}

memory.committed.v1 {
  proposal_id: MemoryProposalId
  record_id: MemoryRecordId
  revision_id: MemoryRevisionId
  namespace_id: MemoryNamespaceId
  scope: Scope
  kind: same enum as memory.proposed
  content: ContentBinding
  evidence: non-empty FactReference[]
  sensitivity: "ordinary" | "restricted"
  confidence_basis_points: 0..10000
  valid_from_position: non-zero u64
  retention_policy_digest: Digest
  expires_at_utc?: timestamp
  supersedes_revision_id?: MemoryRevisionId
}

memory.rejected.v1 {
  proposal_id: MemoryProposalId
  reason: "namespace_denied" | "evidence_not_found" |
          "evidence_mismatch" | "revision_conflict" |
          "retention_rejected" | "sensitivity_denied" |
          "limit_exceeded" | "unsupported"
}

memory.superseded.v1 {
  record_id: MemoryRecordId
  old_revision_id: MemoryRevisionId
  new_revision_id: MemoryRevisionId
  proposal_id: MemoryProposalId
}

memory.tombstoned.v1 {
  command_id: CommandId
  record_id: MemoryRecordId
  revision_id: MemoryRevisionId
  reason: "expired" | "superseded" | "user_request" |
          "policy" | "corrupt_source"
}

memory.retrieval_recorded.v1 {
  query_id: MemoryQueryId
  query_digest: Digest
  namespace_id: MemoryNamespaceId
  retriever_revision: non-empty string
  through_position: u64
  as_of_utc: timestamp
  max_results: non-zero u64
  max_total_bytes: non-zero u64
  include_restricted: bool
  restricted_grant_digest?: Digest
  matches: [
    {
      record_id: MemoryRecordId
      revision_id: MemoryRevisionId
      content: ContentBinding
      content_byte_length: non-zero u64
      evidence: non-empty FactReference[]
      relevance_basis_points: 0..10000
      sensitivity: "ordinary" | "restricted"
    }
  ]
  truncated: bool
}
```

`memory.proposed` and `memory.committed` commit atomically for a direct accept;
`memory.proposed` and `memory.rejected` commit atomically for a direct reject;
an interaction may place a C5/C6 suspension between proposal and decision. A
commit that
supersedes an active revision atomically includes `memory.superseded`.
Tombstone is valid only for the exact active revision.
`include_restricted=true` requires `restricted_grant_digest`, while false
forbids it; every restricted match additionally requires the true/granted
shape.
Retrieval match order is semantic and every content/evidence/byte-length
binding is verified before commit.

Proposal and retrieval facts are parent Turn/Execution scoped. Committed,
rejected and superseded decisions retain that proposal ownership. A tombstone
is Session scoped and has no Execution owner. Namespace-scoped records may be
referenced from another Session only after Runtime authority verification.

## Knowledge facts

```text
knowledge.requested.v1 {
  request_id: KnowledgeRequestId
  source_id: KnowledgeSourceId
  source_revision: KnowledgeSourceRevision
  request_digest: Digest
  mode: "keyword" | "semantic" | "structured"
  query: ContentBinding
  filters: ContentBinding
  through_position: u64
  max_chunks: non-zero u64
  max_total_bytes: non-zero u64
  deadline_budget_ms: non-zero u64
  freshness_kind: "cached_allowed" | "revalidate" | "exact_snapshot"
  exact_snapshot_digest?: Digest
}

KnowledgeEvidenceBinding {
  evidence_id: KnowledgeEvidenceId
  source_snapshot_digest?: Digest
  content: ContentBinding
  content_byte_length: non-zero u64
  citation_kind: "uri_fragment" | "document_offset" |
                 "record_key" | "opaque_locator"
  citation_locator: non-empty string
  citation_title?: non-empty string
  canonical_uri?: non-empty string
  citation_content_digest: Digest
  retrieved_at_utc: timestamp
  freshness: "fresh" | "cached" | "stale"
  trust_class: "curated" | "first_party" | "third_party" | "untrusted"
  rank_basis_points: 0..10000
}

knowledge.completed.v1 {
  request_id: KnowledgeRequestId
  request_digest: Digest
  evidence: KnowledgeEvidenceBinding[]
  truncated: bool
}

knowledge.failed.v1 {
  request_id: KnowledgeRequestId
  request_digest: Digest
  phase: "pre_dispatch" | "dispatched" | "response_validation"
  reason: "source_denied" | "unsupported" | "unavailable" |
          "rejected" | "uncertain" | "citation_invalid" |
          "content_digest_mismatch" | "limit_exceeded"
  ambiguous: bool
  retry_after_ms?: non-zero u64
}
```

`freshness_kind=exact_snapshot` requires `exact_snapshot_digest`; other kinds
forbid it. Requested commits before connector dispatch. Exactly one completed
or failed terminal is admitted. Completed evidence may be empty and is ordered
semantically. `ambiguous=true` is required for a dispatched attempt without a
trustworthy terminal response.

Knowledge facts are scoped to the requesting Turn/Execution.
`KnowledgeRequestId` remains in the payload because L0 has no dedicated
Knowledge identity field.
Every evidence byte length is Runtime-verified; inline UTF-8 must match it and
the ordered result must remain within the request's committed total-byte bound.

## Scheduler facts

```text
schedule.created.v1 {
  command_id: ScheduleCommandId
  schedule_id: ScheduleId
  revision_id: ScheduleRevisionId
  intent: ContentBinding
  intent_digest: Digest
}

schedule.claimed.v1 {
  schedule_id: ScheduleId
  revision_id: ScheduleRevisionId
  occurrence_id: OccurrenceId
  ordinal: non-zero u64
  due_at_utc: timestamp
  lease_id: ScheduleLeaseId
  lease_epoch: non-zero u64
  through_position: u64
}

schedule.fired.v1 {
  schedule_id: ScheduleId
  revision_id: ScheduleRevisionId
  occurrence_id: OccurrenceId
  ordinal: non-zero u64
  runtime_command_id: CommandId
  disposition: "committed" | "replayed"
  committed_position: non-zero u64
}

schedule.cancelled.v1 {
  command_id: ScheduleCommandId
  schedule_id: ScheduleId
  expected_revision_id: ScheduleRevisionId
  reason: "user" | "operator" | "policy" | "superseded"
}

schedule.failed.v1 {
  schedule_id: ScheduleId
  revision_id: ScheduleRevisionId
  occurrence_id?: OccurrenceId
  ordinal?: non-zero u64
  reason: "invalid_schedule" | "subject_not_resumable" |
          "authority_denied" | "clock_invalid" | "occurrence_overflow" |
          "misfire_limit_exceeded" | "dispatch_conflict"
}
```

Occurrence ID/ordinal are both present or both absent in `schedule.failed`.
`schedule.created.intent_digest` must equal `schedule.created.intent.digest`.
Claimed commits before C6 dispatch. Fired commits only after that exact C6
command committed/replayed. A lease takeover uses a higher epoch; a stale lease
cannot append fired/failed. Cancellation cannot erase an already fired command.

Scheduler facts are Session scoped with no Turn/Execution owner. The subject
binding names the exact existing Session/Turn resources inside the canonical
intent; Q0 cannot create an unowned Session implicitly.

## Delegation facts

```text
DelegationBudgetBinding {
  max_child_turns: non-zero u64
  max_child_executions: non-zero u64
  max_iterations: non-zero u64
  max_input_tokens?: non-zero u64
  max_output_tokens?: non-zero u64
  deadline_budget_ms: non-zero u64
  max_depth: non-zero u64
}

DelegationConsumption {
  child_turns: non-zero u64
  child_executions: non-zero u64
  completed_iterations: u64
  elapsed_ms: u64
}

delegation.requested.v1 {
  delegation_id: DelegationId
  intent_digest: Digest
  intent: ContentBinding
  through_position: u64
}

delegation.authorized.v1 {
  delegation_id: DelegationId
  grant_id: DelegationGrantId
  intent_digest: Digest
  reserved_budget: DelegationBudgetBinding
  authority_revision: non-empty string
}

delegation.denied.v1 {
  delegation_id: DelegationId
  intent_digest: Digest
  reason: "authority_denied" | "child_not_found" |
          "child_revision_mismatch" | "budget_exhausted" |
          "depth_exceeded" | "concurrency_exceeded"
}

delegation.child_started.v1 {
  delegation_id: DelegationId
  grant_id: DelegationGrantId
  child_agent_instance_id: AgentInstanceId
  child_turn_id: TurnId
  child_snapshot_digest: Digest
  parent_suspension_id: SuspensionId
}

delegation.child_terminal.v1 {
  delegation_id: DelegationId
  grant_id: DelegationGrantId
  result_id: DelegationResultId
  child_agent_instance_id: AgentInstanceId
  child_turn_id: TurnId
  child_snapshot_digest: Digest
  outcome: "completed" | "stopped" | "failed"
  content?: ContentBinding
  evidence: FactReference[]
  reason?: "iteration_limit" | "token_limit" | "deadline" |
           "cancelled" | "resource_unavailable" | "invalid_input" |
           "invalid_model_output" | "required_capability_unavailable" |
           "port_failure" | "invariant_violation" |
           "durability_failure" | "corrupt_recovery_state"
  usage: UsageEvidence
  consumption: DelegationConsumption
}

delegation.observed.v1 {
  delegation_id: DelegationId
  grant_id: DelegationGrantId
  result_id: DelegationResultId
  parent_suspension_id: SuspensionId
  child_turn_id: TurnId
  observation: ContentBinding
}
```

Requested and authorized/denied are ordered durable decisions; direct denial
may commit atomically with requested. Authorized commits before child creation.
`delegation.requested.intent_digest` must equal
`delegation.requested.intent.digest`.
Child-started commits atomically with the child `turn.started` transaction and
the parent's `DelegationPending` terminal pair. `outcome=completed` requires
content and forbids reason; stopped/failed require reason and forbid content.
Evidence may be empty. Consumption and usage cannot exceed reserved budget;
unknown token usage consumes the full corresponding reservation.
Child-terminal derives only from the exact child terminal. Observed commits
before the parent's `delegation_result` continuation input and binds the same
suspension/result identities.

Requested/authorized/denied are parent Turn/Execution scoped. Child-started is
parent scoped while its atomic sibling `turn.started` is child scoped.
Child-terminal is child Turn scoped. Observed is parent Turn scoped and names
the parent suspension before continuation creates a fresh parent Execution.

## Cross-family invariants

1. A capability fact never creates tool, connector, namespace or actor
   authority.
2. Content affecting a model request is terminally committed first.
3. Same fact/command/capability identity with different semantics conflicts and
   appends nothing.
4. Parent Turn terminal rules account for active Knowledge dispatch,
   capability interaction and delegated child state; Skill/Memory retrieval
   are synchronous committed inputs and Scheduler lives outside a Kernel
   Execution.
5. Unknown/new schema remains inspectable but cannot influence recovery or
   model context until admitted.

## Required fixture coverage

The later shared fact fixture must include every kind above, exact-field/type
negative cases, conditional-field matrices, digest/content corruption,
idempotent replay, semantic collision and lifecycle-invalid transitions in
both Rust and Kotlin validators.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
