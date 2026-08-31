# TUI communication and persistence

> This Spec defines the TUI Host port, command/retry protocol, snapshot and
> event synchronization, backpressure, reconnect policy, local file formats,
> crash behavior, privacy, and process-level failure semantics.

## Audience

Engineers implementing `clients/host-rs`, `tui/src/host.rs`,
`tui/src/persistence.rs`, and the Runtime-backed client harness.

## Why

The current Rust Host client supports H1 mutations and blocks until one Turn
terminal. It has no H2 queries, H3 activity value, incremental event API,
canonical JSON continuation, or local pending-command recovery. A competitive
TUI must remain responsive and truthful across disconnect, timeout, restart,
and duplicate replay without becoming another Session database.

## Host port

The application depends on this semantic port, not HTTP or generated Proto:

```text
HostPort {
  list_definitions() -> AgentDefinitionPage
  list_sessions(limit, before?) -> SessionPage
  get_session(session_id) -> SessionView
  get_timeline(session_id, after_position, limit) -> TurnTimelinePage
  create_session(command) -> CreateSessionResponse
  start_turn(command) -> TurnCommandResponse
  cancel_turn(command) -> TurnCommandResponse
  continue_turn(command) -> TurnCommandResponse
  follow_events(session_id, after_position, sink, cancellation) -> FollowEnd
  follow_live_output(session_id, sink, cancellation) -> LiveFollowEnd
}
```

All values are validated domain values from `garive-host-client`. The concrete
adapter owns HTTP/SSE encoding, loopback URL validation, response bounds, and
Host error classification. The TUI owns application retry policy, task
correlation, and user-visible state.

`follow_events` emits each newly accepted event to a bounded sink and returns
only when cancelled or the stream ends/fails. It does not reduce a whole Turn
into one blocking result. It retains H1 rules: exact `api_version = "v1"`,
requested Session, non-zero increasing positions, legal gaps, identical replay,
unknown event preservation, and no terminal at EOF.

`follow_live_output` implements H4's separately named ephemeral channel. It has
no durable cursor, never mutates `observed_position`, and validates one active
stream generation plus contiguous sequence values. Snapshot, gap, overflow,
and durable convergence follow
[`host-live-output-v1.md`](host-live-output-v1.md); H1 backpressure rules below
do not falsely make H4 lossless.

## Launch configuration

```text
garive-tui [OPTIONS]

--host <http://loopback:port/>       required unless embedded launch is admitted
--session <opaque-id>                select after boot
--definition <opaque-id>             preferred definition for new Session
--state-dir <absolute-path>          explicit test/operator override
--theme <system|dark|light|mono>
--screen-reader
--reduced-motion
--mouse <auto|on|off>
--ephemeral                         disable draft/history/preference writes
--no-prompt-history                 disable prompt history only
--version
--help
```

The Host URL remains explicit, HTTP, loopback, root-path-only, credential-free,
and without query or fragment. Unknown options, duplicate single-value options,
relative state paths, invalid enums, and positional text return exit `2` before
terminal acquisition. CLI values override preferences without rewriting them
unless the user changes the corresponding setting in the TUI.

No endpoint, credential, model, database, or Provider value is discovered from
the environment. Platform state-directory resolution may read the standard OS
home/state variables in the composition root only.

## Request values and digests

```text
PendingCommandV1 {
  schema_version: 1,
  command_id,
  kind,
  session_id?,
  turn_id?,
  suspension_id?,
  expected_session_version?,
  requested_through_position?,
  continuation_variant?,
  request_payload,
  request_digest,
  created_at,
}
```

`kind` is `create_session`, `start_turn`, `cancel_turn`, or `continue_turn`.
Fields forbidden by the selected kind are absent, not null. `request_payload`
contains the exact public request value required for byte-equivalent replay:
definition ID, Turn text, cancel watermark, or typed continuation. It never
contains headers, Host URL, credential, provider value, or raw response.

`request_digest` is lowercase SHA-256 of RFC 8785 canonical JSON over every
field except `created_at` and the digest itself. `command_id` is UUID v4 lower
hyphen text generated once. IDs are validated as ASCII, non-empty, and within
H1's 128-byte bound.

The pending record is written durably before sending a mutation. A successful
validated response removes it only after application state has accepted the
matching result. A known Host rejection removes it and records only the safe
code. Transport failure, deadline, process kill, or response validation failure
retains it as unknown.

