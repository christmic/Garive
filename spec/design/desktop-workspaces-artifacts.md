# A-DESKTOP-WA — Governed Workspaces and artifacts

> This Spec defines the macOS Work-level filesystem boundary: native Workspace
> selection, opaque durable authority, governed read/write/execute effects,
> immutable artifact projection, safe preview/export, revocation, and recovery.

## Status and ownership

Accepted Desktop/Runtime product contract. It refines A-DESKTOP-WORK and
composes C5/C6, H2/H3, M2-D, A-UX1, and macOS sandbox behavior. Runtime owns
durable authority, effects, receipts, and artifact truth. The Tauri backend owns
native selection/bookmarks and bounded OS I/O. React owns presentation only.

V1 is local and macOS 14+. It does not grant general disk or shell access and
does not imply cloud sync, connectors, browser control, or a source-code IDE.

## Product contract

A Workspace is a user-selected local root attached to one or more Sessions by
an explicit grant. The user can see its safe display label and access posture,
but a filesystem path, bookmark, file descriptor, inode, command, or raw
resource key never crosses frontend IPC.

```text
native selection -> validated root -> opaque Workspace grant
                 -> Session attachment -> governed effect
                 -> durable receipt -> artifact revision -> safe preview
```

The Workspace is context and authority, not merely navigation. Selecting a
folder does not read it. Attaching it to a Session does not grant write or
execute. Each effect remains separately prepared, authorized, committed, and
projected through H3.

## Public values

```text
WorkspaceGrantV1 {
  api_version: "v1"
  workspace_id
  display_name
  state: active | unavailable | revoked
  access: WorkspaceAccessV1
  revision
  created_at
  last_verified_at
}

WorkspaceAccessV1 {
  enumerate: bool
  read: none | selected | workspace
  write: none | approval_each | session
  execute: none | approval_each | session
}

WorkspaceEntryV1 {
  entry_id
  parent_entry_id?
  display_name
  kind: directory | text | image | pdf | table | presentation | binary | unknown
  byte_size?
  modified_at?
  selectable
}

SessionWorkspaceAttachmentV1 {
  session_id
  workspace_id
  grant_revision
  access
  attached_at
}

WorkspaceRevocationReceiptV1 {
  schema_version: 1
  workspace_id
  grant_revision
  outcome: revoked | already_revoked
  cleanup_pending
}
```

All identities are opaque lowercase random values bounded to 128 UTF-8 bytes.
`display_name` is the final filesystem component after Unicode normalization,
control-character removal, bidi isolation, and a 128-byte bound. Duplicate
labels are permitted; the UI never reconstructs a path to distinguish them.

`parent_entry_id` expresses hierarchy without revealing a relative path. Entry
IDs bind Workspace revision plus canonical private resource identity. They are
invalid after revocation, replacement, or a root identity change.

## Typed Desktop IPC

```text
choose_workspace(window_id) -> WorkspaceGrantV1 | Cancelled
list_workspaces(cursor?, limit) -> WorkspaceGrantPageV1
verify_workspace(workspace_id) -> WorkspaceGrantV1
list_workspace_entries(workspace_id, parent_entry_id?, cursor?, limit)
  -> WorkspaceEntryPageV1
attach_workspace(command_id, session_id, workspace_id, expected_grant_revision,
                 requested_access) -> SessionWorkspaceAttachmentV1
detach_workspace(command_id, session_id, workspace_id) -> Detached | AlreadyDetached
revoke_workspace(command_id, workspace_id, expected_grant_revision)
  -> WorkspaceRevocationReceiptV1

list_artifacts(session_id, after_position?, limit) -> ArtifactPageV1
get_artifact_preview(artifact_id, revision, preview_kind) -> ArtifactPreviewV1
choose_artifact_export_target(artifact_id, revision) -> ExportTargetCapabilityV1 | Cancelled
export_artifact(command_id, artifact_id, revision, target_capability_id,
                expected_content_digest) -> ArtifactExportReceiptV1
reveal_artifact(artifact_id, revision) -> Revealed | NotRevealable
```

