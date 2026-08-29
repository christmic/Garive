# S0 — Exact Skill activation

## Status

Accepted implementation contract in the Agent capability set.

## Scope and ownership

S0 defines a portable, immutable instruction Skill and deterministic bounded
activation. Engine owns descriptor validation, matching and activation
reduction. Runtime owns registry resolution, content loading, authorization and
the durable activation fact.

A Skill is not an executable plugin, tool, child Agent or hidden model call.
It may shape context and planning; every external action still uses C4/C5 and
every child Agent still requires a future Multi-Agent contract.

## Skill definition and snapshot descriptor

```text
SkillDefinition {
  skill_id, skill_revision
  name, description
  instructions: ContentBinding
  activation:
    ExplicitOnly |
    Tagged { non-empty tags }
  required_capabilities: ordered exact references
  allowed_tool_references: ordered exact references
  max_instruction_bytes: non-zero u64
  contract_version
}
```

IDs and revisions are non-empty typed values. Names/tags use normalized UTF-8
with explicit length limits; comparison is byte-exact after validation. The
definition digest uses the D0 RFC 8785 envelope and binds every field above.

The effective snapshot contains exact descriptors and instruction content
digests. Runtime verifies content before execution. A Skill cannot add a tool,
Memory namespace, Knowledge source, model role or governance permission absent
from the snapshot. `allowed_tool_references` only narrows the already enabled
tool set.

## Activation request and result

```text
SkillActivationRequest {
  activation_id
  turn_id, execution_id, iteration
  mode: Explicit | Tagged
  requested_skill_id?
  trusted_tags: ordered unique strings
  through_position
  max_active_skills: non-zero u32
  max_total_instruction_bytes: non-zero u64
}

ActivatedSkill {
  skill_id, skill_revision, definition_digest
  instructions
  activation_reason
  allowed_tool_references
}

SkillActivationResult = Activated { ordered_skills, truncated }
                      | None | Unsupported | Failed { code }
```

`request_digest` is lowercase SHA-256 over L0 canonical JSON containing
contract `garive.skill-activation`, version `1`, Turn/Execution/iteration,
mode, optional requested Skill, ordered trusted tags, through-position and both
bounds. Activation ID is excluded because its typed outer identity owns
idempotency; changed semantics under one ID conflict.

Explicit activation requires one exact enabled ID. Tagged activation uses only
trusted Runtime-supplied tags, never raw model text. Intent/semantic matching,
embeddings, model calls and remote discovery are not admitted in v1; they need
a focused deterministic matcher/port Spec and cannot hide inside the reducer.

Candidate order is explicit request order, then descending matched-tag count,
then lexical skill ID/revision. Duplicates collapse by exact ID/revision. Bounds
are applied in that order; required explicit activation fails instead of being
silently truncated.

## Context integration

Runtime commits `skill.activated` before the activated instructions enter a
model request. The fact binds activation ID, request digest, exact ordered
Skill IDs/revisions/definition/instruction digests, reason, fixed durable
position and truncation.

C2 inserts activated instructions after trusted system/definition
instructions and before Memory/Knowledge evidence. A Skill cannot override a
higher-precedence instruction, relax governance or rewrite the effective
snapshot. Later iterations may activate a different allowed set only through a
new activation identity and durable fact.

Restart with the exact activation request reuses the committed result. A
registry change cannot replace a frozen revision inside the same Turn.

## Skill-authored tool intent

Skill instructions may guide the model to emit ordinary Tool Intents. Those
intents carry no Skill authority. C4 resolves the exact tool and C5 authorizes
each Prepared Call normally. Runtime may attach the activation identity as
audit provenance, but grants bind the Prepared Call and invocation, not the
Skill.

## Stable failures

`invalid_skill`, `skill_not_enabled`, `skill_revision_mismatch`,
`instruction_digest_mismatch`, `activation_mode_unsupported`,
`required_capability_unavailable`,
`instruction_limit_exceeded`, `activation_conflict`,
`durability_failure`, and `corrupt_skill_state`.

## Acceptance evidence

- shared Rust/Kotlin definition, ordering, bounds and failure fixtures;
- canonical D0 digest vectors for changed instruction/capability/tool bounds;
- tests proving a Skill cannot widen the snapshot or bypass C4/C5;
- tagged ordering/property tests and explicit unsupported-mode behavior;
- Runtime SQLite restart test proving commit-before-model and exact replay;
- Engine Skill has no process, plugin loader, HTTP, filesystem or model client.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