Exact retry loads the record, recomputes its digest, reconstructs the same Host
request and command ID, and sends no alternative representation. For canonical
JSON continuation the persisted bytes must already be RFC 8785 canonical and
match the durable response-schema digest. A retry never reparses editor text.

Explicit abandonment removes only the local pending record after a confirmation
that states the durable outcome remains unknown. It does not cancel, roll back,
or create another command. The Session then reloads H2 before accepting a new
mutation.

## Snapshot and follow synchronization

Session selection uses this order:

```text
1. cancel the prior selected Session's foreground follow, if outside active bound
2. GET exact Session view
3. GET timeline pages through observed prefix
4. validate one consistent Session identity and watermarks
5. atomically install snapshot in AppModel
6. GET SSE events after observed_max_position
```

Timeline pages may have different latest observations while paging. The
application freezes the first `observed_max_position` as its target and rejects
a page whose semantic watermark moves backward or whose items contain a later
position. When H2 offers no explicit target-prefix query, the client accepts
only pages covered by the first prefix and follows from it; newer changes arrive
through H1. A page never splits a Turn.

An H1 event updates one known Turn/activity or creates the expected newly
started Turn shell. If required user text or suspension data is missing from
the event, the client marks the Session snapshot stale and performs a bounded
H2 refresh; it does not invent content.

H3 activity reduction keys by `(turn_id, activity_id)`. A greater source
position replaces the public state. Equal identical replay is ignored; equal
conflict or backward new state is a protocol error. Activity state never grants
continuation authority.

## Event backpressure

The Host follow adapter and application communicate through a bounded channel
of 256 semantic events. The adapter waits when the channel is full, applying
TCP backpressure rather than dropping durable events. Heartbeats are consumed
inside the adapter and do not enter the channel.

The event loop processes at most 64 Host events per iteration before yielding
to terminal input and cancellation. Multiple events may coalesce into one
redraw, but every durable transition reaches the reducer in order. Content is
bounded by Host limits before it enters the channel.

H4 uses its own 256-value bounded channel. Rapid adjacent text deltas may
coalesce exactly. If the adapter or application falls behind, it clears the
partial preview and reconnects for a current in-memory snapshot; it never drops
an unknown prefix and continues rendering a suffix. H4 processing shares the
64-message event-loop budget without delaying terminal input.

A selected Session has one follow task. Up to four background Sessions with a
running or action-required Turn may retain follows; least-recently-visible idle
follows are cancelled when the bound is exceeded. Cancellation of a follow task
has no Host or Turn semantics.

## Disconnect and reconnect

```text
FollowEnd = Cancelled | Eof | TransportFailure | ProtocolFailure | Deadline
```

`Eof`, transport failure, and deadline set `Disconnected`; they do not change
Turn state. Protocol failure sets `Unavailable` for that Session and requires
an explicit retry after a fresh H2 snapshot.

Automatic reconnect applies only while the selected or background Session has
a non-terminal Turn and no protocol failure:

| Attempt | Delay |
|---:|---:|
| 1 | 250 ms |
| 2 | 500 ms |
| 3 | 1 s |
| 4 | 2 s |
| 5 | 4 s |

Delay uses a monotonic clock. Tests inject the clock and advance it without
sleep. A successful event or H2 refresh resets the attempt counter. After five
failures the UI remains disconnected and offers `/reconnect`; the Turn remains
non-terminal. Network state changes or explicit user action may trigger a new
five-attempt series.

Reconnect always starts from the last accepted cursor. The adapter tolerates
identical replay at or below that cursor under H1 rules. It never increments the
cursor for heartbeat, EOF, malformed event, or a value rejected by the reducer.

## Local state ownership

Runtime/Ledger remains the sole durable owner of Sessions, Turns, transcript,
suspension, activity, terminal state, and cursor truth. TUI local storage owns:

| File | Content | Authority |
|---|---|---|
| `preferences.v1.json` | theme, motion, mouse, selected Session, rail/inspector, bounded drafts | disposable presentation |
| `pending/<session-key>.v1.json` | exact unknown/in-flight mutation envelope | client retry identity only |
| `prompt-history.v1.jsonl` | bounded submitted user prompts | local convenience only |
| `diagnostics/garive-tui.log` | content-free operational events | troubleshooting only |

