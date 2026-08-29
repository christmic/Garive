# D0 — Agent Definition and effective snapshot

> Contract for the project owner and Agent/Runtime engineers defining portable
> Agent identity, exact reference resolution, and the immutable snapshot bound
> to every execution of one Turn.

## Audience

Engineers implementing definition/configuration values, Runtime resolution,
Core request construction, persistence bindings, or Kotlin conformance.

## Why

The architecture names Agent Definition but did not define a development-ready
shape or resolution failure contract. Leaving resolution to Runtime code would
allow mutable “latest” references, secret leakage, or revision changes across a
continuation.

## Status

Accepted implementation contract for D0.

## Purpose and ownership

An Agent Definition is portable, immutable intent. Runtime resolves its
references into one immutable Effective Agent Snapshot before starting a Turn.
Core consumes the snapshot; it never consults registries, files, environment
variables, credentials, or mutable product configuration.

- Engine owns definition and snapshot value contracts.
- Runtime owns registries, resolution, product configuration and secrets.
- A Turn binds one exact definition revision and snapshot digest for its entire
  lifetime, including continuations.

## Definition value

```text
AgentDefinition {
  definition_id: AgentDefinitionId
  revision: AgentDefinitionRevision
  instruction_sources: ordered InstructionReference[]
  model_roles: ordered ModelRoleRequirement[]
  capabilities: CapabilityReferences
  governance: GovernancePolicy
  context_policy: ContextPolicyReference
  limits: DefaultLimits
  contract_versions: ContractVersionSet
}
```

V1 nested values are:

```text
InstructionReference {
  source_id, exact_revision, required: bool
}

ModelRoleRequirement {
  role_id
  required_capabilities: unique sorted string set
  required: bool
}

CapabilityReference {
  kind: Tool | Skill | Memory | Knowledge | Delegation
  name, exact_revision, contract_version
  required: bool
}

GovernancePolicy {
  policy_id, exact_revision
  allowed_requirement_capabilities: unique sorted C4 capability set
  interaction_modes: unique sorted Approval | ExternalInput set
  default_unmatched: Deny
}

ContextPolicyReference { policy_id, exact_revision }

DefaultLimits {
  max_iterations: non-zero u64
  max_input_tokens?: non-zero u64
  max_output_tokens?: non-zero u64
  deadline_budget_ms?: non-zero u64
}

ContractVersionSet { contract_name -> non-zero u64 }
```

Instruction list order is precedence order from lowest to highest; duplicate
source IDs are invalid. Role IDs and `(kind, name)` capability keys are unique.
Every reference is exact: ranges and implicit `latest` are invalid in v1.
String sets are serialized in ascending Unicode scalar-value order; enum sets
use their declaration order. Arrays with behavioral order retain declaration
order.

All identities and revisions are non-empty opaque strings. Lists whose order
affects behavior are explicitly ordered. Capability maps use unique stable
keys. A definition contains no actor, Session/Turn identity, secret, endpoint,
workspace path, live handle, provider credential, database key, or task state.

`DefaultLimits.max_iterations` is non-zero. Optional token/deadline limits do
not mean unbounded execution: Runtime must apply an equal or stricter external
bound before dispatching Core.

## Resolution

Runtime resolves every reference exactly once for a new Turn:

1. load the exact definition ID and revision;
2. resolve instruction content and precedence in declared order;
3. resolve model roles to neutral capability targets, not provider clients;
4. resolve enabled tool/skill/memory/knowledge/delegation descriptors to exact
   revisions and capability versions;
5. intersect requested governance/requirements with product and actor policy;
6. apply product overrides only where the definition declares an override
   point, never by mutating the stored definition;
7. validate cross-field invariants and produce one snapshot;
8. commit the definition binding and snapshot digest before Core starts.

Missing, ambiguous, revoked, cyclic, unsupported-version, or policy-incompatible
references fail resolution. Runtime does not silently choose “latest”, omit a
required capability, or weaken a limit.

## Effective Agent Snapshot