The main-window Tauri capability alone may invoke these commands. Native picker
calls originate in backend code. Picker results and security-scoped bookmarks
never serialize through webview messages, logs, errors, analytics, clipboard,
preferences, or window state.

## Native selection and durable root authority

The picker selects exactly one directory. Backend then:

1. obtains a macOS security-scoped bookmark when sandboxed;
2. rejects an empty/root/home selection unless an explicit product policy
   admits that exact scope;
3. opens the selected directory without following a final symlink;
4. captures private volume/file identity and canonical root metadata;
5. stores the bookmark as a write-only credential-store value under a fresh
   reference;
6. commits an opaque Runtime Workspace grant and non-secret display label;
7. returns only `WorkspaceGrantV1` after both stores are durable.

The bookmark reference is not the Workspace ID. Runtime stores only the opaque
reference/digest necessary to bind authority. Reverification resolves the
bookmark, checks staleness and root identity, refreshes it through the same
staged transaction when macOS requires, then advances grant revision.

A cancelled picker creates no identity or durable record. A crash after
bookmark storage but before grant commit removes the unreferenced bookmark. A
crash after grant commit repairs the non-secret receipt and preserves access.

## Enumeration and context selection

Enumeration is explicit and bounded: maximum depth, entries/page, total scanned
entries, name bytes, metadata calls, elapsed time, and ignored-file rules are
frozen in the grant snapshot. V1 does not recursively crawl on selection.

Backend uses descriptor-relative operations beneath the opened root. It rejects
symlinks, aliases escaping the root, device nodes, sockets, package internals,
and identity changes between validation and open. Hidden files and ignored
directories are excluded by installed policy, not frontend filtering.

Selecting entries creates opaque context references. File bytes go directly
from the bounded backend reader into Runtime context derivation. React receives
only selected display metadata and never file content unless a separate safe
preview contract explicitly returns it.

## Session attachment and authority

One exact `attach_workspace` command binds the current grant revision and an
equal-or-narrower access request to a Session. The composer shows the committed
attachment only after acknowledgement. Draft chips are visually distinct from
committed attachment chips.

Read, write, and execute are separate:

- enumerate permits bounded metadata listing only;
- selected read permits only explicitly selected entry capabilities;
- workspace read permits descriptor-relative read under the root;
- write never follows from read and defaults to per-effect approval;
- execute never follows from write and defaults to per-effect approval;
- network, app control, secrets, and elevated privileges are unrelated grants.

The Runtime capability key contains Workspace ID, grant revision, operation,
and private resource identity. A model-visible path or command is data, never
authority. Every prepared effect names exact resource classes and limits;
authority may narrow but cannot widen them.

## Governed file effects

Installed Work tools initially include bounded `read_file`, `write_file`,
`list_directory`, and `run_verification`. Their raw names stay inside the
snapshot; H3 exposes installed label keys only.

```text
effect.prepared
  -> approval when policy requires
  -> effect.authorized
  -> effect.started
  -> effect.receipt
  -> effect.completed | effect.failed | effect.uncertain
  -> effect.observation
```

Writes use same-directory temporary files, fsync, atomic rename, and directory
fsync. The prepared digest binds expected prior file identity/digest, target
Workspace revision, byte limit, overwrite posture, and verification plan.
Concurrent changes make the plan stale before rename.

Execution uses an installed command descriptor, not arbitrary shell text.
Arguments are canonical structured values. Working directory is the opened
Workspace capability. Environment is an explicit allowlist with no inherited
credentials. Duration, output, process count, and cancellation are bounded.

Unknown effect outcome is `attention_required`; Runtime never retries a
non-replayable mutation without conclusive receipt evidence.

## Artifact model

An artifact is an immutable, user-visible projection of a committed result:

