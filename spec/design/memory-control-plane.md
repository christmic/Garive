# M2 — Auditable Memory control plane

> This Spec defines the first user-visible Memory inspection, export, edit,
> import, and erasure workflow. Engine owns deterministic validation and plans;
> Runtime owns files, authorization, persistence, and atomic commits.

## Audience

Engine, Runtime, Kotlin experiment, Host API, and App engineers implementing
Memory transparency without creating a second source of durable truth.

## Why

M0 and M1 govern durable records, recall, maintenance, promotion, and erasure,
but the accepted design still leaves Memory opaque to its owner. A live-watched
Markdown store would introduce split-brain writes, partial file updates, and a
path for users to rewrite Agent-learned evidence. M2 instead provides a bounded,
explicit snapshot workflow whose result is committed through existing Memory
authority and revision rules.

## Ownership and scope

| Owner | Responsibility |
|---|---|
| Engine Memory | Canonical export projection, document validation, diff/plan, authority transition, stable failures. |
| Runtime | Authorization, directory/file I/O, bounds, snapshot read, atomic persistence, receipts, retry identity. |
| Host API | Redacted commands and views after a focused wire increment. |
| Apps | Review, edit, dry-run diff, confirmation, progress, and receipt presentation. |
| Kotlin experiment | Independent semantic planner over the shared fixture; no product file or database adapter. |

Append-only M0 Runtime facts and M2 control-journal events remain durable source
records. Their Runtime-owned Memory repository projection is the sole current
state used by recall/export; exported files are never a second truth source.
No watcher, background sync, merge-on-open, or direct database editing is
admitted.

## Snapshot package v1

An export creates one new directory chosen by the caller through a Runtime file
capability. Runtime refuses a non-empty destination and never follows symlinks.

```text
garive-memory-v1/
  manifest.json
  entries/
    <record-key>.md
    new-<draft-token>.md  # optional user-created import document
```

`manifest.json` is canonical JSON and contains:

| Field | Contract |
|---|---|
| `schema_version` | Exact integer `1`. |
| `export_id` | Non-empty opaque identity generated once by Runtime. |
| `namespace_id` | Exact exported Agent namespace. |
| `through_revision` | Optimistic Memory repository revision. |
| `exported_at` | RFC 3339 display metadata; excluded from semantic entry digests. |
| `entries` | Ordered by raw UTF-8 `record_id`, then `revision_id`; each item binds its file, M0/M1 classification, lifecycle, sensitivity, and digests. |

The exact canonical shape is:

```text
MemorySnapshotManifestV1 {
  schema_version: 1
  export_id, namespace_id, through_revision, exported_at
  entries: MemorySnapshotEntryV1[]
  manifest_digest
}
MemorySnapshotEntryV1 {
  record_id, revision_id, file_name
  authority, memory_type, memory_role, scope, scope_owner_id
  lifecycle, sensitivity
  content_digest, document_digest
}
```

`record_id` and `revision_id` are the exact M0 identities; M2 creates no numeric
entry revision or alias identity. `file_name` is exactly
`entries/<record-key>.md`, where `record-key` is lowercase SHA-256 over UTF-8
`record_id`. `content_digest` hashes the normalized content including its one
final LF; `document_digest` hashes the complete canonical Markdown bytes.
`manifest_digest` is lowercase SHA-256 over RFC 8785 JSON for the manifest with
`manifest_digest` omitted. Entries sort by raw UTF-8 `record_id`, then
`revision_id`; duplicate record/revision identities, file names, or digests fail
closed. Exactly one current revision per record is exported. `exported_at`
participates in the manifest digest but not any entry digest.

Each entry file is UTF-8 Markdown with one strict YAML-compatible front matter
block followed by the Memory content:

```markdown
---
schema_version: 1
record_ref: existing.bWVtLTAx.cmV2LTA0
authority: user_declared
memory_type: semantic
memory_role: preference
scope: agent_instance
scope_owner_b64: YWdlbnQtMDE
lifecycle: active
sensitivity: ordinary
---
Prefer concise status updates.
```

V1 accepts only scalar front matter in the exact declared order. Values use a
restricted ASCII token grammar; quoting, anchors, aliases, tags, comments,
duplicate keys, unknown keys, and extra document markers fail closed. Content
is normalized to UTF-8 with LF endings and exactly one final newline for digest
calculation. Runtime preserves no filesystem metadata as domain truth.

The nine fields shown above are required. An optional tenth field
`erase: true` may follow `sensitivity` and has no other accepted value. It is an
explicit import instruction, never an exported default. A document with erase
set still carries its unchanged content so the planner can bind the destructive
request to the exact exported revision and digest.

Enums are the accepted M1 snake-case names. `memory_role` preserves the M0
content kind; `memory_type` is the orthogonal M1 classification. Import cannot
change type, role, scope, or sensitivity of an existing record. Platform scope,
restricted content, and organisation authority still require their existing
frozen Runtime grants/receipts; text in a file proves none of them.