```text
EffectiveAgentSnapshot {
  definition_id, definition_revision, definition_digest
  instructions: ordered ResolvedInstruction[]
  model_roles: ordered ResolvedModelRole[]
  capabilities: ResolvedCapabilitySnapshot
  governance: EffectiveGovernancePolicy
  context_policy: ResolvedContextPolicy
  limits: EffectiveLimits
  contract_versions: ContractVersionSet
  snapshot_digest: SnapshotDigest
}
```

```text
ResolvedInstruction {
  source_id, exact_revision, content_utf8, content_digest
}

ResolvedModelRole {
  role_id
  capability_target_id
  admitted_capabilities: unique sorted string set
}

ResolvedCapabilityDescriptor {
  kind, name, exact_revision, contract_version, descriptor_digest
}
```

The capability snapshot contains full exact C4 `ToolDefinition` values for
enabled tools and descriptors for other capabilities. A required role or
capability must resolve; an unavailable optional capability is omitted and its
absence is recorded in resolution evidence outside the snapshot. Runtime may
tighten limits, disable optional capabilities, or strengthen governance. It may
not rewrite instructions, add capabilities, widen limits, weaken governance,
or substitute revisions while producing an effective snapshot.

Resolved values contain stable identities, exact revisions, public capability
metadata and content digests. Secret-bearing or executor-bearing data stays in
Runtime-owned port implementations. The snapshot is deeply immutable for one
Turn. A registry/configuration change affects only a new Turn unless an
explicit product migration creates a new Turn.

## Canonical digest

`snapshot_digest` is lowercase SHA-256 over UTF-8 RFC 8785 JSON Canonicalization
Scheme bytes of this versioned preimage:

```json
{
  "contract": "garive.effective-agent-snapshot",
  "version": 1,
  "definition_id": "...",
  "definition_revision": "...",
  "definition_digest": "...",
  "instructions": [],
  "model_roles": [],
  "capabilities": {},
  "governance": {},
  "context_policy": {},
  "limits": {},
  "contract_versions": {}
}
```

The digest field itself and all secret/runtime handles are excluded. Inputs
must satisfy I-JSON; duplicate keys, non-finite numbers, lone surrogates and
values that cannot be represented losslessly by the declared schema are
rejected. This canonical contract is independent from L0 canonical payload v1,
which deliberately permits integer JSON only.

## Failure classes

| Code | Meaning |
|---|---|
| `definition_not_found` | Exact ID/revision is absent. |
| `reference_not_found` | A required exact reference cannot be resolved. |
| `reference_ambiguous` | Resolution produced more than one candidate. |
| `reference_cycle` | Instruction/capability references contain a cycle. |
| `unsupported_contract_version` | A required semantic version is not admitted. |
| `policy_incompatible` | Product authority cannot satisfy required governance. |
| `invalid_definition` | A local or cross-field invariant failed. |
| `non_canonical_value` | The digest input cannot satisfy canonical rules. |

Failures expose secret-free paths and codes. They do not leak referenced
content, credentials, or policy internals.

## Continuation and compatibility

- `Start` commits the snapshot binding before `execution.started`.
- `Continue` must name the same definition revision and snapshot digest.
- A mismatch fails closed as invalid reconstructed input; Runtime does not
  reinterpret prior facts under a newer definition.
- Adding an optional field requires a new canonical preimage version if it
  changes execution meaning. Unknown required contract versions are rejected.

## Required acceptance evidence after approval

- fixture cases for exact resolution, ordering, missing/ambiguous/cyclic
  references, stricter effective limits and continuation mismatch;
- canonical fixture pairs proving map-order independence and meaning-change
  digest sensitivity in Rust and Kotlin;
- property tests for non-empty identities, unique capability keys, limit
  monotonicity and deterministic resolution;
- dependency tests proving Core has no registry/config-loader dependency.

## See also

- [`agent-architecture.md`](agent-architecture.md) — Agent/Runtime ownership.
- [`prepared-tool-call.md`](prepared-tool-call.md) — exact tool definitions
  included in the capability snapshot.
- [`durable-runtime-turn.md`](durable-runtime-turn.md) — Turn binding and
  continuation validation.
- [`cross-language-agent-contract.md`](cross-language-agent-contract.md) —
  current and proposed Rust/Kotlin admission rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