```text
ArtifactV1 {
  api_version: "v1"
  artifact_id
  revision
  session_id
  turn_id
  display_name
  kind
  mime_type
  byte_size
  content_digest
  committed_position
  verification: not_run | passed | failed | partial
  preview: unavailable | text | image | pdf | table | presentation
  workspace_id?
  revealable
  exportable
}
```

Artifact identity is stable across revisions; each revision is immutable. A
new write never mutates an earlier artifact projection. Artifact commit binds
the effect receipt, producing Turn, exact content digest, private backing
capability, and verification receipt in one durable transaction.

Completion copy distinguishes:

- result committed and verified;
- result committed but verification not run;
- result committed with failed verification;
- response completed without an artifact.

The UI must not call work “finished” when a promised artifact failed to commit.

## Preview, reveal, and export

Preview is read authority, not a URL convention. Backend validates artifact ID,
revision, digest, MIME, size, and preview policy before returning one bounded
view:

- text/code: UTF-8, line/byte bounded, escaped, no executable HTML;
- image: decoded dimensions/pixels bounded and re-encoded to a safe bitmap;
- PDF: page/count/byte bounded, rendered in a sandboxed process;
- table: typed cell/page bounds, formulas never execute;
- presentation: static slide render only in V1;
- unknown/binary: metadata only.

Remote URLs, `file://`, arbitrary `data:` values, scripts, active PDF content,
macros, plugins, and web navigation are never trusted preview authority.

Export uses a separate one-shot native target capability. V1 never overwrites:
an existing target returns `artifact_overwrite_required` and the operator must
choose a new filename. The capability binds the opened destination directory,
final component, owner window, five-minute expiry, and exact Artifact
coordinates; no path crosses React. Export rechecks the committed source digest,
writes a same-directory temporary file, fsyncs, atomically creates the new
target, fsyncs the directory, consumes the capability, and returns a receipt.
Before creating the temporary, Desktop atomically journals only the bounded
opaque target ID—never a destination path. A crash leaves that ID recoverable.
On the next explicit native selection of the same destination directory,
Desktop removes only the exact ID-derived temporary and clears the journal.
It cannot scan or reopen arbitrary export directories at launch because that
would broaden the operator's one-shot authority.
Reveal applies only to an artifact already backed by an active Workspace grant.

## Product interaction

The primary flow is:

1. User selects **Add context → Choose folder**.
2. A native picker appears after a plain-language preflight.
3. The composer shows a pending Workspace chip with `Local`, display label, and
   requested read posture.
4. Sending commits Session attachment before starting the Turn; failure leaves
   the draft and pending chip intact.
5. Activity shows only H3 states. An approval card names the safe operation,
   Workspace label, scope, duration, overwrite consequence, and grant duration.
6. Committed artifacts appear inline and in the inspector. Selecting one opens
   its contextual preview without navigating away from the Session.
7. Revoke is available from the chip context menu and Settings → Permissions.

Attention decisions stay inline. The default does not interrupt with a modal.
Keyboard focus returns to the invoking control after picker cancellation,
approval, preview close, export, or error.

## Revocation and recovery

Revocation is monotonic. It blocks new enumeration/context/effects immediately,
invalidates entry/export capabilities, releases the security-scoped resource,
and schedules bookmark deletion. It does not erase prior receipts or artifact
provenance. Existing artifact bytes remain previewable only when backed by a
separate committed artifact-store capability.

Desktop first removes the process-local authority and atomically journals a
private revoked tombstone in manifest schema v2. Keychain deletion then clears
`cleanup_pending`; deletion failure does not reactivate authority. The tombstone
remains path-free and bounded so a restart or repeated command can return
`already_revoked` and retry cleanup without inventing a successful deletion.

At startup, Runtime reconciles grant/bookmark journals before Agent mutation:

- uncommitted bookmark -> delete;
- committed grant with missing receipt -> repair receipt;
- stale bookmark -> mark Workspace unavailable, preserve history;
- revoked grant with bookmark -> retry bounded cleanup;
- started effect without conclusive receipt -> suspend for reconciliation;
- pending path-free export ID -> remove its exact temporary on the next explicit
  authorization of the owning directory, then clear the journal;
