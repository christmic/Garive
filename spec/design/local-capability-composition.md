# R1-C — Local Memory and Knowledge composition

> Implementation contract for Runtime and Desktop engineers wiring accepted
> M0/K0 capabilities into the shipping local Agent loop. It makes snapshot
> admission, explicit system bindings, durable retrieval and C2 injection one
> fail-closed production chain; it does not admit self-evolution.

## Audience

Engineers changing `runtime/replica`, Desktop Host construction, D0 Agent
installation, Memory storage, Knowledge connectors or local crash recovery.

## Why

M0 and K0 already define portable values, durable facts and Core capability
inputs, but the production `LocalExecutionWorker` currently invokes Core with
an empty `PreparedAgentCapabilities`. Unit tests that call the lower-level
capability entry point directly do not make Memory or Knowledge available to a
shipping Agent. Product composition must close that gap without allowing Core,
the model or Desktop presentation to discover stores, sources or credentials.

## Ownership

| Concern | Owner |
|---|---|
| Memory/Knowledge values and deterministic reduction | Engine M0/K0 |
| Exact capability admission | D0 effective Agent snapshot |
| Namespace/source authority and concrete bindings | Runtime system configuration |
| SQLite Memory repository, Knowledge connector and crash policy | Runtime |
| Capability candidate selection and budget | Core C2 |
| Configuration persistence and credential resolution | Desktop backend |
| Capability presentation | Clients through Host projections only |

Self-evolution, automatic model-output learning, autonomous Knowledge queries,
and background Memory distillation are outside this contract.

## Installed binding

Every local execution resolves its durable Definition ID, Definition revision
and snapshot digest through the immutable `RuntimeAgentCatalogue`. The resolved
snapshot is the sole authority to request a capability.

```text
LocalCapabilitySystemConfig {
  memory?: LocalMemorySystemBinding
  knowledge?: LocalKnowledgeSystemBinding
}

LocalMemorySystemBinding {
  capability_name, exact_revision, descriptor_digest
  namespace_id, retriever_revision, source_policy_revision
  max_results, max_total_bytes, max_repository_records, max_repository_facts
}

LocalKnowledgeSystemBinding {
  capability_name, exact_revision, descriptor_digest
  source_descriptor, request_policy_revision
  max_chunks, max_total_bytes, deadline_budget_ms
  connector
}
```

All text and bounds are explicit constructor inputs. The bindings contain no
process-environment lookup, implicit file search, mutable `latest` reference,
provider-specific model value or client-supplied authority.

For each capability:

- snapshot descriptor absent: capability is not prepared even when a system
  binding exists;
- descriptor present and system binding absent: execution fails before model or
  connector dispatch;
- identity, revision, contract version or descriptor digest mismatch: execution
  fails before dispatch;
- an extra binding never adds a capability to the snapshot;
- a continuation must reuse the same snapshot and system-binding revision.

## Runtime preparation boundary

`LocalExecutionWorker` owns one optional `LocalCapabilityPreparationFactory`.
After fixed-prefix Turn reconstruction and before Core/model dispatch, it asks
the factory to prepare capability inputs from:

```text
LocalCapabilityPreparationInput {
  durable Agent coordinates
  Session, Turn and Execution identities
  trusted current input binding
  committed_position
  canonical recorded_at
}
```

The factory receives a read-only `SqliteLedger`; it cannot commit facts. It
returns `PreparedAgentCapabilities`, whose facts are committed by the existing
durable execution coordinator before C2 can expose content. Model-only and
tool-capable executions use the same preparation boundary. Installing tool
governance does not imply Memory or Knowledge authority.

## Production Memory preparation

The first product slice is a bounded push of explicit `UserDeclared` Memory.
`AgentLearned` and `OrganisationPublished` revisions are not injected by this
slice; later menu/detail recall must use the accepted M1 contract.

Preparation performs this exact sequence:

1. Resolve the installed Memory descriptor and exact system binding.
2. Open the configured namespace from the canonical fact-backed M2 projection.
   An absent repository is an authorized empty namespace; isolated test state
   is unavailable; corrupt fact/projection parity fails closed.
3. Verify every selected current row against its source and classification
   facts. Admit only active, non-erased `UserDeclared` revisions in configured
   scope classes. Restricted revisions are excluded in v1.
4. Freeze the repository revision and the exact source-prefix set before
   scoring. Source Session positions are local and are never compared with the
   consuming Turn's local position or with another Session's position.
5. Build one deterministic M0 query from the trusted current input, configured
   namespace, consuming Turn position, canonical time and explicit bounds.
