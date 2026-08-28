# Ledger — Session-as-Directory Append-only Log

> **One session = one directory.** Inside the directory:
> a SQLite database (the "container"), and a blob store
> keyed by content hash for large objects (files, snapshots,
> multi-KB outputs). The ledger is the single source of truth
> for an `agent_turn`'s state — append-only, durable, and
> replayable. Together with the loop (`loop.md`), the ledger
> implements "the model never loses information that matters,
> and never re-pays for information it once saw."

This document describes the ledger **as a design**. Specific
SQLite pragmas, index DDL, hash function choice, and blob
file naming land with the slice — the *mechanism* (session-
as-directory, content-hash addressing, append-only) is what
the codebase ships.

## TL;DR

Four invariants define the ledger:

1. **Session = directory.** One `agent_turn`'s (or longer-
   running session's) data lives under `<root>/<session>/`.
2. **Blobs are external + content-addressed.** A large body
   (file, snapshot, KB-plus output) is stored as
   `<root>/<session>/blobs/<sha256>`; the db row carries
   the hash, not the bytes.
3. **SQLite is the container.** Schema: a main table (one
   row per ledger entry), a dedup table (one row per unique
   blob hash), a blob registry (which hashes exist where),
   and a metadata table (session / agent / model / run tags).
4. **The ledger never forgets.** The surface is a lossy
   projection; this document describes the durable store
   underneath.

## Context

The loop needs to answer four questions reliably:

1. **Recoverable:** if the process dies mid-round, where
   should it resume? Answer: read the directory.
2. **Auditable:** why did this round call this tool at this
   iteration? Answer: read the main table.
3. **Bounded:** the model's context window is finite; how
   do we give it the right slice? Answer: derive from the
   main table + blob registry, projecting into the surface.
4. **Multi-language:** Rust and Kotlin both need to read the
   same data with the same semantics. Answer: SQLite is the
   lingua franca; both languages use the same `.sql` schema.

These together rule out an in-memory-only store (no recovery)
and a hand-rolled binary log (no schema, no cross-language).
A SQLite-backed session directory with a content-addressed blob
store fits.

## Options Considered

### A. In-memory only

`Vec<Entry>` in process memory.

Rejected. Loses everything on crash. Cannot recover.

### B. Plain text log (JSONL), one file per session

One JSON object per line, big bodies inlined.

Rejected. Inline bodies turn the log into a giant string per
row; parsing on every read is slow without an index;
cross-language readers would need to agree on serialisation
order; no schema enforcement.

### C. SQLite per session + content-addressed blobs

A small embedded SQLite db for entry metadata + a sidecar
blob store keyed by content hash for large bodies.

**Selected.** Durable, queryable, well-understood. Blobs live
next to the db (so a session directory is fully portable —
`tar` it, move it, restore it). Hash-based content addressing
gives dedup for free (a 50 MB log read twice is one file on
disk).

### D. CRDT / event-sourced framework

e.g. `durable-streams`, `eventfold`, custom CRDT.

Considered. Overkill for a single-process writer. Future
option if multi-process writers become a requirement.

### E. Custom binary log with sidecar index

Tempting for performance; rejected for maintenance cost and
reinventing SQLite badly.

## Decision

The ledger is **one directory per session**, containing a
SQLite db + a blob store + (optionally) sidecar indexes.

### Directory Layout

```
<root>/
└── <session>/
    ├── ledger.db          SQLite database
    ├── blobs/             content-addressed blob store
    │   └── <sha256>       file with the bytes; one per unique hash
    ├── index/             optional sidecar indexes (search, etc.)
    └── meta.json          session-level metadata (created_at, agent, model, …)
```

A session may span multiple `agent_turn`s; `turn_id` is a
column in the main table, not a directory. The directory
boundary is **session**, not **turn** — so a multi-turn
session is one directory; a single-turn agent run is also one
directory.

### SQLite Schema (4 tables)

The container is a small SQLite database with **four tables**.
A row in `entry` is the durable record of one ledger event;
a row in `blob` is the durable record of one blob's existence
and location; `entry.blob_ref` points at `blob.hash`.

#### `entry` — main table

One row per ledger event. Append-only: rows are never
updated or deleted in place (only superseded logically —
see `entry.superseded_by` below).