`scope_owner_b64` is canonical unpadded base64url over the exact opaque
Runtime-authorized scope owner identity. An existing document must preserve it.
A new document may name only one owner in the export's frozen authorized scope
set; Runtime checks the binding again at prepare and commit.

For an exported entry, `record_ref` is exactly
`existing.<record-id-b64>.<revision-id-b64>`. Both components are unpadded
base64url encodings of exact non-empty UTF-8 M0 identities; canonical decoding
requires re-encoding to reproduce each component. A user-created entry instead
uses `record_ref: new.<draft-token>` and file name
`entries/new-<draft-token>.md`, where the token is 1–64 ASCII alphanumeric,
hyphen, or underscore characters and is unique in the package. Runtime treats
it only as import correlation and allocates the real record/revision identities
before returning the plan. A new document is not inserted into the original
manifest; all other unlisted files remain invalid.

## Bounds and privacy

The caller supplies non-zero limits for entry count, total bytes, bytes per
entry, content bytes, and diagnostic count. Export and import fail before a
partial domain commit when any bound is exceeded.

- File names derive only from validated Memory IDs; path separators, traversal,
  absolute paths, hidden files, symlinks, hard-link aliases, and case-folded
  collisions are rejected.
- Export includes content only after explicit read authorization for the exact
  namespace and scopes. It never includes source payloads, credentials, raw
  tool results, deleted content, or secret diagnostics.
- Failures identify the entry and stable code but never echo Memory content.
- Temporary export files are created in the destination and renamed only after
  every digest is durable. Import leaves source files untouched.

## Import plan

Import is two-phase:

1. `prepare_import(snapshot, current_state, limits)` is pure and returns a
   canonical `MemoryImportPlan` or stable failures.
2. Runtime presents the plan. `commit_import(command_id, plan_digest,
   expected_repository_revision)` revalidates authority and revision, commits
   all changes atomically, and appends one receipt.

The exact plan shape is:

```text
MemoryImportPlanV1 {
  schema_version: 1
  export_id, namespace_id, through_revision
  input_manifest_digest, expected_repository_revision
  operations: MemoryImportOperationV1[]
  add_count, supersede_count, archive_count, erase_count
  plan_digest
}
MemoryImportOperationV1 =
  Add {source_draft_token, record_id, revision_id,
       expected_absent: true, document_digest}
  | Supersede {record_id, expected_active_revision_id, new_revision_id,
               authority, document_digest, supersedes_learned_revision_id?}
  | Archive {record_id, expected_active_revision_id, document_digest}
  | Erase {record_id, expected_active_revision_id, document_digest}
```

Runtime allocates proposed new identities before presentation and freezes them
in the plan. `Add.record_id` is a fresh M0 record identity; every add/supersede
revision identity is fresh. Operations order by raw UTF-8 `record_id`, then
variant order `add`, `supersede`, `archive`, `erase`. Counts must exactly match
the list. `plan_digest` is lowercase SHA-256 over RFC 8785 JSON with that field
omitted. Unchanged entries produce no operation. A command retry with the same
identity and byte-equivalent plan returns the original receipt; different
bytes conflict.

The planner input is the verified manifest/documents plus an ordered current
Memory projection containing exact record/revision identities, authority,
type, role, scope/owner, lifecycle, sensitivity, content digest, and
supersession provenance. It performs no I/O, clock read, ID generation,
authorization lookup, or implicit merge.

## Authority semantics

| Edited snapshot entry | Result |
|---|---|
| Existing `user_declared` content | New revision under the same record identity; prior revision stays attributable. |
| Existing `agent_learned` content | New `user_declared` Active revision under the same record identity supersedes it and records the learned revision as provenance; learned evidence is not rewritten. |
| Existing `organisation_published` content | Rejected; organization publication uses its owning channel. |
| New entry | Allowed only as `user_declared` with an exact scope class/owner present in the authorized export scope set; no implicit scope hierarchy is used. |
| Missing entry | No action. Absence is never deletion. |
| Lifecycle changed to `archived` | Archive plan if authority permits. |
| Explicit `erase: true` marker | Erasure plan and existing M1 erasure receipt; content must be otherwise unchanged. |

Imports cannot set confidence/evidence tallies, source facts, promotion links,
timestamps, use counts, exploration weights, or repository revisions. These are
Engine/Runtime facts. Scope widening, lifecycle resurrection, identity change,
and downgrade from user authority fail closed.

Any content edit is planned before a lifecycle-only comparison and must render
the new revision as `Active`; Candidate/Cold/Archived belongs to the replaced
revision and is never copied onto the new user declaration. A content-identical
Archive remains a lifecycle transition and is admitted only from `Cold`.

## Conflict and recovery

- Every plan binds `export_id`, namespace, input file digests, base entry
  revisions, and `through_revision` into a canonical digest.
- Any repository or affected-entry revision change after planning returns
  `stale_memory_snapshot`; Runtime does not partially rebase.
- The Runtime transaction writes the M2 journal event, all record revisions,
  lifecycle changes, import receipt, and repository revision together.