6. The `user-declared-push-v1` policy gives every admitted declaration equal
   relevance. Portable M0 lexical record/revision ordering breaks ties; local
   source positions never do. A future relevance/recency policy requires a new
   retriever revision and source-aware index evidence.
7. Plan one `memory.retrieval_recorded` fact even when the authorized result is
   empty. Core sees content only after that exact fact commits.

The Runtime record and source-fact bounds are applied before M0 result/byte
bounds. Exceeding either repository scan bound fails closed rather than
truncating an unverified source set. Normal M0 result truncation remains
explicit in the committed fact.

## Production Knowledge preparation

K0 remains externally sourced evidence, not Memory and not a browsing loop.
The first product slice admits at most one exact Knowledge descriptor and one
matching Runtime source binding per Agent snapshot.

Preparation derives a bounded Keyword request from the trusted current input
only when the exact request-policy revision permits automatic retrieval. The
request freezes source revision, filters, freshness requirement and all bounds.
The configured connector receives no ambient Garive configuration and no model
provider object.

The durable sequence remains:

```text
knowledge.requested
  -> knowledge.dispatched
  -> knowledge.completed | knowledge.failed
  -> optional atomic C2 Knowledge candidate
```

Requested-without-dispatch may be redispatched after policy revalidation.
Dispatched-without-terminal is uncertain unless the connector's configured
read-only/idempotent contract and recovery policy admit a fresh attempt. No
evidence enters the model from an in-memory connector response.

## Context and precedence

Memory and Knowledge enter `AgentTurnRequest.capability_context_candidates`
only through the existing durable execution coordinator. Core performs the
sole C2 derive. Both candidates are attributed data, never system/developer
instructions, governance decisions, tool grants or sandbox authority.

Required system facts, explicit policy and current trusted input outrank both
capabilities. Candidate omission by the C2 item/byte budget is recorded through
the existing retained/dropped references.

## Recovery

| Durable cut | Recovery |
|---|---|
| Before capability fact | Reconstruct the same snapshot/binding and prepare again. |
| `memory.retrieval_recorded` committed | Reuse and verify the committed exact result; do not query a changed repository under the same identity. |
| `knowledge.requested` only | Revalidate and dispatch the same request identity. |
| `knowledge.dispatched` only | Apply K0 uncertainty/retry classification. |
| Capability terminal committed, model not started | Reconstruct the exact candidate from facts before model dispatch. |
| Model started | Existing C6 model recovery rules own the outcome. |

Restart must reject a changed repository/source binding masquerading as the
same prepared identity. A new execution may prepare a new capability request
under its own identity after C6 explicitly abandons the prior execution.

## Stable failures

| Code | Meaning |
|---|---|
| `capability_binding_missing` | Snapshot requires a capability without a system binding. |
| `capability_binding_mismatch` | Descriptor and system binding differ. |
| `memory_repository_corrupt` | Fact-backed repository and projection disagree. |
| `memory_repository_bound_exceeded` | Configured pre-retrieval scan bound was exceeded. |
| `memory_preparation_failed` | Authorized Memory query/result could not be constructed. |
| `knowledge_preparation_failed` | Exact K0 request/source values could not be constructed. |

Failures contain no Memory content, query text, filesystem path, connector
endpoint, credential, provider body or raw SQLite error.

## Acceptance evidence

- D0 installation rejects absent/mismatched Memory and Knowledge bindings;
- shared Rust/Kotlin M0 fixture proves Session-local positions are neither
  visibility checks nor cross-Session tie breakers;
- real SQLite product test writes classified user Memory in one Session,
  starts another Session and proves `memory.retrieval_recorded` precedes
  `model.started` and exact content reaches C2;
- agent-learned, restricted, tombstoned, superseded, expired, foreign namespace
  and corrupt projection records never reach the model;
- empty configured namespace commits an empty retrieval without inventing
  Memory content;
- restart tests cover every table row in the Recovery section;
- fake Knowledge connector proves exact binding, commit-before-model ordering,
  redaction and dispatched uncertainty;
- production Desktop Host reconstruction retains exact bindings without
  environment discovery;
- formatting, strict Clippy, tests, warning-denied Rustdoc and applicable
  cross-language conformance pass.

## See also

- [M0 Memory](memory-capability.md) — portable record/query/result contract.
- [K0 Knowledge](knowledge-retrieval.md) — attributed connector lifecycle.
- [D0 Agent snapshot](agent-definition-snapshot.md) — exact capability admission.
- [R1 local Runtime](local-runtime-composition.md) — local worker and recovery.
- [C2 context](context-surface.md) — sole candidate derive and precedence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: accepted
