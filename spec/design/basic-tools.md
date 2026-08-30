# T1 — Built-in local tool baseline

## Status

Accepted implementation contract for five provider-neutral local tools. T1
uses C4 Prepared Calls, C5b exact access, F0 sandbox enforcement and C5
authority/receipts. Installing the catalogue grants no Agent permission.

## Ownership and naming

`engine/tools` owns immutable definitions, schemas and pure exact-access
resolvers. Runtime owns workspace capabilities and concrete executors. Names
and revisions are constants in one catalogue module, not repeated string
literals across Core, Runtime and clients.

```text
garive.workspace.read_text@1
garive.workspace.list@1
garive.workspace.search_text@1
garive.workspace.apply_patch@1
garive.process.run@1
```

Tool names contain lowercase ASCII segments separated by dots. Revision `1`
means the exact T1 schema, access resolver, sandbox profile, result envelope
and replay class. Any executable change creates a new revision.

## Common input rules

Workspace file paths are non-empty relative slash-separated UTF-8 values
satisfying C5b filesystem-key rules. No absolute path, embedded `.` or `..`,
empty component, backslash, NUL, home expansion, environment expansion, glob
expansion or implicit current directory is accepted. The exact string `.` is
the sole workspace-root identity and is admitted only by directory-valued
arguments explicitly named below; it is never silently inserted.

All schemas set `additionalProperties: false`. Counts and byte limits are
positive interoperable integers bounded again by the Tool Definition and
Runtime snapshot. Defaults are never inserted by C4; the schema requires every
behavior-affecting argument.

Results use valid bounded I-JSON. Ordering below is semantic and deterministic.
Absolute paths, environment values, credentials and raw executor diagnostics
never appear.

The exact revision-1 ceilings are frozen with the catalogue:

| Surface | Ceiling |
|---|---:|
| file or search input file | 1,048,576 bytes |
| file content or process output | 1,048,576 bytes |
| serialized result envelope | 2,097,152 bytes |
| path | 4,096 Unicode scalars |
| literal query | 4,096 Unicode scalars |
| list entries or search matches | 4,096 |
| search traversal nodes | 10,000 |
| patch text | 1,048,576 Unicode scalars |
| patch targets | 128 |
| process argv entries | 256 |
| one argv entry | 32,768 Unicode scalars |
| process duration | 300,000 ms |
| sandbox open files | 64 |
| sandbox processes | 16 |

Read and list definitions use a 5,000 ms execution ceiling; search and patch
use 30,000 ms. Runtime may narrow any caller value or definition ceiling but
cannot widen it without a new Tool revision.

## `garive.workspace.read_text@1`

Input:

```json
{
  "path": "src/main.rs",
  "max_bytes": 65536
}
```

`path` and `max_bytes` are required. The exact access set contains one
`Filesystem(path, Read)`. Requirements are `FilesystemRead`; replay is
`ReadOnly`; F0 requires filesystem scope, symlink containment and resource
limits.

The executor opens from the frozen workspace capability without following
links, reads at most `max_bytes + 1`, rejects non-UTF-8 and returns:

```json
{"path":"src/main.rs","text":"...","byte_count":12,"content_digest":"...","truncated":false}
```

V1 does not truncate file content: exceeding the bound returns
`result_bound_exceeded`. `truncated` is therefore always false but retained as
an envelope compatibility assertion.

## `garive.workspace.list@1`

Input requires `path`, `max_entries`, and `include_hidden`; `path = "."`
explicitly selects the workspace root. The exact access is
one `Filesystem(path, Read)`. Requirements/replay/F0 profile equal read_text.

The executor opens the exact directory capability, reads no entry target and
returns entries sorted by raw UTF-8 name bytes:

```json
{
  "path":"src",
  "entries":[{"name":"lib.rs","kind":"file"}],
  "truncated":false
}
```

`kind = file | directory | symlink | other`. Symlinks are reported but never
followed. Hidden means the name begins with ASCII `.`. More than `max_entries`
returns the first bounded prefix with `truncated: true`; omission is explicit.

## `garive.workspace.search_text@1`

Input requires `path`, non-empty literal `query`, `case_sensitive`,
`max_matches`, `max_file_bytes`, and `max_nodes`. V1 has no regex or glob
grammar. The access is
one rooted `Filesystem(path, Read)` declaration; `path = "."` explicitly
selects the workspace root. The executor recursively
walks only real directories beneath the opened capability, never follows
links, and applies the per-file and total output bounds.

`max_nodes` counts every non-dot directory entry visited, including links and
non-regular files. Exceeding it returns `search_bound_exceeded` without a
partial observation; it is not a truncation signal. Case-insensitive matching
folds ASCII letters only and compares every non-ASCII UTF-8 byte exactly.
Occurrences are non-overlapping and advance left to right.

Matches are ordered by path raw UTF-8 bytes, then one-based line, then one-based
Unicode scalar column. Result:

```json
{
  "matches":[{"path":"src/lib.rs","line":4,"column":9,"preview":"..."}],
  "files_scanned":3,
  "skipped":{"access_denied":0,"non_utf8_content":0,"result_bound_exceeded":0},
  "truncated":false
}
```

Unreadable, non-UTF-8 or over-bound files are counted in the fixed `skipped`
summary above. Links and non-regular objects are intentionally ignored rather
than counted as attempted files. Their raw errors are not exposed. Search sees
one consistent open-by-component traversal, not a workspace snapshot
guarantee.

