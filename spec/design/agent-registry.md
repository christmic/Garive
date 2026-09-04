# A0 — Agent registry and directory binding

> Public Runtime contract for defining an Agent by stable metadata and loading
> its user-managed resources from local directories.

## Status

Accepted implementation contract.

## Purpose and ownership

The Agent registry is the source of truth for which Agents may participate in
Sessions. An Agent record is metadata, not a copy of Agent content and not an
immutable content version:

```text
Agent {
  agent_id
  working_directory
  readonly_knowledge_directories[]
  writable_knowledge_directory?
  status: inactive | active | archived
}
```

The Runtime owns registry persistence and admission. The user owns the files
below the declared directories and may edit them without publishing a new
Agent version. Engine and Core receive only the resolved, bounded execution
snapshot; they do not read the registry.

## Field invariants

- `agent_id` is 1–64 lowercase ASCII characters. The first character is
  alphanumeric; remaining characters are alphanumeric, `.`, `_`, or `-`.
- `working_directory` is an absolute existing directory at creation and is
  immutable with `agent_id`.
- `readonly_knowledge_directories` is an ordered list of zero or more absolute
  existing directories. Duplicate canonical paths are invalid.
- `writable_knowledge_directory` is absent or one absolute existing directory.
- Every directory root is canonicalized before persistence. A declared root
  itself may not be a symbolic link.
- Working and knowledge roots may not equal or contain one another. This keeps
  write authority and read-only authority unambiguous.
- No credential, model/provider configuration, Session state, Skill list,
  instruction body, content digest, or public version belongs in this record.

The working directory is both the Agent resource root and its ordinary
workspace. Conventional resources such as `AGENT.md`, optional `SOUL.md`, and
`skills/` live below it. There is no Skill registration API: installing a
Skill means placing its bounded files below the working directory.

## Lifecycle

Creation stores an `inactive` Agent. Activation reopens every declared root,
requires the working directory to remain readable and writable, requires a
bounded valid UTF-8 `AGENT.md`, validates optional `SOUL.md`, and requires
knowledge roots to satisfy their declared read/write authority. If `skills/`
exists it must be a directory and its traversed resources must remain confined
to the working root.

Session membership is metadata and may reference an Agent in any lifecycle
state, or an identity that will be registered later. `active` admits new Runs;
`inactive` and `archived` do not. Archive prevents new work but does not
corrupt or abruptly discard a Run already executing; that Run reaches a normal
terminal state. An archived Agent may be activated again after validation.

Because users may edit directories after activation, Runtime reopens and
validates the binding at each new Run. Missing, unreadable, oversized,
non-UTF-8, escaped, or authority-incompatible resources fail closed. A Run
uses the exact content it resolved at admission; later file changes affect the
next Run only.

## HTTP API

All responses use `api_version = "v1"`. Mutation bodies reject unknown fields.
Mutation commands require `Idempotency-Key` under the H1 rules.

| Method and path | Request | Result |
|---|---|---|
| `POST /v1/agents` | all four identity/directory fields except status | create inactive Agent |
| `GET /v1/agents` | none | bounded stable `agent_id` order |
| `GET /v1/agents/{agent_id}` | none | exact Agent or `not_found` |
| `PATCH /v1/agents/{agent_id}` | complete replacement knowledge binding | atomic knowledge update |
| `POST /v1/agents/{agent_id}/activate` | empty JSON object | validate and activate |
| `POST /v1/agents/{agent_id}/archive` | empty JSON object | archive |

PATCH contains both `readonly_knowledge_directories` and
`writable_knowledge_directory`; `null` clears the writable directory. It cannot
carry `agent_id`, `working_directory`, or `status`. A knowledge update on an
active Agent is validated before commit and applies to newly admitted Runs.

Create conflicts when `agent_id` already exists. Replaying the same command
with identical semantics returns the committed record; semantic reuse returns
`command_conflict`. Lifecycle commands are idempotent for an already matching
state. Invalid directories/resources return `precondition_failed`; persistence
failure remains `durability_unavailable`.

## Session and execution binding

Session creation and Agent membership accept `agent_id`, not definition
identity. Durable Session membership records the stable `agent_id` without
loading or validating the Agent. Before each new Run, Runtime requires current
membership, resolves the active directory binding into its internal immutable
Effective Agent Snapshot, and durably binds the snapshot digest to that Run.
The snapshot is an audit/execution coordinate, not a user-visible Agent version
and does not prevent direct directory edits.

Existing Sessions retain membership when an Agent is archived, but cannot
start another Run for that Agent until it is active again. Anonymous/forked
subagents inherit the initiating Agent binding unless explicitly assigned an
active named Agent.

## Acceptance

1. Registry state survives Runtime restart in the same SQLite database.
2. API rejects attempts to mutate identity, working directory, or status by
   PATCH and rejects unknown JSON fields.
3. Session membership accepts syntactically valid identities independently of
   Agent state; a new Run rejects missing or non-active Agents.
4. Editing `AGENT.md` between Runs changes only the later Run snapshot.
5. Directory escape, root symlink, overlap, missing required content, invalid
   UTF-8, and incorrect write authority fail before execution.
6. Registry responses never expose loaded Agent/knowledge content.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-03
- Status: accepted