- committed artifact with missing index projection -> rebuild from receipt.

All recovery is bounded and idempotent. The UI exposes `Unavailable` or
`Attention required` with exact safe next actions; it never silently selects a
replacement folder.

## Security and privacy

- No path, bookmark, raw resource key, command line, environment, file bytes,
  tool arguments/results, or receipt evidence crosses ordinary frontend IPC.
- All IDs, text, counts, pages, bytes, depth, time, process, preview, and active
  capability collections have non-zero hard bounds.
- Every open is descriptor-relative and rejects links/identity races.
- Workspace and artifact authorization is checked again immediately before I/O.
- Logs contain stable codes, operation classes, public IDs only when policy
  admits them, and aggregate timings; private names are excluded by default.
- Clipboard, drag/drop, Quick Look, Finder reveal, notifications, and recent
  menus require their own explicit product path and never broaden authority.

## Accessibility, localization, and performance

Workspace chips, trees, approvals, artifacts, and previews are fully operable
with keyboard and VoiceOver. State is never color-only. Tree disclosure uses
native semantics; virtualized lists preserve focus and announced position.
Approval receives one assertive summary only when work is blocked.

All labels and failure copy use stable localization keys. User filenames are
bidi-isolated and never interpolated into unbounded accessibility labels.

Targets on a supported warm Mac:

- native picker visible within 300 ms after invocation;
- first bounded root page within 500 ms p95 for a responsive local volume;
- attachment acknowledgement within 100 ms excluding durable Host commit;
- 60 fps scrolling for the maximum rendered entry/artifact page;
- preview cancellation releases work and memory within one second.

## Stable failures

| Code | Meaning |
|---|---|
| `workspace_selection_cancelled` | Native selection was cancelled. |
| `workspace_capability_invalid` | Grant is absent, stale, revoked, wrong-window, or wrong-operation. |
| `workspace_unavailable` | The selected root cannot be safely resolved. |
| `workspace_bound_exceeded` | Enumeration, context, or metadata cannot fit installed bounds. |
| `workspace_attachment_stale` | Session or grant revision changed before commit. |
| `workspace_effect_denied` | Exact read/write/execute authority was not granted. |
| `workspace_effect_uncertain` | Mutation outcome requires reconciliation. |
| `artifact_not_found` | Exact artifact revision is absent. |
| `artifact_preview_unavailable` | Safe preview cannot be produced. |
| `artifact_export_stale` | Artifact or target capability changed before export. |
| `artifact_overwrite_required` | V1 refuses the existing target; choose a new filename. |

Failures never include paths, names, commands, file content, or OS error text.

## Acceptance evidence

1. Shared strict fixtures cover grant/entry/attachment/artifact values, every
   state, unknown strings, bounds, revocation, and stable failures.
2. Backend tests inject picker/bookmark/filesystem ports and prove cancellation,
   symlink/escape/identity-race refusal, opaque IPC, staged crash recovery, and
   bookmark cleanup.
3. File-backed SQLite E2E attaches one Workspace, reads selected input, requests
   exact write authority, atomically writes output, runs verification, commits
   an artifact, restarts, and returns byte-equivalent H2/H3/artifact views.
4. Crash matrices cover bookmark, attachment, effect, rename, receipt, artifact
   index, export, revocation, and cleanup boundaries.
5. React tests cover picker preflight/cancel, pending versus committed chips,
   tree paging, approval, attention, artifact preview/export/reveal, revoke,
   offline/stale states, keyboard, focus, VoiceOver labels, reduced motion,
   200% zoom, CJK, and bidi filenames.
6. Source/wire/log scans prove no path/bookmark/raw fact/tool content crosses
   React IPC and no frontend value can authorize filesystem or process access.
7. Packaged sandbox tests verify Files permission first use, denial, bookmark
   persistence, volume removal/reconnect, sleep/wake, quit/reopen, and uninstall
   data-retention behavior.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