The opaque Session key used in filenames is lowercase SHA-256 of the Session ID,
not the ID itself. File contents may contain the Session ID where replay needs
it. No completion text, activity payload, public prompt, Host URL, headers,
credentials, provider values, raw body, terminal bytes, or internal facts are
stored locally.

## Preference schema

```text
TuiPreferencesV1 {
  schema_version: 1,
  revision,
  theme,
  reduced_motion,
  mouse,
  selected_session_id?,
  session_rail,
  activity_inspector,
  bell,
  persist_drafts,
  drafts: [{session_id, text, updated_at}],
}
```

Unknown fields, duplicate fields, null scalar values, invalid enums, duplicate
Session drafts, non-monotonic revision, invalid timestamp, too many drafts, or
oversized text reject the entire file. Unknown schema versions preserve no
fields. Defaults are `system`, OS reduced-motion when discoverable otherwise
false, mouse `auto`, expanded rail, closed inspector, bell true, and draft
persistence true.

Limits are 32 drafts, 64 KiB total draft UTF-8, and the Host command byte bound
per draft. Least-recently-updated idle drafts evict first. A draft bound failure
keeps the in-memory draft and surfaces that crash recovery is unavailable; it
never truncates.

## Prompt history

Each JSONL record is strict `PromptHistoryEntryV1` with schema version, opaque
entry UUID, Session ID, submitted text, and RFC 3339 timestamp. It is appended
only after durable start acknowledgement. Exact consecutive duplicates in one
Session coalesce in memory and on compaction.

History is capped at 500 entries and 2 MiB. Compaction keeps newest valid
records within both bounds, writes a new file atomically, and preserves order.
A torn final line is ignored during read and repaired before the next append;
malformed complete lines reject/quarantine the file. History search never reads
Host transcript or other projects.

Sequential Up/Down browsing is an in-memory projection over these validated
records. Its selected index and saved pre-browse draft/cursor are transient UI
state: they are never serialized, appended, compacted, or treated as Host
authority. Returning past the newest entry restores that saved draft exactly;
an edit discards the browse state without changing any persisted history row.

`--no-prompt-history` disables reads and writes without deleting the file.
`--ephemeral` also disables preferences, drafts, pending persistence, and
diagnostic files; mutations are then refused unless the user confirms that an
unknown response cannot survive process exit.

## Clipboard output

Clipboard copy is a one-way terminal presentation effect, never local or Host
state. The controller may pass the last visible completion, selected Session
ID, or exact active composer selection to the single OSC 52 encoder. The
encoder rejects values above 64 KiB, emits one bounded base64 sequence, and is
disabled in screen-reader mode. The TUI never reads the system clipboard and
never writes copied content to preferences, prompt history, pending recovery,
diagnostics, or the Host.

The composer kill buffer is a separate single-entry process-memory component,
not a clipboard adapter. It is bounded by the composer input limit, never
serialized or rendered, survives undo and same-Session draft replacement, and
is cleared before a different Session draft is loaded. Process exit clears it
by construction.

## File durability and concurrency

The default state root follows the platform user-state convention and is
created with owner-only permissions. Unix directories use `0700` and files
`0600`; Windows uses a current-user-only ACL. A path with broader effective
access is rejected for pending commands and prompt history.

The Windows root is `%LOCALAPPDATA%\Garive\tui`. Before terminal acquisition,
the persistence adapter reads the current process-token SID and creates each
missing private directory or file with a protected, non-inherited DACL whose
only ACE grants that SID full control. Opening an existing object verifies by
handle that:

1. it is the expected file or directory kind and not a reparse point;
2. its owner equals the current process-token SID;
3. its DACL is protected from parent inheritance; and
4. its ACL bytes equal the canonical single-current-SID ACL for that object
   kind.

Every existing ancestor below the selected private path is checked for reparse
points before creating descendants. ACL-less filesystems and objects that
cannot be inspected with `READ_CONTROL` fail closed. Windows replacement uses
same-volume `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` and
`MOVEFILE_WRITE_THROUGH`; a new temporary file receives and passes the same ACL
check before replacement. Unix keeps `fsync` plus same-directory rename and
parent-directory sync.

Every state mutation acquires an OS advisory lock on a sibling lock file,
reads and validates the latest revision, writes canonical JSON to a new
same-directory temporary file, flushes file contents, renames atomically, then
flushes the parent directory where supported. Temporary names contain a random
nonce and never user/Session content. Existing destinations are not overwritten
without successful validation of the prior revision.

