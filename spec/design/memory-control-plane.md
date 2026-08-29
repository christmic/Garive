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

The Memory database remains the SSOT. Exported files are immutable snapshots
until an explicit import command is submitted. No watcher, background sync,
merge-on-open, or direct database editing is admitted.

## Snapshot package v1

An export creates one new directory chosen by the caller through a Runtime file
capability. Runtime refuses a non-empty destination and never follows symlinks.

```text
garive-memory-v1/
  manifest.json
  entries/
    <memory-id>.md
```

`manifest.json` is canonical JSON and contains:

| Field | Contract |
|---|---|
| `schema_version` | Exact integer `1`. |
| `export_id` | Non-empty opaque identity generated once by Runtime. |
| `namespace_id` | Exact exported Agent namespace. |
| `through_revision` | Optimistic Memory repository revision. |
| `exported_at` | RFC 3339 display metadata; excluded from semantic entry digests. |
| `entries` | Ordered by raw UTF-8 `memory_id`; each item has file name, authority, lifecycle, revision, and SHA-256 content digest. |

Each entry file is UTF-8 Markdown with one strict YAML-compatible front matter
block followed by the Memory content:

```markdown
---
schema_version: 1
memory_id: mem-01
revision: 4
authority: user_declared
kind: semantic
scope: agent
lifecycle: active
---
Prefer concise status updates.
```

V1 accepts only scalar front matter in the exact declared order. Values use a
restricted ASCII token grammar; quoting, anchors, aliases, tags, comments,
duplicate keys, unknown keys, and extra document markers fail closed. Content
is normalized to UTF-8 with LF endings and exactly one final newline for digest
calculation. Runtime preserves no filesystem metadata as domain truth.

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

The plan contains ordered operations and totals. Operation order is
`memory_id`, then `operation` using `add`, `supersede`, `archive`, `erase`.
Unchanged entries produce no operation. A command retry with the same identity
and byte-equivalent plan returns the original receipt; different bytes conflict.

## Authority semantics

| Edited snapshot entry | Result |
|---|---|
| Existing `user_declared` content | New revision under the same identity; prior revision stays attributable. |
| Existing `agent_learned` content | New `user_declared` entry supersedes it and records the learned identity as provenance; learned evidence is not rewritten. |
| Existing `org_published` content | Rejected; organization publication uses its owning channel. |
| New entry | Allowed only as `user_declared` with a caller-authorized scope no broader than the export scope. |
| Missing entry | No action. Absence is never deletion. |
| Lifecycle changed to `archived` | Archive plan if authority permits. |
| Explicit `erase: true` marker | Erasure plan and existing M1 erasure receipt; content must be otherwise unchanged. |

Imports cannot set confidence/evidence tallies, source facts, promotion links,
timestamps, use counts, exploration weights, or repository revisions. These are
Engine/Runtime facts. Scope widening, lifecycle resurrection, identity change,
and downgrade from user authority fail closed.

## Conflict and recovery

- Every plan binds `export_id`, namespace, input file digests, base entry
  revisions, and `through_revision` into a canonical digest.
- Any repository or affected-entry revision change after planning returns
  `stale_memory_snapshot`; Runtime does not partially rebase.
- The database transaction writes all record revisions, lifecycle changes,
  M1 facts, import receipt, and repository revision together.
- A crash before commit leaves no domain change. A crash after commit returns
  the original receipt when the command is replayed.
- Export is a read snapshot and may be retried to a new empty destination; an
  incomplete destination is never treated as a valid package.

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