| Column | Type | What it holds |
|--------|------|---------------|
| `seq` | INTEGER PRIMARY KEY | Monotonic per session. Strictly increasing. |
| `turn` | TEXT | `turn_id` (UUID). The agent_turn this entry belongs to. |
| `step` | INTEGER | Iteration index within the turn. `0` for user entry, `n` for the n-th `iteration`. |
| `kind` | TEXT NOT NULL | Dotted kind name (e.g. `assistant.text`, `tool.call`, `tool.result`, `summary.v1`, `rewrite_directive`). |
| `provenance` | TEXT | Opaque source tag: which model, which tool, which rule produced this entry. |
| `surface_visible` | INTEGER (bool) | Whether this entry is *currently* visible on the surface. Defaults to `1`. Gets `0` when the entry has aged out / been compressed / been evicted. |
| `pinned` | INTEGER (bool) | `1` if the entry is **pinned** — never compressed, never evicted, always on the surface. `goal` and `system` are pinned by default; the model can request pin via intent; `governance.judge` can mark entries as pin-worthy. |
| `pair_ref` | INTEGER | `seq` of the paired entry (e.g. `tool.call` ↔ `tool.result`, `approval_request` ↔ `approval_response`). `NULL` if unpaired. |
| `schema_var` | INTEGER | Schema version of this entry's payload. Lets readers tolerate old / new fields without parsing errors. |
| `wall_ts` | INTEGER | Wall-clock time of append (Unix epoch milliseconds). |
| `body_hash` | TEXT NULL | Content hash (`sha256:<hex>`) of the body if the body is externalised in the blob store. `NULL` if inline. |
| `body_inline` | BLOB NULL | Inline body for small entries (≤ a configurable byte threshold). `NULL` if externalised. |
| `body_size` | INTEGER | Size in bytes (whether inline or externalised). |
| `covers_start` | INTEGER NULL | For `summary.v*` entries: the start of the `seq` range it replaces. |
| `covers_end` | INTEGER NULL | For `summary.v*` entries: the inclusive end of the `seq` range it replaces. |
| `superseded_by` | INTEGER NULL | If this entry was logically replaced (e.g. by a summary), points at the replacing entry's `seq`. Enables history walks without `covers_*` joins. |
| `ext` | BLOB | Extension bag (kind-specific fields). Indexable per kind as needed. |

The pair `(body_inline, body_hash)` is "small body in db" or
"large body by hash" — pick one per row based on a size
threshold (e.g. 4 KiB inline, anything larger externalised).

#### `blob` — content-addressed registry (no refcount)

A blob lives at `<session>/blobs/<sha256>` on disk and is
referenced by hash from `entry.body_hash`. The `blob` table
is the **single index** that maps hashes to file paths and
records metadata — **and nothing more**. There is no
reference count.

| Column | Type | What it holds |
|--------|------|---------------|
| `hash` | TEXT PRIMARY KEY | `sha256:<hex>` — the content hash. The row's *identity*; also the on-disk filename. |
| `size` | INTEGER | Bytes on disk. |
| `mime` | TEXT | Best-guess MIME type. |
| `wall_ts` | INTEGER NOT NULL | Wall-clock of when the blob was registered. Diagnostic; used by archive sweeps. |
| `path` | TEXT NOT NULL | Relative path under the session directory (`blobs/<hash>`). |

Multiple `entry` rows referencing the same blob hash **do
not** duplicate bytes on disk — dedup is by content hash. The
table itself only knows the blob exists, where it is, and
how big it is.

#### Why no `refcount`

We deliberately do **not** track per-row references in
`blob`. The lifecycle is governed by **session activity**, not
by ledger semantics:

- **Active sessions** — defined as "any session with at least
  one `entry` append in the last N minutes" — **never have
  their blobs garbage-collected**. A blob in an active
  session may be referenced by N entries, 0 entries, or any
  count; the activity rule wins.
- **Idle / archived sessions** — defined as "no appends in N
  days" — are eligible for **whole-session archival**:
  `entry` rows go to cold storage, the `entry` table is
  trimmed, and the corresponding blob files are unlinked.
  The sweep runs once per `ops_log` row.

The implication: a blob in an active session is **never
deleted**, even if no live entry references it. A blob in an
idle session goes when the whole session goes. There is no
"GC this one blob because its refcount dropped to zero"
path — that path is more complex than the use case warrants
and creates windows where data can leak between sessions.

#### Integrity checks

The blob table is the source of truth for **what files
should exist** under `<session>/blobs/`. Three checks fall
out of that:

1. **Hash verification (rare, on suspicion).** When something
   looks wrong (a read returns garbage, a checksum mismatch
   surfaces downstream), `verify_blob(hash)` reads the file
   from disk, computes `sha256`, and compares to `blob.hash`.
   Mismatch → log to `ops_log` as `blob_corruption`,
   **do not delete or rewrite** (the file may still be the
   right bytes — corruption in our SHA-256 is far less likely
   than a bug in our SHA-256 implementation). Repair is a
   manual operator decision.
2. **Spot-check on session open (cheap).** When a session
   directory is mounted (process restart, attach tool,
   inspection), the runtime walks `blob` rows whose
   `wall_ts > session_recent_threshold` and checks that the
   file exists at `blob.path`. Missing files are flagged.
   The runtime **does not** auto-repair or panic; it logs
   `blob_missing` to `ops_log` and continues with whatever
   blobs do exist. The session is recoverable from the
   `entry` table — the model reads `entry.body_hash`, the
   blob lookup fails, the surface shows a `seq_pointer` with
   "blob missing", and the loop recovers.