`preview` is the complete logical line when it has at most 256 Unicode
scalars. Longer lines expose a 256-scalar window beginning at most 96 scalars
before the first matched scalar; a leading or trailing Unicode ellipsis marks
omitted content. The reported column always addresses the original line.

## `garive.workspace.apply_patch@1`

Input requires a non-empty `patch` string in the admitted Garive unified-patch
subset and a non-empty ordered unique `expected_files` array:

```json
{
  "patch":"*** Begin Patch\n...\n*** End Patch",
  "expected_files":[{"path":"src/lib.rs","before_digest":"..."}]
}
```

The pure resolver parses the patch, rejects paths not byte-equal to
`expected_files`, and returns one `Filesystem(path, Write)` per affected file
in canonical order. Rename, delete, binary patch, mode change, absolute path,
symlink target, undeclared file and overlapping hunk are unsupported in v1.
Target extraction admits only `*** Begin Patch`, `*** Update File:`, one or
more `@@` hunk markers, changed/context lines and `*** End Patch`. Every target
must contain a hunk and at least one added or removed line. Each
`before_digest` is exactly 64 lowercase hexadecimal SHA-256 characters.

Requirements are `FilesystemRead + FilesystemWrite`; replay is
`ReceiptRecoverable`. F0 adds filesystem scope, symlink containment and
resource limits. Runtime:

1. verifies each current content digest;
2. constructs every new bounded byte sequence without mutation;
3. writes/fsyncs a workspace journal and same-directory temporary files;
4. atomically replaces files in canonical order;
5. fsyncs directories and commits a receipt containing before/new digests;
6. removes the journal only after durable result publication posture exists.

Recovery uses the journal to finish or reconstruct; it never applies patch
hunks again based only on missing result. Multi-file visibility is transaction-
like only through the Runtime journal/recovery contract, not an OS-wide atomic
rename claim.

Result contains ordered `{path,before_digest,after_digest}` entries and one
receipt digest. File content and absolute temporary locations are absent.

## `garive.process.run@1`

Input requires `lane`, non-empty `argv`, `working_directory`, `max_output_bytes`
and `timeout_ms`:

```json
{
  "lane":"rust-toolchain",
  "argv":["cargo","test","-p","garive-tools"],
  "working_directory":".",
  "max_output_bytes":65536,
  "timeout_ms":30000
}
```

`working_directory = "."` is the explicit workspace root identity; other
values satisfy normal path rules. `lane` uses the C5b process
key grammar. The access set is `Process(lane, Exclusive)` plus
`Filesystem(working_directory, Read)`. V1 declares no network access.

Requirements are `FilesystemRead + Process`; replay is `NeverReplay`. F0
requires filesystem/symlink/process containment, structured arguments,
environment allowlist and resource limits. Runtime resolves `lane` to an exact
configured executable set. It does not search caller PATH, load shell startup
files or accept model-provided environment values.

The executor captures stdout/stderr separately under one aggregate bound and
returns a trustworthy receipt plus:

```json
{
  "exit_kind":"code",
  "exit_code":0,
  "stdout":"...",
  "stderr":"...",
  "truncated":false
}
```

`exit_kind = code | signal | timeout | cancelled`. A timeout/cancellation is a
terminal result only when the executor proves the process tree terminated and
the receipt binds that classification. Otherwise C5 returns uncertainty and
operator reconciliation.

## Catalogue admission

The catalogue constructor freezes:

- exact names, revisions, descriptions and Portable Tool Schemas;
- C4 requirements/replay classes;
- C5b policy/resolver revisions and maximum exact access/result bounds;
- F0 sandbox profiles;
- Runtime executor IDs/revisions and workspace/policy references separately.

An Agent snapshot includes an explicit subset of exact tool revisions. Runtime
refuses a definition without a matching executor binding before the snapshot
is usable. UI catalogue descriptions are projections, never executable source.

A concrete executor receives the same frozen catalogue instance (or an
equivalent snapshot-bound immutable value) used by `ToolPreparationPort`. Its
preflight re-prepares normalized arguments and requires the complete Prepared
Call to match before `effect.started`; matching only a tool name/revision is
insufficient. Dispatch also rechecks the executor ID, revision and deterministic
dispatch-attempt binding selected by preflight.

## Stable safe terminal codes

T1 uses existing preparation/governance failures plus:

`path_not_found`, `path_type_mismatch`, `access_denied`, `non_utf8_content`,
`result_bound_exceeded`, `entry_bound_exceeded`, `search_bound_exceeded`,
`patch_invalid`, `patch_target_mismatch`, `content_changed`,
`patch_conflict`, `process_lane_unavailable`, `process_exit_nonzero`, and
`executor_state_unknown`.

Tool results may expose only these codes and bounded neutral fields. Raw OS
error strings, command search paths and sandbox diagnostics remain Runtime
audit evidence.

## Acceptance evidence

- shared Rust/Kotlin definitions/schema/access/digest fixture;
- pure hostile-input tests for every path, patch, argv and bound rule;
- real Runtime filesystem tests including links, concurrent replacement,
  Unicode/case spelling and result bounds;
- apply-patch journal fault injection before/after each durable/rename point;
- real process argv/environment/cwd/output/tree-termination/no-network tests;
- C4/F0/C5 end-to-end tests proving invalid, denied or unsupported calls never
  start and uncertain process state never replays;
- source scan proving no tool or adapter reads process environment implicitly.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
