# M2-D — Desktop Memory review and control

> This Spec exposes M2 snapshots and import plans through typed local Desktop
> IPC with backend-owned native file capabilities. React can review and confirm
> bounded changes but never receives arbitrary paths, database access, or hidden
> Memory evidence.

## Audience

Runtime Memory, Desktop backend/frontend, and product test engineers completing
the first user-visible Memory transparency workflow.

## Why

M2-A through M2-C define canonical documents, pure planning, and atomic durable
commit. They do not define how a user chooses a directory, opens an editor,
reviews authority changes, confirms erasure, or receives a durable receipt.
Passing raw paths or database handles through React would bypass Tauri and
Runtime authority. M2-D adds opaque short-lived file capabilities and exact UI
states.

## Scope and ownership

V1 is Desktop-local. The Tauri backend owns native file/directory selection,
path validation, symlink refusal, bounded I/O, Runtime authorization, and M2-C
commands. Frontend owns presentation only. Web/mobile Memory control requires a
separate authenticated content-transfer contract and is not implied.

## Typed IPC

```text
choose_memory_export_target() -> MemoryFileCapabilityV1 | Cancelled
export_memory_snapshot(command_id, capability_id, scope) -> ExportReceiptV1
choose_memory_import_source() -> MemoryFileCapabilityV1 | Cancelled
prepare_memory_import(capability_id) -> MemoryImportReviewV1
commit_memory_import(command_id, plan_digest,
                     expected_repository_revision,
                     confirmation) -> ImportReceiptV1
discard_memory_import(plan_digest) -> Discarded | AlreadyCommitted
reveal_export(capability_id) -> Revealed
get_memory_command(command_id) -> Unknown | Pending |
                                  Committed {receipt} | Failed {code}
```

`MemoryFileCapabilityV1` contains only an opaque random identity, operation
`export_target | import_source`, bounded display name, expiry, and state
`fresh | consumed | export_committed`. It contains no path. It is process-local,
main-window-bound, non-serializable to preferences, and invalid after
expiry/restart/discard. Import consumption is terminal; a committed export may
be used only by `reveal_export` until expiry.

Export selects an empty directory and invokes M2-C. `ExportReceiptV1` returns
export identity, manifest digest, entry count, through revision, and display
name. `reveal_export` asks the backend to reveal that already authorized
directory using a native OS action; it cannot open another path.

## Review model

```text
MemoryImportReviewV1 {
  schema_version: 1
  plan_digest, export_id, expected_repository_revision
  totals, warnings[]
  changes: MemoryChangeReviewV1[]
  expires_at
}
MemoryChangeReviewV1 {
  operation, record_id, memory_type, memory_role, scope, sensitivity
  authority_before?, authority_after?
  lifecycle_before?, lifecycle_after?
  content_before?: MemoryContentReviewV1
  content_after?: MemoryContentReviewV1
  destructive, provenance_note_key?
}
MemoryContentReviewV1 =
  Visible { text } | Redacted { content_digest, utf8_bytes }
```

Changes use M2 canonical order. Content is included only after exact read
authority and under independent item/total byte bounds. Restricted content is
shown only when the current Desktop actor holds its frozen grant; otherwise the
review returns a redacted digest/size marker and cannot commit that change from
this UI. Warnings and provenance notes are stable localization keys.

An Agent-learned edit displays that the imported text will become a new
User-declared supersession, not rewritten evidence. Organisation-published
changes are rejected before review. Archive and erase are visually distinct;
erase requires a second explicit confirmation naming the number of entries and
cannot be combined with an unchecked bulk action.

`confirmation` is exactly `None` when `erase_count = 0` and exactly
`ConfirmErase { plan_digest, erase_count }` when it is non-zero. Any mismatch
fails before Runtime commit. The backend plan, not a frontend count, remains
authoritative.

## State machine

```text
Idle -> Choosing -> Exporting -> Exported | Failed
Idle -> Choosing -> Preparing -> Reviewing -> Committing -> Committed | Failed
Reviewing -> Discarded
Reviewing/Committing -> Stale
```

Closing/navigation during Choosing cancels the native picker. Closing during
Exporting/Committing does not cancel a submitted durable command; reopening
queries the backend by command identity and renders its known receipt/unknown
state. The frontend never generates a replacement command automatically.

The review is immutable. Editing happens in the user's external editor between
export and import selection. M2-D does not embed a Markdown editor or watch
files. After prepare, changed source bytes make the plan stale at commit.

## Command, privacy, and recovery rules

- Command identities and byte-equivalent retry follow A1. Plan identity and
  expected revision follow M2.
- Backend stores at most a bounded number of active capabilities/plans and
  zeroizes in-memory restricted content when discarded/expired.
- IPC may contain only the authorized bounded `Visible.text` or redacted marker
  in `MemoryImportReviewV1`. IPC otherwise, and all log/debug/analytics values,
  contain no paths, full manifests, raw files, Memory content, credential data,
  evidence payloads, or repository internals.
- A successful response is emitted only after the export rename or import
  database transaction and receipt are durable.
- File capability loss after restart does not affect committed receipts.
  Uncommitted temporary export directories are recovered/removed by M2-C.

## Stable failures

| Code | Meaning |
|---|---|
| `memory_file_selection_cancelled` | User cancelled; not an error alert. |
| `memory_file_capability_invalid` | Capability is wrong, expired, used, or owned by another window. |
| `memory_review_bound_exceeded` | Safe review cannot fit configured bounds. |
| `memory_review_redacted_change` | A proposed change cannot be confirmed without content authority. |
| `memory_import_stale` | Files or repository changed after prepare. |
| `memory_import_confirmation_required` | Destructive changes lack exact second confirmation. |
| `memory_control_failed` | Stable mapped M2/Runtime failure; details remain backend-only. |

## Acceptance evidence

- typed IPC fixture covers capability lifecycle, export receipt, every review
  operation, learned-to-user supersession, redaction, warnings and errors;
- backend tests use native-picker and filesystem ports, prove paths never cross
  IPC, reject symlinks/non-empty targets, and recover every export/import crash;
- temporary SQLite E2E exports, edits, prepares, commits and re-exports the
  exact new revision; stale file/repository and retry conflicts commit nothing;
- React tests cover cancellation, review, external-editor handoff, destructive
  confirmation, close/reopen unknown command, keyboard/focus and bounded lists;
- source/log scans prove no arbitrary filesystem command, path/content logging,
  frontend database import, file watcher, or hidden organization edit path.

## See also

- [`memory-control-plane.md`](memory-control-plane.md) — M2 canonical package,
  plan, authority, and durability rules.
- [`client-product-experience.md`](client-product-experience.md) — A-UX1 state,
  errors, privacy, and accessibility.
- [`desktop-configuration-onboarding.md`](desktop-configuration-onboarding.md)
  — the separate write-only system setup channel.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