3. **Archive sweep (cold storage).** When a session ages
   past the active-session threshold, an archive sweep
   moves its `entry` rows to cold storage and unlinks the
   blob files. The sweep **does not** re-verify the blob
   contents (that's expensive); it relies on the file
   existing from the previous spot-check. A second spot-check
   runs on the cold-storage mount, separately, if needed.

The `ops_log` table records every check and every sweep:

```
{'op': 'blob_spotcheck', 'started_at': …, 'finished_at': …,
 'items_removed': 0, 'notes': '{"checked": 47, "missing": 1, "missing_hashes": ["sha256:..."]}'}
```

Missing blobs are a **loud signal**, not a silent failure.
The runtime continues — the session is still useful, just
with one or two holes.

#### `dedup` — idempotency table (the *client_generation* key)

`dedup` is **not** content-dedup (the blob store already
dedups by content hash). It is the **idempotency table** for
writes: the loop client retries writes with the same
`client_generation` when a previous attempt crashed / timed
out; the server detects the duplicate and returns the
authoritative `seq` of the original write.

| Column | Type | What it holds |
|--------|------|---------------|
| `client_generation` | TEXT PRIMARY KEY | Idempotency token supplied by the client (UUID, monotonic counter, or whatever the client uses). Same token across retries = same write. |
| `body_hash` | TEXT NOT NULL | The body the original write carried. Lets a retry verify intent ("same body? then it's a true retry; different body? that's a new write under a reused token — surface as a conflict"). |
| `seq` | INTEGER NOT NULL | The authoritative `seq` returned to the client on the first write. This is what a duplicate retry gets back. |
| `wall_ts` | INTEGER NOT NULL | Wall-clock of the first write. Diagnostic only. |

**Write semantics:**

```
def append(entry, client_generation):
    row = dedup.lookup(client_generation)
    if row is not None:
        if row.body_hash == hash(entry.body):
            return row.seq                 # true retry — return the original seq
        else:
            raise IdempotencyConflict    # same token, different body — explicit conflict

    # first-write path
    new_seq = entry_table.append(entry)
    dedup.insert(client_generation, hash(entry.body), new_seq, now())
    return new_seq
```

`IdempotencyConflict` is a deliberate error — it signals that
the client reused a token with intent that doesn't match the
original write. It's not silently overwritten; it's surfaced
to the caller. The caller decides whether the new body is a
real update (generate a fresh token) or a bug (fix the
caller).

**Why `body_hash` is part of the key, not the whole key:**

- Pure-`hash` dedup: any retry that happens to have the same
  body succeeds silently, but a retry with a different body
  silently overwrites. Wrong.
- Pure-`client_generation`: same token must mean same body.
  Different body under same token is a bug. Right.
- `client_generation` + `body_hash`: same token + same body
  is a retry; same token + different body is a conflict.
  Both detected at write time.

The `dedup` row is small and writes are cheap (an `INSERT OR
IGNORE` per append). GC walks the table periodically and
removes rows older than a TTL — see `ops_log` below.

#### `ledger_meta` — session-level KV (structured keys)

Session-scoped tags. The table is a small KV; the *keys* are
constrained to a documented set so readers know what to
expect. Anything outside the set is an extension (still
allowed) but should not be the only source of truth.

| Column | Type | What it holds |
|--------|------|---------------|
| `key` | TEXT PRIMARY KEY | A documented tag name (see below). |
| `value` | TEXT NOT NULL | The tag value. Most are strings or JSON-encoded objects. |
| `wall_ts` | INTEGER NOT NULL | When the tag was last set. |

##### Documented keys

| Key | Type | What it holds | When set |
|-----|------|---------------|----------|
| `schema_version` | string | The version of the ledger schema this db was created under (e.g. `"v1"`). Read by readers to migrate old rows on read. | once at session creation; only changed by a migration op. |
| `session_id` | UUID (string) | The identity of this session. Matches `<root>/<session>/` directory name. | once at session creation; immutable. |
| `mode` | enum string | `"interactive"` \| `"batch"` \| `"ci"` \| `"replay"` \| … — how this session is being driven. | once at session creation; immutable for the session. |
| `agent` | string | The agent identifier this session is running (`"garive-default"`, `"garive-experimental"`, …). | once at session creation. |
| `created_at` | INTEGER (epoch ms) | Wall-clock the session directory was created. | once at session creation; immutable. |
| `last_active_at` | INTEGER (epoch ms) | Wall-clock of the most recent `entry.append` in this session. Updated on every write; read by `ops_log` GC sweeps to identify idle sessions. | updated on every write. |
| `lineage` | JSON | `{"parent_session": uuid \| null, "boundary_seq": int \| null}` — for a forked session, where it branched from. For a root session, both fields are `null`. See "Lineage and fork" below. | once at session creation; immutable. |
| `memory_watermark` | INTEGER (seq) | The latest `seq` whose contents have been **extracted into long-term memory**. The next `extract_memory()` call resumes from `seq + 1`. See "Memory extraction watermark" below. | updated by the memory-extraction op. |
| `backup_watermark` | INTEGER (seq) | The latest `seq` that has been **safely persisted to the next backup tier** (e.g. shipped to cold storage). Used by **incremental backup** — only entries with `seq > backup_watermark` need to ship. | updated by the backup op. |

##### Lineage and fork

A session may be **forked** from another session. The fork creates
a new `<root>/<session>/` directory whose `lineage` row points
back at the parent:

```
{"parent_session": "uuid-of-parent", "boundary_seq": 42}
```

- `parent_session` is the UUID of the session being forked from.
- `boundary_seq` is the `seq` in the parent up to (and including)
  which the fork inherits entries. Entries with
  `seq > boundary_seq` in the parent are **not** visible to the
  fork.

**Root sessions** carry the literal `null` lineage:

```
{"parent_session": null, "boundary_seq": null}
```

The lineage is written **once at session creation** and is
immutable for the lifetime of the session — a fork never
re-parents, and a root session never sprouts a parent. Cross-
session reads that need to follow lineage (e.g. "did the
parent already see this file?") use `ledger_meta` + the parent's
db.

##### Memory extraction watermark

A periodic op walks the ledger and extracts durable facts
into long-term memory (e.g. "the user prefers tabs over
spaces", "the project uses Cargo workspace"). The
`memory_watermark` row marks progress so the op can resume
after a crash.

- On each successful extraction pass, the op updates
  `memory_watermark = last_extracted_seq`.
- On restart, the op resumes from `seq = memory_watermark + 1`.
- A `memory_watermark` smaller than `entry.latest_seq()` is the
  signal that an extraction is in flight (or crashed).
- Multiple extractors may run concurrently if gated by
  `client_generation` (see `dedup` table); the watermark
  update is itself idempotent.

##### Backup watermark (incremental backup)

A periodic op ships new ledger entries to a backup tier (cold
storage, off-site, durable beyond the SQLite file).
`backup_watermark` marks progress.

- The backup op reads `entry WHERE seq > backup_watermark`,
  serialises them (along with referenced blobs), ships, and on
  success sets `backup_watermark = last_shipped_seq`.
- A fresh process / attach tool reads `backup_watermark` to
  know "what's already off-site" — restoring from backup
  starts at `seq = backup_watermark + 1`.
- This is **incremental backup**: only the delta ships each
  cycle. The full ledger can be reconstructed by replaying
  backup-watermark checkpoints.

`ops_log` records every memory-extraction pass and every
backup pass — `notes` carries the relevant watermarks.

#### `ops_log` — operations and GC history

Background operations on the ledger — most importantly the
**dedup GC** that reclaims old `client_generation` rows,
but also vacuum, schema-migration history, and any other
maintenance work — record their runs here for auditing.

| Column | Type | What it holds |
|--------|------|---------------|
| `id` | INTEGER PRIMARY KEY | Auto. |
| `op` | TEXT NOT NULL | Operation name (`dedup_gc`, `ledger_vacuum`, `blob_compact`, `schema_migrate`, …). |
| `started_at` | INTEGER NOT NULL | Wall-clock. |
| `finished_at` | INTEGER | `NULL` while running; set on completion. |
| `items_removed` | INTEGER | Count of rows / files cleaned. `0` for non-cleanup ops. |
| `notes` | TEXT | Free-form. Diagnostic only. |

**Dedup GC** (the headline op) walks `dedup`, removes rows
whose `wall_ts` is older than a TTL (e.g. 7 days), and writes
one `ops_log` row summarising the run. The TTL is a config
knob; the table itself does not enforce it.

Why `ops_log` is inside the db: GC, vacuum, and migration
runs are themselves part of the ledger's audit story.
A future "what happened to this session" query joins
`entry`, `dedup`, and `ops_log` together.

### Entry Kinds (by family, with body schemas)

Kinds are **dotted, lower-case, family-prefixed**. The
prefix is the family; the suffix names the specific event.
Adding a new kind is **a row in the catalog** plus a typed
payload — nothing else changes.

Body schemas below are normative: the wire format in
`spec/proto/` must match these shapes. Inline bodies carry
the value directly in `entry.body_inline`; large bodies
reference a blob hash in `entry.body_hash`.

#### `text.*` — text exchanges

The free-form text the model and the user exchange. Both
share the same body shape (`{text: string}`); the family
prefix distinguishes the **producer** so `derive` can filter.

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `text.user` | `{text: string}` | runtime | One per user message entry. Pinned on the system side per request. |
| `text.assistant` | `{text: string}` | model | Model's reply text before judge. May be large (long explanations) → externalise. |

#### `tool.*` — tool calls and results

The model's verb (call) is paired with the runtime's
response (result) via `pair_ref`. Body shape:

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `tool.call` | `{name: string, args: Value, call_id: string}` | model | One per model intent. `call_id` is the model-side identifier; the result row carries the same `call_id` + a `pair_ref` to this `seq`. |
| `tool.result` | `{call_id: string, status: enum{ok, error, timeout}, output: OutputPayload}` | executor | Pairs with `tool.call`. `output` is one of two shapes (below). |
| `tool.result.rejected` | `{call_id: string, reason: string}` | executor | When governance denied. Carries the reason back to the model. Inline (reasons are short). |

`OutputPayload` (the `output` field of `tool.result`):

| Shape | When | Form |
|-------|------|------|
| `{inline: {text: string}}` | small body (< threshold) | `entry.body_inline` carries the text directly |
| `{blob: {hash: string, preview: string}}` | large body (≥ threshold) | `entry.body_hash` carries `sha256:<hex>`; `preview` is a one-screen rendering (head + tail + size) for the surface even when the full blob is offloaded |

The threshold for "small vs large" is configurable (default
~4 KiB) and may eventually be per-kind. The `preview` is a
** surface hint, not the body itself** — the body lives in
the blob store.

#### `governance.*` — verdicts, approvals

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `governance.verdict` | `{decision: enum{approve, deny, rewrite, ask_user}, rule_id: string, evidence_ref: seq}` | governance | One per `tool.call`. `evidence_ref` is the `seq` of an entry (typically the `tool.call` itself, or an earlier context entry) that triggered this verdict. |
| `governance.approval_request` | `{question: string, blocking: bool}` | runtime | AskUser verdict. Round pauses here. `blocking=true` → the round **must** pause; `blocking=false` → optional (the runtime may continue without waiting). |
| `governance.approval_response` | `{question: string, answer: enum{approve, deny}, notes?: string}` | runtime | Human's reply on Resume. `pair_ref` → the `governance.approval_request`. |

#### `compaction.*` — summarisation and rewrite directives

These replace the older `summary.v1` / `rewrite_directive`
names. The prefix `compaction.` makes the family explicit;
together they form the **compression machinery**.

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `compaction.summary` | `{text: string, structured: SummaryFields}` | model | Structured summary. `covers_start` / `covers_end` set on the row. `structured` is the `SummaryV1` shape from `loop.md` (`goal_progress`, `confirmed_facts`, `actions_taken`, `state_progress`, `open_questions`). `text` is a free-form narrative companion to the structured fields — the model produces both; `derive` uses `structured` for queryable facts and may show `text` for context. |
| `compaction.rewrite` | `{covers: {from: seq, to: seq}, generation: u32, summary_seq: seq}` | runtime | Signals `derive` to reset its surface cache. `covers` is a structured range (`from` inclusive, `to` inclusive). `generation` increments each time a re-compression lands on the same prefix (so the model can tell "this is the third summary of these same N turns"). `summary_seq` points at the `compaction.summary` this directive supersedes. |

The `compaction.rewrite.covers` is `{from, to}` — a range
object — not two flat columns, because the range's semantics
("from inclusive, to inclusive") belong with the data, not
with the row schema.

#### `harness.*` — platform-specific injections

Used to feed runtime / harness signals into the ledger so
the model can read them via `derive`. Examples include
IDE state snapshots, test runner outputs, env metadata.

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `harness.feature` | `{feature: string, content: Value}` | runtime | A single feature flag / injection. `feature` names the feature (`"vscode.diagnostics"`, `"cargo.test_results"`); `content` is its structured payload. Multiple `harness.feature` entries may coexist — each describes one feature of the runtime state. |

`harness.feature` is the **generic slot** for platform-specific
data. Adding a new feature = appending a new `harness.feature`
entry; no new kind is needed unless the feature has
distinct lifecycle requirements.

#### `session.*` — turn and session boundaries

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `session.turn_start` | `{turn: {id: uuid, boundary: enum{user_message, resume, fork}, compression: enum{ok, partial, failed}, fork: option<{from_turn: uuid, reason: string}}}` | runtime | Marks the **legal cut point** for compaction and fork. A `turn_start` with `boundary=user_message` starts a new turn; `boundary=resume` continues a Suspended turn; `boundary=fork` branches from another turn (with `fork.from_turn` set). The `compression` field records whether the round paused with a complete summary or a partial one — useful for `Resume`. |
| `model.usage` | `{tokens: Tokens, model_reported: bool, model_id: string}` | runtime / model | Inline. Records token cost per `model.invoke` call. Used by `state.tokens_used` accounting. `model_reported=true` means the counts are the provider's billed values; `false` means the client estimated them. |

```python
class Tokens:
    in:          u32   # input tokens
    out:         u32   # output tokens
    cache_read:  u32   # input tokens served from cache
    cache_write: u32   # input tokens written to cache
    total:       u32   # in + out (computed or summed)
```

`model_reported` matters for cost accuracy — providers bill
on their reported numbers, clients estimate from tokenisers
that may diverge by a few percent. `state.tokens_used` should
prefer `model_reported=true` when both sources are available.

#### Load classes (unchanged)

Kinds split into three load classes (see `loop.md` for
details):

- **Always-loaded** (`goal`, `system`): `pinned=1`,
  `surface_visible=1`, never summarised.
- **Body** (`text.*`, `tool.*`, `governance.*`, `compaction.*`,
  `model.usage`): subject to compression + eviction;
  `surface_visible` flips to `0` as the entry ages out.
- **Meta** (`session.turn_start`): boundary markers,
  pinned, always present, but invisible to the model. `meta`
  table captures session-level tags instead.

### Why content-addressed blobs

A typical agent turn stores **a few large bodies** — file
diffs, test logs, command outputs — and **many small
bodies** — model messages, intents, verdicts. Putting the
large bodies in SQLite BLOB columns makes the db large,
slow to back up, and the duplicate-friendly (the model may
read the same file twice). Externalising them as
**content-addressed files** gives:

- **Dedup for free** — `sha256("hello")` is one file on disk
  regardless of how many `entry` rows reference it.
- **Portability** — a session is one directory; `tar` it,
  copy it, restore it without DB-level tooling.
- **Streaming reads** — a future Tauri / mobile client can
  serve the blob file directly without round-tripping
  through the runtime process.
- **GC-able** — `refcount` on the `blob` table tells us
  when a blob is no longer referenced and can be unlinked.

The db row carries `body_hash`; the body itself lives at
`blobs/<hash>`. The choice between `body_inline` and
`body_hash` is per-row, based on a byte threshold.

### The `surface_visible` and `pinned` columns

Two columns on the main table capture the policy side of
derive:

- **`surface_visible`**: `1` by default. The loop sets it to
  `0` when an entry has aged out of the surface (tier 2
  one-liner still uses `surface_visible=1`; only **evicted**
  entries flip to `0`). Future derives can use it as a
  pre-computed hint instead of recomputing the visibility
  from scratch.
- **`pinned`**: `1` means "never compress, never evict, always
  on the surface". Set at entry creation for `goal`,
  `system`, and any user-marked intent. The mechanism is
  cheap: `derive` filters `pinned = 1` as always-visible;
  no policy code is needed for it.

These columns move what was loop.md policy into a queryable
schema. The loop's `Surface` cache and `derive` algorithm
remain the authoritative view — `surface_visible` is a hint,
not a source of truth.

### API Surface

| Method | Returns | Notes |
|--------|---------|-------|
| `append(entry)` | the new `seq` | The only write op. Monotonic per session. |
| `since(seq)` | list[Entry] | `seq > seq`, ordered ascending. Used by `derive`. |
| `latest()` | Entry | Highest-`seq` row in the session. |
| `latest_kind(kind)` | Entry or null | Highest-`seq` row of the given kind. |
| `pair_of(seq)` | Entry | The paired entry (`pair_ref` → …). |
| `surface_entries()` | list[Entry] | Rows where `surface_visible=1`, ordered. |
| `pinned_entries()` | list[Entry] | Rows where `pinned=1`. Always-loaded. |
| `covers(start, end)` | list[Entry] | Rows with `covers_start <= X <= covers_end` for any X — used to find the summary that replaced a given seq. |
| `blob_path(hash)` | Path | Resolves `body_hash` → file on disk. |
| `blob_get(hash)` | bytes | Reads the blob file. Lazy, cached. |
| `blob_register(content)` | hash | Adds the content if absent; returns the hash. |

The pure form of every method operates on an in-memory
snapshot (for tests); the impure form reads SQLite + the
filesystem.

### Multi-session ledger

A process can have **multiple sessions** concurrently
(multi-agent runs, fork, distributed). Each session is its
own directory under `<root>`; the loop multiplexes them.

`derive` is per-session: each session's surface is computed
from its own entries + blobs. Cross-session reads are explicit
(e.g. "find the most recent `goal` in any session") and not
on the hot path.

## Consequences

### Positive

- **Recoverable by construction.** `Resume` re-attaches to
  the session directory; the loop re-derives the surface from
  the SQLite + blobs. Nothing in memory is required to
  survive a crash.
- **Auditable.** Every decision — model intent, governance
  verdict, executor effect, approval — is a row in `entry`
  with provenance. Reconstruct any past iteration from a
  single directory.
- **Portable.** A session is one directory. Backup = `tar`.
  Sync = `rsync`. Inspect = `sqlite3 ledger.db` + `cat blobs/<hash>`.
- **Cross-language.** SQLite + content-addressed files are
  language-neutral. Rust and Kotlin both use the same schema
  and the same blob hash; the conformance suite asserts
  semantic equivalence.

#### Index plan (candidates)

Indexes that the loop's expected query patterns need.
**This is a draft plan; the actual index set lands with the
slice and gets re-validated against `EXPLAIN` output.** Each
index is named after the query it serves, not after the
column.

| Index | Definition | Query it serves | Notes |
|-------|------------|------------------|-------|
| `entry_kind_seq` | `(kind, seq DESC)` | `latest_kind(kind)`, `rewrite_directive_since(seq)` | The hot path: `derive` filters by kind then walks seq descending to find the latest directive. **DESC matters**: SQLite uses the index in reverse for `seq > ?` queries when sorted DESC. |
| `entry_turn_seq` | `(turn, seq)` | Per-turn seq range: `since(seq)` within a turn, surface entries within a turn. | The most common scan after `entry_kind_seq`. |
| `entry_turn_step` | `(turn, step)` | Per-turn step lookup; boundary validation against `session.turn_start.body.turn.id`. | Explicit `step` is the iteration index the loop cares about; seq is monotonic *within* a turn, but step is the user-facing semantic. |
| `entry_turn_kind` | `(turn, kind, seq)` | Per-turn kind filter (e.g. "all `compaction.rewrite` in turn X"). | Could be folded into `entry_kind_seq` + a per-turn filter; kept separate because the per-turn shape dominates. |
| `entry_pair_ref` | `(pair_ref)` | Pair-completeness check (`pair_ref IS NOT NULL` rows, dangling `pair_ref`s). | Required for the integrity check that `tool.call ↔ tool.result`, `governance.approval_request ↔ approval_response`, etc. are paired. |
| `entry_pinned_seq` | `(seq) WHERE pinned = 1` | `pinned_entries()` — fetch all pinned entries for the surface. **Partial index** — only the pinned subset is indexed; cheaper to maintain. | When the pinned set is large, falls back to a full scan with `surface_visible = 1`. |
| `summary_covers` | `(turn, covers_start, covers_end) WHERE kind LIKE 'summary.%'` | `covers(start, end)` — find the summary that replaced a given seq. **Partial index** because only `compaction.summary` rows have covers. | Hot path during `compaction.rewrite` resolution. |
| `blob_size` | `(size)` | GC sweeps that scan large blobs first. | Only useful if the archive GC sorts by size to free the most bytes quickly. |
| `blob_wall_ts` | `(wall_ts)` | Age-based archive sweeps (oldest-first). | Hot path for the periodic ops_log op. |
| `ops_log_started` | `(started_at)` | "What ops ran recently?" — dashboard / status queries. | Rare path; cheap to drop if not needed. |
| `ops_log_op_started` | `(op, started_at)` | "What's the history of dedup_gc runs?" — per-op audit. | Same as above. |

**Indexes intentionally NOT created:**

- `(seq)` alone — `seq` is the primary key; the PK index
  already covers seq-only lookups.
- `(provenance)` — provenance is diagnostic; full-scan is
  acceptable for the rare "who produced this?" queries.
- `(body_hash)` — pointer lookups go through `entry.body_hash`,
  not the blob table; the join cost is dominated by the entry
  scan, not the blob lookup.

**Index tuning note:** every index above is a candidate.
Empirical `EXPLAIN QUERY PLAN` against a realistic workload
  determines which survive. A rule of thumb: if the query
  never appears in the loop's hot path, the index is cargo-
  culted. Add only what `derive` / `assembly` / `summarize`
  actually call.

The conformance suite (`just conformance`) asserts schema
**shape** (the index list above) but not the actual `EXPLAIN`
plans — those are runtime concerns, land with the slice.

### Write Discipline

The ledger is durable, but durability is enforced by a
specific recipe of SQLite pragmas + application invariants +
flush discipline. Three rules hold:

#### 1. PRAGMAs (set once at session creation)

```
PRAGMA journal_mode = WAL;        -- readers don't block writers
PRAGMA synchronous  = NORMAL;    -- WAL-safe: fsync on commit,
                                 -- not per-write; ~100x faster
                                 -- than FULL with no safety loss
PRAGMA foreign_keys = ON;        -- enforce pair_ref integrity
```

- **`journal_mode = WAL`** lets readers continue while a
  writer holds the WAL. The hot path of `derive` (lots of
  reads) is never blocked by an in-flight `append`.
- **`synchronous = NORMAL`** is the WAL-safe setting — the WAL
  is fsynced at commit time (not on every write), and the
  main db file is fsynced periodically by the WAL writer.
  `FULL` would fsync every write, ~100× slower, with no
  durability gain under WAL.
- **`foreign_keys = ON`** turns SQLite's FK enforcement on
  so a `tool.result` row can't reference a non-existent
  `tool.call`. The FK on `entry.body_hash → blob.hash` and
  `entry.pair_ref → entry.seq` is what makes pair_ref
  integrity real, not aspirational.

These three are **set once per session** in `PRAGMA journal_mode = WAL;` order — they
survive across process restarts (the journal mode is
persisted in the db header; the others can be re-applied on
open if needed).

#### 2. App-level append-only invariant

The application **never** issues `UPDATE` or `DELETE`
against `entry`, `blob`, `dedup`, `ledger_meta`, or
`ops_log` rows. The only writer is `INSERT` (and `BEGIN`/
`COMMIT` to group inserts into transactions).

The sole exception is **dedup GC**, which deletes `dedup`
rows older than a TTL (default ~7 days). GC is itself
recorded in `ops_log`; the GC transaction is the only
allowed `DELETE` statement.

This invariant is what lets `Resume` work — the in-flight
turn's data is never partially mutated. If a row is in the
db, it's because the transaction that wrote it committed.

#### 3. Transaction boundaries — batched writes

A round trip that produces N entries writes them in a
**single transaction**, not one transaction per entry:

```
def append_batch(entries: list[Entry],
                 dedups: list[DedupRow]):
    with conn:                                     # BEGIN
        for e in entries:
            cursor.execute("INSERT INTO entry (...)", ...)
        for d in dedups:
            cursor.execute("INSERT OR IGNORE INTO dedup (...)", ...)
        conn.commit()                              # fsync WAL
```

This is **atomic** at the SQLite level — either the whole
batch lands or none of it does. The loop's three-pass
`assemble` writes its outputs as one batch per iteration.
The mid-iteration `summarize(prefix)` writes the
`compaction.summary` + the trailing `compaction.rewrite`
as one batch (so the rewrite always points at a committed
summary — no dangling pointers).

Batch size defaults to "the whole iteration" (~3-15 entries).
Very long rounds can split into smaller batches if a
transaction takes too long, but the unit is always "logical
group of entries".

#### 4. Flush triggers

`PRAGMA wal_checkpoint(TRUNCATE)` is the explicit
checkpoint op; commits are implicit. Three triggers:

| Trigger | Op | Why |
|---------|-----|-----|
| **Turn boundary** (`session.turn_start` row written) | `BEGIN; … write … COMMIT; wal_checkpoint(TRUNCATE)` | A turn boundary is a recovery point — the runtime may die right after. The WAL must be empty so a fresh process can open the db without a recovery replay. |
| **Suspended** (`governance.approval_request` row written) | same | A round may pause for hours. Before pausing, the WAL is empty; the resume process starts from a clean checkpoint. |
| **Process shutdown** | same | Graceful exit. Ungraceful exit is fine — SQLite's WAL recovery handles it on next open. |

Between triggers, the WAL grows as the loop appends. The
runtime does **not** checkpoint per-iteration — that would
defeat the speed gain of WAL. The implicit checkpoint per
`COMMIT` keeps the WAL bounded in the common case.

`wal_checkpoint` is a no-op data-wise — it moves WAL pages
back into the main db file and truncates the WAL. After
checkpoint, the main db is self-contained and the WAL file
is empty.

#### 5. Backup — directory is git-managed

A session is one directory; **that directory is the unit of
backup**. The simplest production backup for a single-
machine or small-team deployment is `git`:

- `cd <root> && git add . && git commit -m "session <id> @ seq N"`
- push to a remote on every commit (or every Nth commit).

This gets you:
- **Incremental backups**: git deltas are cheap.
- **Off-site redundancy**: push to a remote.
- **Audit trail**: commit messages + branch history.
- **Free dedup at the file level**: identical blob files
  between two sessions are deduplicated by git's content-
  addressable object store.

For larger teams or multi-machine deployment, the same
directory layout works with `git-annex`, `restic`, S3
mounts, or any object store. The **on-disk format is
portable**; the backup tooling is replaceable.

The `backup_watermark` row in `ledger_meta` tracks "what's
been shipped to the *next* tier" — git's commit log is a
secondary record, but the db row is what an in-process
check uses to decide "does this entry still need backing
up?".
- **Lossless projection.** Surface is derived; ledger never
  forgets. `body_hash` re-resolves any dropped content
  without replaying the loop.
- **Dedup for free.** Repeated reads of the same file or
  the same large model output is one blob on disk, many
  entry rows.

### Costs

- **Two storage systems per session.** SQLite + a filesystem
  blob store. They have to be kept consistent — a successful
  `append` that wrote the row but failed to write the blob
  file leaves a dangling reference. **Two-phase write**:
  blob first (idempotent — content-addressed), then row
  (with the hash).
- **GC.** `blob.refcount` lets us garbage-collect blobs that
  are no longer referenced. Doing GC wrong = data loss;
  doing it safely = refcount walks + atomic unlink. Future
  work; the agent loop never deletes from the ledger
  eagerly.
- **Schema migration.** Adding a kind = a new typed payload.
  Adding a *column* (e.g. a new always-loaded flag) = schema
  version bump + a careful migration. The `schema_var`
  column lets readers tolerate old rows without breaking;
  writers carry the schema version they assume.
- **Single writer per session.** Two `agent_loop`s writing
  the same session directory would race on SQLite WAL and
  on the blob store. Today's design: one writer per session.
  Multi-writer is a future problem (see Open Questions).

## Open Questions

1. **Multi-writer per session.** When does this matter
   (fork, multi-agent coordination, distributed runtime)?
   And what's the consensus model — Raft, CRDT, leader-
   follower? For now: one writer per session is a hard
   invariant.
2. **Blob GC.** When does a `refcount==0` blob get unlinked?
   On session close? On a background sweep? On demand?
3. **Inline vs externalised body threshold.** Right now the
   threshold is a single config knob. Future: per-kind
   thresholds (e.g. `tool.result` bodies externalise at 1 KiB,
   `assistant.text` at 16 KiB).
4. **Body-level dedup vs entry-level.** The `dedup` table is
   value-level (across entries). Useful for analytics; less
   useful for the loop itself. Is it worth the write
   overhead?
5. **`surface_visible` drift.** The column is a hint; the
   loop's `Surface` cache is authoritative. A buggy `append`
   that sets `surface_visible=0` on a fresh entry should be
   impossible — should we make it impossible (e.g. a CHECK
   constraint)?
6. **Schema versioning migration path.** When `schema_var`
   bumps, do old rows get rewritten, or do readers carry
   per-version parsers? Rewriting is offline + slow;
   per-version parsers are online + complex.
7. **Concurrent reads from a long-running attach** (e.g. a
   debug tool attaching to a live session). WAL mode
   handles this; the open question is whether the surface
   cache (`loop.md`) should be shared with the attach tool
   or rebuilt independently.

## See also

- `loop.md` — the driver / turn / iteration loop; the
  ledger is its single source of truth.
- `AGENTS.md` — repo-wide rules.
- `.agents/multi-language.md` — Rust + Kotlin mirror; the
  ledger is one of the shared surfaces.
- `.agents/testing.md` — conformance + property tests are
  how we lock the ledger's invariants.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: **draft (possible mechanism)** — session-as-
  directory + content-addressed blobs + 4-table SQLite are
  candidates; specific pragmas, index DDL, hash function, and
  inline-vs-externalised threshold land with the slice. No
  final code.