Preference writers use compare-and-swap on `revision`; a conflict reloads,
merges independent semantic fields, and retries at most three times. Conflicting
edits to the same Session draft keep the newer timestamp only when clocks are
valid; otherwise the current process leaves its draft in memory and reports a
safe conflict.

Pending-command files never merge. An existing different valid digest is
`local_pending_conflict` and blocks the Host mutation. Prompt history append is
serialized by its lock and repairs only a torn EOF fragment.

Startup removes abandoned temporary files only after acquiring the owning lock
and validating their filename grammar. Corrupt primary files move to a unique
`quarantine/` path through atomic rename; failures leave the original untouched.
The UI and log name only the stable error and file category, never content or
full path.

## External-editor transient file

The handoff file is not TUI state. It is created exclusively with private
permissions, removed on every success/error/drop path, and never recovered.
Reads are bounded to the Host draft limit plus a possible final CRLF before
validation. File path, argv, prompt, and edited bytes never enter diagnostics.
`--ephemeral` permits this user-requested transient handoff because it creates
no durable convenience state.

## Crash matrix

| Boundary | Recovery |
|---|---|
| before pending record rename | no Host call; temporary file removed later |
| after pending record rename, before Host call | exact retry available |
| during Host call | result unknown; exact retry available |
| after validated response, before pending removal | H2 refresh then replay same ID; Host idempotency resolves |
| after pending removal, before preference write | Host truth reloads; presentation preference may be stale |
| during preference/history rewrite | old primary remains or torn JSONL EOF is repaired |
| during event follow | cursor reloads from H2 snapshot; no local cursor authority required |

The TUI never concludes that a mutation failed because the pending record
exists. It never concludes that a Turn completed because the TUI process exited.

## Logging and privacy

Diagnostics use a fixed vocabulary and bounded rotation: five files, 1 MiB
each. Fields may include timestamp, build version, event kind, stable error
code, effect kind, retry count, duration bucket, terminal capability flags, and
opaque random trace ID.

Logs and `Debug` must not include user/Agent text, draft/history content,
Session/Turn/Activity/command IDs, Host URL, file path, public suspension text,
JSON schema, raw body, headers, credentials, provider/model values, clipboard,
or environment values. Logging failure never blocks Host truth or terminal
restore.

## Exit codes

| Code | Meaning |
|---:|---|
| `0` | user-requested clean exit after terminal restore |
| `1` | unavailable Host or product failure after UI started and restored |
| `2` | CLI/configuration/TTY/terminal setup error |
| `70` | internal invariant or supervised task failure after restore |

A Turn failure does not exit the resident TUI. Signal exit returns `128 +
signal` on Unix after bounded graceful restore when the platform exposes the
signal number.

## Acceptance

- Rust client fixtures cover H2 queries, H3 values, typed continuation,
  incremental SSE, every stable Host error, redirects, bounds, gaps, replay,
  protocol failures, EOF, cancellation, and deadline;
- reducer/time tests cover snapshot-follow races, five reconnect attempts,
  explicit retry series, active-follow eviction, and backpressure fairness;
- canonical fixtures prove command digest and JSON continuation byte identity;
- file tests inject failure at write/flush/rename/directory-flush boundaries,
  concurrent writers, permission errors, stale temp files, torn JSONL, corrupt
  primary, unknown versions, bounds, and quarantine failure;
- process-kill tests cover every pending-command crash boundary against a
  file-backed Runtime and exact idempotent replay;
- privacy canaries and source scans prove forbidden content never enters local
  state, logs, errors, titles, or debug output;
- restart E2E creates a Session and Turn, kills the TUI at an injected mutation
  boundary, restarts, resolves exact truth, reopens timeline, submits a second
  Turn, and exits with terminal modes restored.

## See also

- [`tui-application-architecture.md`](tui-application-architecture.md) — reducer/effect and terminal ownership.
- [`tui-interaction-and-rendering.md`](tui-interaction-and-rendering.md) — user-visible recovery and command surfaces.
- [`host-api-v1.md`](host-api-v1.md) — H1 durable command and SSE contract.
- [`host-read-model-v1.md`](host-read-model-v1.md) — H2 snapshot/query contract.
- [`host-agent-activity-v1.md`](host-agent-activity-v1.md) — H3 activity contract.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