- A crash before commit leaves no domain change. A crash after commit returns
  the original receipt when the command is replayed.
- An unknown export replays only with the same command/capability binding. A
  different empty destination requires a new command and export identity; an
  incomplete destination is never treated as a valid package.

Successful Runtime commands return these exact public receipts:

```text
MemoryExportReceiptV1 {
  schema_version: 1
  receipt_id, command_id, export_id, namespace_id
  manifest_digest, through_repository_revision, entry_count
  receipt_digest
}
MemoryImportReceiptV1 {
  schema_version: 1
  receipt_id, command_id, export_id, namespace_id, plan_digest
  previous_repository_revision, committed_repository_revision
  add_count, supersede_count, archive_count, erase_count
  changed, receipt_digest
}
```

Revisions and counts are unsigned; repository revisions are non-zero while
counts may be zero. Import with any operation requires `changed = true` and
`committed_repository_revision = previous_repository_revision + 1` without
overflow. An empty plan is valid, commits only its audit receipt with
`changed = false`, and preserves the repository revision. Each `receipt_digest`
is lowercase SHA-256 over RFC 8785 JSON with itself omitted. Receipts contain no
path, content, evidence, credential, or hidden authority value. Command replay
returns the byte-equivalent original receipt.

## Durable control journal

M2 does not encode a user control operation as a fake conversation Turn and
does not mutate an M0 fact. Runtime adds one namespace-scoped append-only
Memory journal beside its projection:

```text
MemoryImportJournalEventV1 {
  schema_version: 1
  event_id, namespace_id, command_id, plan_digest
  previous_repository_revision, committed_repository_revision
  operations: ContentBinding
  receipt_digest, event_digest
}
MemoryExportJournalEventV1 {
  schema_version: 1
  event_id, namespace_id, command_id, export_id, manifest_digest
  through_repository_revision, receipt_digest, event_digest
}
```

This journal is not the Session Ledger and has no Session/Turn/Execution ID.
Existing M0/M1 facts remain immutable provenance inputs; their committed Runtime
transaction updates the same Memory repository projection. M2 events are the
source records only for explicit control operations. Recall/export fixes one
repository revision and never merges directly from files or two live stores.

`operations` is canonical JSON for the exact ordered plan operations. Event
digests use RFC 8785/lowercase SHA-256 with `event_digest` omitted. Command ID is
unique per namespace: equal replay requires equal event/receipt bindings;
different semantics conflict. For import, journal event, projection changes,
receipt, and revision commit in one SQLite transaction. For export, Runtime
uses a fsynced recovery journal so a directory rename followed by process loss
either repairs the exact export event/receipt or classifies the destination as
incomplete; it never creates a different manifest under the same command.

Unknown `schema_version` values are rejected; a future reader may add a new
version but cannot reinterpret v1 fields. Export/import canonical bytes are a
cross-language contract, so Rust and Kotlin must produce identical manifest,
document, and plan digests rather than only equivalent outcomes.

## Stable failures

| Code | Meaning |
|---|---|
| `memory_control_unauthorized` | Caller lacks exact namespace/scope/action authority. |
| `memory_export_target_invalid` | Destination is unsafe, non-empty, or cannot be committed. |
| `memory_snapshot_invalid` | Layout, manifest, front matter, digest, or encoding is invalid. |
| `memory_control_bound_exceeded` | A declared count or byte bound was exceeded. |
| `memory_import_forbidden_change` | Input attempts to alter Engine-owned or broader-authority state. |
| `stale_memory_snapshot` | Repository or affected entry changed after export/plan. |
| `memory_import_command_conflict` | Command identity was reused with a different plan. |
| `memory_control_persistence_failed` | Runtime could not durably export or atomically commit. |

## Delivery slices and evidence

| Slice | Evidence |
|---|---|
| M2-A projection/parser | Shared fixture covers canonical export, CRLF normalization, order, unknown/duplicate fields, traversal, symlinks, digest and every bound. |
| M2-B semantic planner | Rust and Kotlin produce equivalent ordered plans for add, supersede, archive, erase, no-op, authority denial, and stale revisions. |
| M2-C durable control | SQLite tests prove atomic multi-entry commit, exact facts/receipts, retry conflict, and crash-before/after-commit recovery. |
| M2-D Host/App | Versioned redacted API plus Desktop UI supports export, editor handoff, dry-run diff, confirmation, commit, erasure warning, and receipt. |

M2-D does not begin until M2-A through M2-C are verified. Importing arbitrary
knowledge graphs, live filesystem sync, automatic conflict merges, organization
publication, and mobile file editing remain outside M2.

## See also

- [`memory-capability.md`](memory-capability.md) — M0 records and authority.
- [`memory-hypothesis-lifecycle.md`](memory-hypothesis-lifecycle.md) — M1 lifecycle, maintenance, promotion, and erasure.
- [`../../docs/architecture/core/memory.md`](../../docs/architecture/core/memory.md) — design context and transparency gap.
- [`../../.agents/dependency-versions.md`](../../.agents/dependency-versions.md) — stable dependency and toolchain admission.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
