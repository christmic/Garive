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

## Mechanism Map — eight problems, one ledger

The ledger is **append-only**. Every "fix a problem" the
runtime has is therefore a **masking family** — append a row
that hides or transforms part of the surface, never delete.

| # | Problem | Mechanism | Where it lives | Key invariant |
|---|---------|-----------|----------------|----------------|
| 1 | **Window overflow** — long round doesn't fit the model's context | `compaction.rewrite` + `compaction.summary`: mask the **prefix**, replace with a structured summary; tail stays full-fat. | `compaction.*` family + `loop.md` "Summary Entry Schema" | Compression is **append-only**; original entries stay in the ledger forever. |
| 2 | **Information density** — even after compression, the model wastes attention on stale content | `derive` rules + 3-pass `assemble` (tier / evict / format) + per-tool policy profiles | `loop.md` "Surface Optimisation Passes" + "Per-tool Policy Profiles" | Tier decisions are **sticky** (one-way within a session); **no re-promotion**. |
| 3 | **User goes back** — "actually, undo that" | `session.undo` (mask **suffix** after a `turn_start` target) + `session.redo` (re-extend) | `session.*` family + "Undo / Redo (a third masking family)" | **Cost does not roll back** — `model.usage` rows are preserved; only the *context* rewinds. |
| 4 | **Agent tries multiple strategies, picks the best** | `branch.*` family + `branch_path` column: each attempt gets a `branch_path`; `branch.verdict{adopt\|discard}` resolves. Path-style nesting (`A.alt.deep`) for trees. | `branch.*` family + "Branches (in-session lightweight fork)" | Default `PROMPT_FOR_MODEL` projection is **mainline-only**; discarded branches are kept in the ledger for `dream` extraction but hidden from the surface. |
| 5 | **Long-task goal** — agent must not lose the thread across hours | `goal.declare` / `goal.update` / `goal.close` rows; `pinned=1`; **current goal = derived**, not stored | `goal.*` family + "current goal derived, not stored" | No "current_goal" field anywhere; `derive` walks the `goal.*` timeline each call. |
| 6 | **Right to erasure** — user / regulator wants data gone | `privacy.redact` (mask a range or a single `uid`); blob bytes **physically unlinked**; the row stays for audit | `privacy.*` family + "Right to Erasure" | `redactable` is a registry flag — some kinds (`compaction.rewrite` itself, `privacy.redact`) **never** redact. The audit trail survives; the bytes do not. |
| 7 | **Recovery** — crash, kill, or `AskUser` Suspend/Resume | `pair_ref` (in-library) + `state.phase` + `derive_position` on Resume; `uid` keeps the in-flight turn uniquely identified | `entry.pair_ref` + `session.turn_start.boundary=resume` + "The Turn State" | Pair completeness invariant — every `tool.call` has a `tool.result`, every `approval_request` has a `approval_response`. Resume is **mid-pair safe** (it back-fills unpaired calls). |
| 8 | **Cross-ledger address** — parent refs child, fork refs source, dream refs origin, audit refs any | `uid` (session-scoped global id) + `ref` (`{session, uid}` JSON) on `entry`; `ledger_meta.lineage` for fork | `entry.uid` + `entry.ref` + "Two reference paths" + "lineage" | `ref` is **decoupled from `seq`** — never breaks across re-numbering, archive, or compaction. |
| 9 | **Idempotency, backup, compat** — retries, durable storage, schema evolution | `dedup` (`client_generation` PRIMARY KEY) + `backup_watermark` + WAL/sync=NORMAL + `kind_registry` (write-strict read-lenient) + `kind_migration` (forward-only) | `dedup` table + `ops_log` + "Write Discipline" + "Kind Compatibility Principle" | Old rows are **always readable**, even by readers that don't know the kind; the registry is **append-only**; the ledger is the source of truth, **everything else is derived**. |

**The shape of the table matters.** Every row is
**append-only**. Every problem is solved by **adding a row
that masks / supersedes / annotates an earlier row**. There
is no "delete" primitive; the cost of physical deletion
(of an unlinked blob, of an unreferenced dedup row) is the
only place where bytes are physically removed, and it is
**gated by explicit ops** (`blob_redact_unlink`, dedup GC)
that themselves leave `ops_log` rows for audit.

**Cost of these mechanisms.** Adding rows is cheap;
reading them is cheap (the `entry` table is indexed by
`seq`, `kind`, `turn`, `pinned`, `pair_ref`); the
projection cost (`derive` + `assemble`) is bounded by the
size of the surface, not the size of the ledger. The
mechanisms are layered: each one solves one problem, and
the problems don't step on each other. `branch.*` works
inside a `compaction.rewrite`; `session.undo` works across
a `branch.verdict`; `privacy.redact` works on any entry.
The mechanism that says "we never lose data" is the
**immutability** of the ledger; every other mechanism is a
**projection** over it.

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
| `seq` | INTEGER PRIMARY KEY | Monotonic per session. Strictly increasing. The **physical** ordering key — `derive` reads via `seq`. |
| `uid` | TEXT NOT NULL UNIQUE | **Global entry identity** within the session. Stable across the entry's lifetime. Generated once at append time (UUID or sortable id). |
| `turn` | TEXT | `turn_id` (UUID). The agent_turn this entry belongs to. |
| `step` | INTEGER | Iteration index within the turn. `0` for user entry, `n` for the n-th `iteration`. |
| `branch_id` | TEXT NULL | **Lightweight in-session branch path** this entry belongs to. `NULL` = mainline (the default). Non-null = an in-session fork attempt. **Path-style**: a nested branch uses dotted form (`A.B.C`); the segment before the first `.` is the top-level branch, the segment after is the parent. See `branch.*` family below. |
| `kind` | TEXT NOT NULL | Dotted kind name (e.g. `assistant.text`, `tool.call`, `tool.result`, `compaction.summary`, `compaction.rewrite`). |
| `provenance` | TEXT | Opaque source tag: which model, which tool, which rule produced this entry. |
| `surface_visible` | INTEGER (bool) | Whether this entry is *currently* visible on the surface. Defaults to `1`. Gets `0` when the entry has aged out / been compressed / been evicted. |
| `pinned` | INTEGER (bool) | `1` if the entry is **pinned** — never compressed, never evicted, always on the surface. `goal` and `system` are pinned by default; the model can request pin via intent; `governance.judge` can mark entries as pin-worthy. |
| `pair_ref` | INTEGER NULL | **In-library** reference: `seq` of the paired entry in the **same session**. `tool.call ↔ tool.result`, `governance.approval_request ↔ governance.approval_response`. `NULL` if unpaired or cross-library. |
| `ref` | TEXT NULL | **Cross-library** reference: JSON object `{session: <uuid>, uid: <uid>}` pointing at another session. See "Two reference paths" below. `NULL` if no cross-library reference. |
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

#### Two reference paths: `pair_ref` vs `ref`

Every entry carries **zero, one, or both** reference columns.
They have different semantics and are **not** interchangeable.

**`pair_ref` (in-library)** — integer `seq` of the paired entry
in the **same session**.

- UseUse for: `tool.call ↔ tool.result`,
  `governance.approval_request ↔ governance.approval_response`,
  any same-session dual.
- Why: cheap to follow (`SELECT … WHERE seq = ?`), survives
  a session rename (it's a session-local pointer).
- Foreign key: `pair_ref REFERENCES entry(seq)` enforces
  "every pair points at a real entry in this db".

**`ref` (cross-library)** — JSON-encoded object pointing at
**another session**:

```json
{"session": "uuid-of-other-session", "uid": "uid-in-that-session"}
```

- UseUse for: **sub-agent completion messages** (parent session
  receives a message from a child session it spawned),
  **memory attribution** (long-term memory notes which
  session / entry first observed the fact), **fork lineage**
  (`session.turn_start` carries `fork.from_session.uid`).
- Why: cross-session references must address by both **where**
  (session id) and **what** (entry id); `uid` alone is not
  unique across sessions.
- The runtime resolves `ref` by **fetching the target
  session's db** and looking up `entry WHERE uid = ?`. This
  is a `cross_session_fetch` op, not a within-session query.
- There is **no** foreign key — the target session may not
  exist yet (future message) or may have been archived. The
  reader follows `ref` and handles the not-found gracefully.

A row can have **both** `pair_ref` and `ref` set — e.g. a
`tool.call` that references a session-shared tool (pair to
local `tool.result`, ref to the upstream tool registry).
Most rows set neither; some set one; few set both.

#### `uid` is what makes `ref` decoupled

`uid` is the **stable identity** of an entry across its
lifetime. It is generated **once** at append time (UUID v4
or a sortable id like ULID). It does **not** change when:

- The session is renamed.
- The session is forked (the fork's entries get fresh `seq`,
  but `uid` carries the lineage by being copied into the
  fork's `ledger_meta.lineage`).
- The session is archived / restored from cold storage.
- The entry is compacted / superseded (the new entry gets a
  fresh `uid`; the old `uid` lives on as the
  `superseded_by` chain's anchor).

Critically: **`uid` lets a parent ledger reference a child
ledger without depending on the child's `seq`.** The parent
writes `{"session": "<child-uuid>", "uid": "<child-entry-uid>"}`
into `ref` and that reference is stable forever — even if
the child's `seq` gets renumbered, compacted, or the child
session is archived. This is what unblocks:

- **Sub-agent completion messages**: parent waits for child;
  child writes a completion entry; parent reads `ref` to find
  it without caring about the child's internal `seq` numbering.
- **Fork lineage**: a forked session records `parent_session +
  boundary_uid` (instead of `parent_seq`), and the reference is
  stable across any compaction of the parent's earlier entries.
- **Audit / memory attribution**: long-term memory notes
  "first observed by `ref → {session, uid}`" — the note
  survives any re-numbering of the source session.

`uid` is what makes cross-ledger references **portable** rather
than brittle.

#### `blob` — content-addressed registry (no refcount)

A blob lives at `<session>/blobs/<sha256>` on disk and is
referenced by hash from `entry.body_hash`. The `blob` table
is the **single index** that maps hashes to file paths and
records metadata — **and nothing more**. There is no
reference count.

| Column | Type | What it holds |
|--------|------|---------------|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | Auto-assigned rowid. Internal handle. |
| `hash` | TEXT NOT NULL UNIQUE | `sha256:<hex>` — the content hash. The row's *business identity*; also the on-disk filename. |
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
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | Auto-assigned rowid. Internal handle. |
| `client_generation` | TEXT NOT NULL UNIQUE | Idempotency token supplied by the client (UUID, monotonic counter, or whatever the client uses). Same token across retries = same write. |
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
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | Auto-assigned rowid. Internal handle. |
| `key` | TEXT NOT NULL UNIQUE | A documented tag name (see below). |
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

### Entry Kinds — ten categories, one namespace

Kinds are **dotted, lower-case, category-prefixed**. The
prefix is the category; the suffix names the specific event.
Adding a new kind is **a row in the catalog** plus a typed
payload — nothing else changes.

Body schemas below are normative: the wire format in
`spec/proto/` must match these shapes. Inline bodies carry
the value directly in `entry.body_inline`; large bodies
reference a blob hash in `entry.body_hash`.

**Ten categories** (the namespace is closed; new categories
require an update to this list):

| # | Category | Kinds | What it covers |
|---|----------|-------|----------------|
| 1 | **user/model** | `text.user`, `text.assistant` | The free-form text the user and the model exchange. |
| 2 | **tool** | `tool.call`, `tool.result`, `tool.result.rejected` | The model's verb (call) and the executor's response (result). Pairs via `pair_ref`. |
| 3 | **governance** | `governance.verdict`, `governance.approval_request`, `governance.approval_response` | The gate between intent and effect. AskUser ↔ ApprovalResponse is a complete pair. |
| 4 | **compaction** | `compaction.rewrite`, `compaction.summary` | Window-overflow machinery. Masks the prefix; the new prefix starts at the rewrite point. |
| 5 | **session** | `session.turn_start`, `session.undo`, `session.redo` | Turn boundaries, human rewind, redo. |
| 6 | **branch** | `branch.open`, `branch.verdict` | In-session lightweight fork. Path-style nesting (`A.B.C`); the verdict picks the winner. |
| 7 | **goal** | `goal.declare`, `goal.update`, `goal.close` | The long-task frame. Current goal = derived from the entry stream. |
| 8 | **privacy** | `privacy.redact` | Right-to-erasure masking. The blob may be physically unlinked; the row stays for audit. |
| 9 | **injection** | `harness.feature`, `system` | The harness-supplied context: env snapshots, skills catalog, `AGENTS.md`, and the framework-pinned system prompt. |
| 10 | **telemetry** | `model.usage` | Per-call observability: token counts (`in / out / cache_read / cache_write / total`), `model_reported` flag, `model_id`. |

The categories are **ordered by where they sit in the
agent's mental loop** (which is also the order `derive`
applies its masking + projection pipeline — see
`loop.md` "Derive pipeline"):

1. **session** (`session.undo`) — first, because undo masks
   the largest possible range
2. **branch** (`branch.verdict`) — second, because
   discarded branches get masked wholesale
3. **compaction** (`compaction.rewrite`) — third, masks the
   prefix
4. **privacy** (`privacy.redact`) — fourth, masks individual
   ranges / `uid`s
5. **clipping rules** (per-tier / age / volume) — fifth,
   shrink the surviving entries
6. **kinds filter + pinned** — sixth, project the visible
   categories to the surface

Categories **1–4** are the **masking family** (cover a
range, hide content from the surface). Category **5** is the
**shrinking family** (shrink content within a still-visible
range). Category **6** is the **projection step** (which
categories go on the surface at all). Categories **7–10**
are **content categories**: where the entries come from, not
how they are projected.

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
| `session.undo` | `{target: seq, reason: string}` | runtime / user | **Masks the suffix after `target`** — the surface is "rolled back" to `target`. `target` must point at a `session.turn_start` entry (legal cut point). Model sees only entries up to and including `target`'s iteration; the next loop iteration runs **from `target` forward**. The undo itself is a row in the ledger — never deleted. |
| `session.redo` | `{target: seq, ref: {session, uid} \| null, reason: string}` | runtime / user | **Cancels a prior `session.undo`** by re-extending the visible range. The most recent `session.undo` whose `target` ≤ `target` of this `redo` is conceptually undone; future derives again include the previously-masked suffix. `ref` may point at the prior `session.undo` entry being reversed. Like undo, redo is itself a row in the ledger. |
| `branch.open` | `{branch_id: string, from_seq: seq, purpose: string}` | runtime / model | Opens a **lightweight in-session branch** starting at `from_seq`. All entries appended between this row and the matching `branch.verdict` carry `branch_id = branch_id`. **Nested branches** are allowed: a `branch.open` whose `branch_id` contains a `.` (e.g. `A.B`) is a sub-branch of `A`; its `from_seq` must be ≥ the parent branch's `from_seq` and ≤ the parent's `branch.verdict.seq`. Multiple `branch.open` rows from the same `from_seq` are parallel attempts at that level; each carries its own `branch_id`. |
| `branch.verdict` | `{branch_id: string, decision: enum{adopt, discard}, reason: string, ref: {session, uid} \| null}` | runtime / model / governance | Resolves a branch. `adopt` makes the branch's entries part of the **mainline** (`branch_id` is preserved on the rows for audit, but the surface projection includes them). `discard` retires the branch's entries from the surface (still in the ledger for audit). The verdict is itself a row in the ledger — auditable, like `governance.verdict`. **A verdict's `branch_id` matches a single `branch.open`'s `branch_id`** (the path). A nested branch's verdict operates at that level only; it does not auto-adopt / auto-discard the parent. |
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

#### `goal.*` — current goal derived, not stored

`goal` is **not** a variable that the runtime mutates; it is
**derived** from the entry stream at every `derive()` call.
The current goal = the most recent unclosed `goal.*` entry.
That single rule lets `goal` survive Suspend / Resume,
fork, archive, and any ref rewrite, with no extra state.

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `goal.declare` | `{text: string, subgoals?: list<Subgoal>, 来源?:: enum{user, derived, agent}}` | runtime / user | Creates a new goal. `pinned=1` by default. `text` is the human-readable goal; `subgoals` is an optional hierarchical decomposition. |
| `goal.update` | `{text?: string, subgoals?: list<Subgoal>, 来源?: enum}` | runtime | Refines an existing goal. `ref = {session, uid}` points at the prior goal entry being updated (the prior entry is *not* superseded — it stays in the ledger as audit; the update is a *new* entry that supersedes semantically). |
| `goal.close` | `{status: enum{完成, 放弃}, 结论?: string}` | runtime / model | Closes the open goal. `ref` points at the goal entry being closed. After this entry, no goal is open until a new `goal.declare` arrives. |

**Current goal = derived.** No table, no field — the
runtime computes it from the entry stream:

```
def current_goal(entries):
    # walk entries in seq-descending order; the first goal.*
    # that is not closed is the current one.
    open_goal = None
    for e in entries_desc(entries):
        if e.kind == 'goal.declare' or e.kind == 'goal.update':
            open_goal = e
        elif e.kind == 'goal.close' and open_goal and e.ref.targets(open_goal):
            open_goal = None
    return open_goal   # None if no open goal
```

**Why derive instead of store:**

- **Resume** — after a Suspend, the loop's first `derive` finds
  the open goal; no separate state to load.
- **Fork** — child session inherits parent's open goal
  automatically (parent goal entries are in the ledger before
  the fork's `boundary_seq`).
- **Archive / restore** — current goal survives any compaction
  because `goal.*` entries are themselves entries; archive
  keeps the entry list, restore re-derives.
- **Cross-session** — `goal.declare` can carry `ref = {session,
  uid}` pointing at a parent goal, so a sub-agent can adopt a
  parent's goal.
- **Personal layer** — long-lived personal goals (user-level,
  across sessions) live in a special session with `mode =
  'personal'` (per `ledger_meta.mode`). Other sessions
  cross-reference personal goals via `ref`. The session-as-
  directory shape carries it.

`goal.*` kinds are **always-loaded** (they go on the surface
whenever a goal is open) and **pinned** (never compressed,
never evicted — they are the model's frame of reference).

#### `privacy.*` — Right to Erasure (redaction is to deletion as compaction is to summarisation)

The ledger is durable and append-only; **the only way to
"delete" data is to mark it deleted in the ledger and let the
runtime render the redaction**. The row itself is never
removed; the blob bytes it points at may be physically
unlinked.

| Kind | Body | Producer | Notes |
|------|------|----------|-------|
| `privacy.redact` | `{covers: {from_seq: int, to_seq: int} \| {uid: string}, reason: string}` | runtime | Marks a range (or single entry by `uid`) as redacted. `derive` projects entries inside the range as `[已删除] <kind> @<seq>` placeholders; **the entry row is preserved**, the body content (if any) is **never** returned. The blob may be physically unlinked. |

Two granularity levels:

- **Session-level delete** — `rm -rf <session>/`. The whole
  directory goes. This is the strong form: no audit, no
  recovery, no replay. Reserved for the user's explicit
  request to forget the session entirely.
- **Entry-level redaction** — `privacy.redact`. **Isomorphic
  to `compaction.rewrite`**: it adds an entry to the ledger
  that covers a range; the covered entries' bodies are
  hidden from `derive`; the row itself stays. **Auditable**:
  the redaction entry records what was covered, when, and
  why — so the deletion itself is part of the ledger.

**`derive` behaviour with redaction:**

```python
def project_entry(e, redactions):
    for r in redactions:
        if r.covers.contains(e.seq):
            return RedactedPlaceholder(
                kind   = e.kind,
                seq    = e.seq,
                by_seq = r.seq,
                when   = r.wall_ts,
                reason = r.reason,
            )
    return e    # unredacted — surface the body
```

The placeholder preserves the entry's identity (`seq`,
`kind`, `wall_ts`, `provenance`, `pinned`, `pair_ref` /
`ref`) but not its body. The model sees something like:

```
TOOL_RESULT @seq 42 (redacted by privacy.redact@seq 47 on
 2026-08-29, reason="user-requested")
```

— enough to know a tool was called, not enough to see its
output.

**What stays, what goes, after redaction:**

| Field | Stays | Why |
|-------|-------|-----|
| `seq`, `uid`, `kind`, `turn`, `step` | yes | what happened |
| `provenance`, `wall_ts`, `pair_ref`, `ref` | yes | chain-of-custody for the redaction itself |
| `body_inline` / `body_hash` content | **no** | the whole point of the redaction |
| The row itself | yes | the redaction is itself an entry in the ledger |

**The blob is the only thing physically removable.** When no
non-redacted entry references a blob, the blob's bytes are
unlinked (recorded in `ops_log` as `blob_redact_unlink`). The
**blob hash stays in the row's `body_hash`** — so any future
audit can still see "this entry pointed at a blob that was
unlinked on date X". The body is gone; the pointer is not.

#### Undo / Redo (a third masking family)

The ledger is immutable; you cannot delete a row. Undo and
redo are therefore **third masking instructions** alongside
`compaction.rewrite` (masks a prefix) and `privacy.redact`
(masks a range or a single `uid`). The pattern is the same:
**append a row that covers a range; the projection hides
the covered content; the row itself stays forever.**

| Instruction | Direction | What it masks | Effect on the surface |
|-------------|-----------|----------------|----------------------|
| `compaction.rewrite` | masks **prefix** | `covers: {from, to}` | the prefix is replaced by `compaction.summary`; model sees the summary plus the post-cut tail. |
| `privacy.redact` | masks a **range** | `covers: {from, to}` or `{uid}` | rows in the range render as `[已删除]` placeholders. Blob may be physically unlinked. |
| `session.undo` | masks **suffix after `target`** | `target: seq` | model sees only entries `seq <= target`'s turn; the suffix beyond is "retired" for the duration of the undo. |
| `session.redo` | reverses a `session.undo` | `target: seq`, `ref: {session, uid}` of the prior undo | the masked suffix is visible again. |

**Rules** (these are the *only* rules; everything else
follows from the general masking family):

1. **`target` is a turn boundary.** `session.undo.target`
   must equal a `session.turn_start.seq`. The projection
   walks entries after `target` and marks them retired; the
   next `derive` recomputes with that retirement in effect.
   The loop resumes from `target`'s turn forward — the user
   re-runs the round as if it had paused at that boundary.
2. **Complete pairs required.** Every entry in the retired
   suffix must be part of a *complete* pair (its
   `pair_ref` resolved, no dangling `intent` without
   `tool_result`, no `approval_request` without
   `approval_response`). Undo is **rejected** with an error
   if the suffix is mid-pair. This is the same boundary
   invariant as `compaction.rewrite`'s clean-cover rule
   (`loop.md`).
3. **The undo is itself a row.** The conversation about the
   undo — what was undone, when, why — lives in the ledger
   too. `session.undo.body.reason` carries the user's
   rationale; the `ops_log` carries the timestamp of the op.
4. **Redo is a forward-pointing undo.** `session.redo.target`
   is **at or after** the most recent undo's `target`.
   Conceptually, redo re-extends the visible suffix.
   `session.redo.ref` may point at the prior undo entry it
   reverses; the projection uses `ref` to disambiguate when
   multiple undos are stacked.

**Cost does not roll back.** This is the load-bearing
invariant:

- The **context** rolls back: the surface presented to the
  model is truncated to `target`. Token **spend** does not.
- Every `model.usage` row that was appended before the undo
  is **preserved** verbatim, including its `tokens_in` /
  `tokens_out` / `cache_read` / `cache_write`. Cost reports
  keep the true spend — the agent was paid to think those
  tokens; rolling back the budget would be a billing lie.
- `state.tokens_used` is a *cache*; it does not store cost.
  The authoritative cost is the sum of `model.usage` rows in
  the ledger, which the undo does not touch.

**What this looks like in `derive`'s surface:**

- Entries with `seq > session.undo.target` are **retired**:
  the projection's `PROMPT_FOR_MODEL` view omits them
  (they are past the user's "rewind to here" point).
- `state.iteration_count` and `state.tokens_used` reflect the
  **current** iteration (after the rewind), not the sum of
  pre-rewind history.
- `state.phase` resumes from `Running` at the target turn's
  end.
- A new `model.usage` row after the undo is **appended** to
  the ledger as the new work happens, in `seq > target`. The
  cost report queries `SUM(tokens) FROM model.usage WHERE
  wall_ts <= ...` — the historical rows are still there,
  counted, never deleted.

**Stability** (see `loop.md` "Derive Stability"):
`session.undo` is a **boundary-anchored reordering**. The
prefix up to `target` is unchanged; the suffix is hidden.
The prompt-cache key for the prefix is preserved (it's the
*same prefix* the model saw before the undo). The new
iteration's prompt adds the undo's body and a new model
call at `target`'s next iteration; the cache breaks at that
point (intentional — the user rewound).

**Redo-vs-undo ordering.** If a user undoes, then makes
progress, then undoes again — the second `session.undo` is
just a row. The projection applies the **most recent**
undo at each `derive` call. Redo is similarly "the most
recent redo that cancels a currently-active undo".

```
# pseudo-projection
def mask_suffix(surface, undos, redos):
    active = latest_active_undo(undos, redos)   # walks both lists
    if active is None:
        return surface                        # no undo in effect
    return [e for e in surface if e.seq <= active.target]
```

`latest_active_undo` is a small interpreter: it walks the
undo / redo timeline, applies them in order, and returns the
`target` of the undo that is currently in force (or `None` if
the timeline ends in an undone state). The projection then
truncates at that `target`.

#### Encryption (static at-rest, future-proofing)

Three layers, in order of immediacy:

| Layer | Today | Future |
|-------|-------|--------|
| **Static at rest — db** | OS directory permissions + the redaction mechanism (above) is the primary privacy layer today. | `sqlcipher` — full SQLite encryption, transparently to the schema. Key material in OS keychain / KMS. |
| **Static at rest — blobs** | OS directory permissions; the redaction + `blob_redact_unlink` op removes sensitive bytes from disk. | Per-blob symmetric encryption keyed by the session's wrapping key; key unwrapped on session open. |
| **In-flight — hook content** | `harness.feature` (and any other hook-supplied `entry` payload) is **encrypted at the row level** before it reaches the ledger. The session key wraps the hook key. | Same shape; the hook key rotates per the hook's lifetime, not the session's. |

The redaction mechanism is the **first line of defence** —
most "delete" requirements are met by making the body
unreadable, not by reaching for cryptographic keys.

The encryption roadmap is:

1. **Today** — directory permissions (chmod 700 / per-user)
   + the redaction layer. Redacted blobs are physically
   unlinked, so the body bytes do not sit on disk.
2. **Next** — `sqlcipher` for the main db. Schema unchanged.
   Key material in OS keychain (Keychain on macOS,
   libsecret on GNOME, Windows DPAPI).
3. **Later** — per-blob encryption. Hook payload encryption
   is in scope from day 1 of step 2.

`hook` design itself is not in this document — see future
`docs/architecture/core/hooks.md` — but the invariant
**"hook payloads are encrypted at rest, just like other entry
bodies"** is stated here so any hook design must respect it.

#### Kind Compatibility Principle (forward-only)

Kind handling is **asymmetric** by design:

- **Write-strict.** The runtime accepts an `entry.append` **only
  if its `kind` is registered in the kind registry** (see
  below). Unknown kinds are **rejected** with a write error.
  This prevents the runtime from sprinkling unknown kinds
  into the ledger; the registry is the gate.
- **Read-lenient.** A reader that encounters a row whose
  `kind` it doesn't know **does not error**:
  - The row stays in the ledger (not deleted, not modified).
  - `derive` **skips** the row in the surface (the model
    doesn't see a half-rendered unknown).
  - Audit / inspection tools **show** the row with its raw
    body and `kind`, so operators can see what's in the db
    even if no reader knows how to render it.
  This is forward-compat: an old reader (Rust, Kotlin,
  debugging tool) can open a newer ledger without crashing.

The asymmetry prevents production data corruption (writes
that nobody can read) while never losing data to a
reader's missing knowledge.

#### Kind Registry (append-only)

Every kind the runtime knows about is registered **once** in
the **kind registry**. The registry is the **authoritative
list of allowed kinds**; anything not in it is rejected at
write time.

The registry is **append-only** — kinds are added, never
deleted. A deprecated or retired kind stays in the registry
forever, so old entries always have a definition to look up.

| Field | Type | What it holds |
|-------|------|---------------|
| `kind` | TEXT PRIMARY KEY | The dotted kind name (e.g. `tool.call`). |
| `family` | TEXT NOT NULL | The family prefix (e.g. `tool`). |
| `status` | TEXT NOT NULL | Lifecycle stage: `active` (current) \| `deprecated` (still written, prefer alternatives) \| `retired` (no longer written, but old entries still readable). |
| `body_schema_ref` | TEXT NOT NULL | Pointer to the canonical proto definition (e.g. `spec/proto/garive/v1/agent.proto#ToolCall`). Readers use this to know how to decode the body. |
| `default_surface` | TEXT NOT NULL | How `derive` projects this kind by default: `full` \| `preview` \| `one_liner` \| `redacted_placeholder`. |
| `pinned` | INTEGER (bool) | `1` if this kind is always-loaded (e.g. `goal.*`, `system`). |
| `pair_kind` | TEXT NULL | The kind this entry pairs with (e.g. `tool.call` ↔ `tool.result`). `NULL` if unpaired. |
| `redactable` | INTEGER (bool) | `1` if entries of this kind can be `privacy.redact`-ed. `0` for kinds whose body must never be hidden (e.g. `privacy.redact` itself, `compaction.rewrite`, audit kinds). |
| `registered_at` | INTEGER | Wall-clock when the kind was first registered. |
| `deprecated_at` | INTEGER NULL | Wall-clock when the kind moved to `deprecated` or `retired`. |
| `notes` | TEXT | Free-form. Used for migration notes, deprecation reasons, etc. |

**Lifecycle transitions** (one-way):

```
active ──▶ deprecated ──▶ retired
```

- **`active`**: writers may emit this kind; readers project it
  as `default_surface`.
- **`deprecated`**: writers should prefer the replacement
  kind (recorded in `notes`); readers still project the
  default surface; `derive` may emit a soft warning to the
  surface ("this entry is from the deprecated `kind.x`
  family; prefer `kind.y`").
- **`retired`**: writers must not emit this kind. **Readers
  continue to read it** — old entries stay readable forever.
  `derive` still skips it (the registry says the kind is
  known, the status says it's not in the live set).

A kind never goes from `retired` back to `active`. A
deprecated kind that turns out to be needed may stay
deprecated forever; the right move is to register a new
`active` kind that replaces it.

The kind registry itself lives in `spec/proto/` — the same
proto file defines the kind, its body schema, and the
metadata in this registry. Both Rust and Kotlin generate
their kind-aware code from this single source.

#### Unknown-Kind Handling (Read Path)

| Path | Behaviour |
|------|-----------|
| `append(kind=X)` | **Error**: `UnknownKind`. The runtime refuses the write before any row is created. The error message includes the candidate kind and a pointer to the kind registry ("register it in `spec/proto/` first"). |
| `derive()` on a row with unknown kind | **Skip**: the row is not in the surface. The skip is recorded in `ops_log` as `unknown_kind_skip` with the row's `kind` and `seq`, so an operator can see what's being skipped. |
| Audit / inspection tool reading a row with unknown kind | **Show raw**: the tool prints `kind=<name>, seq=<n>, body=<bytes-as-base64>`. The operator decides what to do (probably: register the kind, or accept it as an external write). |
| `surface_visible` on a row with unknown kind | `0` (skipped). The registry doesn't know the kind, so the surface policy doesn't know how to project it. |
| Re-deriving a session that has unknown-kind rows | All known kinds render normally; unknown-kind rows are still skipped, with a count reported in `derive`'s return value. |

The intent: **forward compat without silent loss.** An old
reader can open a new ledger (no crash), and an operator can
see exactly which rows are unrendered (no silent skip).

#### Versioned reads (schema evolution)

Each entry carries a `schema_var` (per `entry` table). The
`schema_var` is the **version of the kind's body schema**
that the row was written under. When the kind's body schema
changes (a field is added, a sub-message is renamed), the
kind's `body_schema_ref` in the registry points at the new
version, and **the next write uses the new version's
`schema_var`**. Old rows keep their old `schema_var`.

A reader that knows the current `body_schema_ref` may not
know old versions. The mitigation:

- The proto definition supports **forward compat per
  field** (proto3 default — unknown fields are preserved
  on parse, not dropped).
- A `migration` registry row lists, per kind, how old
  versions' payloads are brought forward. The runtime's
  `migrate(entry)` function applies it lazily at read time.
- The migration function is **never destructive** — it
  converts old payload → new payload, but the original is
  preserved in `body_inline` / `body_hash` (which is
  content-addressed, so the original is recoverable).

```sql
CREATE TABLE kind_migration (
    kind             TEXT NOT NULL,
    from_schema_var  INTEGER NOT NULL,
    to_schema_var    INTEGER NOT NULL,
    migration_path   TEXT,        -- description or executable identifier
    PRIMARY KEY (kind, from_schema_var, to_schema_var)
);
```

When `derive` reads a row with a stale `schema_var`, it
looks up the migration path and applies it. The migrated
body lives in the **surface** only; the ledger row keeps
its original `body_inline` / `body_hash` (immutable, per
append-only invariant).

**Why forward-only:** we never delete a kind or a
`schema_var`. The registry grows; the migrations grow; old
rows are always readable. The cost is more code, more
migrations, more registry rows — the benefit is **no data
loss across schema evolution**, ever.

#### Load classes

Kinds split into three load classes (see `loop.md` for
details):

- **Always-loaded** (`goal.*`, `system`): `pinned=1`,
  `surface_visible=1`, never summarised. `goal.*` rows are
  all pinned because they are the model's frame; `derive`
  projects the *current* one (via the algorithm above).
- **Body** (`text.*`, `tool.*`, `governance.*`, `compaction.*`,
  `model.usage`): subject to compression + eviction;
  `surface_visible` flips to `0` as the entry ages out.
- **Meta** (`session.turn_start`): boundary markers,
  pinned, always present, but invisible to the model. `meta`
  table captures session-level tags instead.
- **Branch** (entries with `branch_id` non-null): **in-session
  branches** that are not the active mainline. By default
  they are **excluded from the surface** — see "Branches
  (in-session lightweight fork)" below.

### Branches (in-session lightweight fork)

When the agent wants to **try several strategies and pick the
best** within one session, it can do so by opening
**lightweight branches** rather than forking a whole new
session. A branch is a `branch_id` non-null on the entries
between `branch.open` and `branch.verdict`. The whole
mechanism rides on top of the masking family — it does not
need a new ledger or new state, just two new kinds and one
new column.

```
# Example
branch.open{branch_id:"A", from_seq:100, purpose:"方案A"} → 试A
branch.open{branch_id:"B", from_seq:100, purpose:"方案B"} → 试B
branch.open{branch_id:"C", from_seq:100, purpose:"方案C"} → 试C
branch.verdict{branch_id:"B", decision:"adopt", reason:...}   → 选B
branch.verdict{branch_id:"A", decision:"discard", reason:...} → 弃A
branch.verdict{branch_id:"C", decision:"discard", reason:...} → 弃C
```

**Path-style nesting.** `branch_id` is a **path**:
top-level branches are `"A"`, `"B"`, `"C"`; nested branches
are dotted (`"A.alt"`, `"A.alt.deep"`). The **first segment**
is the top-level branch; segments after the first dot are
descendants. Constraints:

- A sub-branch's `from_seq` must be **between** the parent
  branch's `from_seq` (inclusive) and the parent branch's
  `branch.verdict.seq` (exclusive).
- A sub-branch is **opened and resolved within the parent
  branch's lifetime**; it does not auto-promote to mainline
  when the parent is adopted (the parent's verdict operates
  at its own level; the sub-branch has its own verdict).
- Surface projection: an entry's `branch_id` is a prefix
  match. Adopted `"A"` includes `"A"`, `"A.alt"`,
  `"A.alt.deep"` (if those were adopted too). Adopted
  `"A.alt"` only includes `"A.alt"`, not its parent or its
  siblings.

```
def branch_visible(branch_id, adopted_set):
    # adopted_set is {branch_id, ...} from the latest verdicts
    for adopted in adopted_set:
        if branch_id == adopted or branch_id.startswith(accepted + '.'):
            return True
    return False
```

**Surface projection for branches.** The default
`PROMPT_FOR_MODEL` projection follows three rules:

1. **Mainline only** by default. Entries with `branch_id IS
   NULL` are always on the surface; entries with
   `branch_id` non-null are **excluded** unless the branch
   has been `adopt`-ed.
2. **Adopted branches count as mainline.** When
   `branch.verdict{branch_id:X, decision:"adopt"}` lands, the
   branch's entries become mainline for projection purposes
   (their `branch_id` is preserved on the row for audit, but
   the surface includes them). Adopt propagates to **all
   adopted descendants** of `X` (path-prefix match).
3. **Discarded branches are hidden.** A `discard` verdict
   retires the branch from the surface projection. The
   entries still live in the ledger — they are not deleted,
   just masked — and audit / `AUDIT_REPLAY` see them.

Undecided branches (no verdict yet) are treated as discarded
from the default projection. The model can opt in to
"exploratory view" via a `BRANCH_VIEW_ALL` projection that
shows every branch side-by-side; that's how the
`FORK_BRANCH` projection differs from the mainline-only
default.

**Branch verdict is governance-shaped.** `branch.verdict`
follows the same shape as `governance.verdict`:
`{branch_id, decision, reason, ref?}`. The branch
verdict's `ref` may point at the `branch.open` it resolves.
A branch verdict can itself be **overruled** by a later
verdict (re-adopt a previously-discarded branch; the
projection uses the **most recent** verdict for each branch).

**Cross-branch and cross-session references.** A
`branch.*` entry's `uid` is its global identity; a `ref`
pointing at a branch entry from another session (or from
this session's mainline into a branch) uses the standard
`{session, uid}` shape. The branch's `purpose` is the
single human-readable field — a one-line description of
what the branch is trying.

#### Branch analytics: cost, learning, weight

The branch machinery pays for itself across three axes.

**1. Per-branch cost accounting.** A branch's
`model.usage` rows carry `branch_id` set. The
`memory_watermark` / cost queries naturally group by
branch:

```sql
SELECT branch_id, SUM(tokens_in) AS in_, SUM(tokens_out) AS out
  FROM entry
 WHERE kind = 'model.usage' AND branch_id IS NOT NULL
 GROUP BY branch_id
 ORDER BY in_ + out DESC;
```

Output is "branch A cost X tokens, branch B cost Y, branch C
cost Z, the mainline cost W". The user can see which
strategies are cheap and which are expensive — **a real
metric for "how much did it cost to try this approach?"**.
Adopted branches are not double-counted: the agent does the
work once, the row is counted once.

**2. Discarded branches feed long-term memory (dream).**
The `memory_watermark` op walks the ledger and extracts
durable facts into long-term memory. **Discarded branches are
the highest-value input to that walk** — they are the
agent's empirical record of "I tried X and it didn't work
because Y". This is exactly the kind of fact worth carrying
forward: future sessions of the same user / project should
not re-discover it.

The dream walk (forthcoming `docs/architecture/core/dream.md`)
special-cases `branch.*` rows:

- For every `branch.verdict{decision:"discard", reason:...}`,
  extract the failure pattern (`purpose`, `from_seq`,
  surrounding tool results) into long-term memory.
- For every `branch.verdict{decision:"adopt", reason:...}`,
  extract the success pattern — but **only if** the branch
  has a non-trivial `purpose` (otherwise it's noise).
- The walk uses the existing `memory_watermark` row to
  checkpoint progress, so a crashed dream walk resumes
  cleanly.

The **immutability** of the ledger is what makes this work:
a discarded branch is **never deleted**, the failure
pattern stays readable forever, and the dream walk can find
it whenever it runs.

**3. Weight escalation: short branch ↔ long sub-agent.**
A branch is **in-process and lightweight** — one entry's
`branch_id` set. When the agent decides a strategy needs
**long exploration** (sub-tasks, parallel agents, hours
rather than minutes), the right move is to **escalate the
branch into a sub-agent** rather than carry on inside the
branch:

- Spawn a new session for the sub-agent (a new
  `<root>/<sub-session>/` directory, new `ledger.db`).
- Pass the **branch's `from_seq` snapshot** as a
  `harness.feature` entry into the sub-session.
- The sub-agent returns a verdict (also a row in the
  sub-session's db).
- The **parent session receives a `compaction.summary` or a
  new `harness.feature` row carrying the result**, with
  `ref = {session: <sub-session-uuid>, uid: <result-uid>}`.

The two-tier pattern:

```
session (main)                              sub-agent session
─────────────                               ──────────────────
branch.open{A, from_seq:100}               (harness.feature: goal)
branch.* ...                                 ... runs long
branch.verdict{A, discard}                   ...
                                          returns verdict via ref
```

A **short** exploration stays inside the parent's branch
machinery. A **long** exploration escalates to a sub-agent.
The decision boundary is the agent's, not the runtime's —
the runtime just makes both paths cheap to take.

**4. Branch trees are the audit trail.** The path
`A.alt.deep` is also a literal path through the
exploration tree. `AUDIT_REPLAY` shows the full tree;
`FORK_BRANCH` shows side-by-side comparisons; the
`BRANCH_VIEW_ALL` projection shows every leaf and its
verdict. The user can answer "what did the agent try, and
why did each attempt fail or succeed?" from a single
ledger scan.

#### Relation to `session.fork`

| | `branch.*` (intra-session) | `session.fork` (inter-session) |
|---|---|---|
| Scope | Same session db | New session directory + new db |
| Tag | `branch_id` column | `ledger_meta.lineage` |
| Cost | One `branch_id` set per row | Whole directory + db copy |
| Use | "Try 3 solutions to this step" | "Take the whole conversation elsewhere" |
| Granularity | Step / sub-step | Turn / whole session |
| Status | Siblings, not replacements | Siblings, not replacements |

### Why content-addressed blobs

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

## Summary — 1 main + 3 aux + 1 ops

The schema lands at **5 tables** with a deliberate split:

| | Table | PK | Business key | Role |
|---|---|---|---|---|
| **Main** | `entry` | `seq` (monotonic, loop-controlled) | `seq` itself | The ledger events. One row per `entry.append`. **All metadata is inline on the entry** — fractal structure (`ext` BLOB), source (`provenance`), pairing (`pair_ref`), framing (`pinned`, `surface_visible`), window timing (`wall_ts`), version (`schema_var`), span (`covers_*`), history (`superseded_by`). |
| **Aux 1** | `blob` | `id` AUTOINCREMENT | `hash` (sha256, UNIQUE) | Content-addressed large bodies. |
| **Aux 2** | `dedup` | `id` AUTOINCREMENT | `client_generation` (UNIQUE) | Idempotency table for retries. |
| **Aux 3** | `ledger_meta` | `id` AUTOINCREMENT | `key` (UNIQUE, documented) | Session-level KV — schema_version, session_id/mode/agent, timestamps, lineage, watermarks. |
| **Ops** | `ops_log` | `id` AUTOINCREMENT | — | Operations history (GC, vacuum, sweep, migration). |

**Pattern**: every **aux / ops** table has
- `id INTEGER PRIMARY KEY AUTOINCREMENT` — internal handle, never user-facing.
- A **business key** with `UNIQUE` — the value the rest of the schema looks up by.
- The main `entry` table is the only one whose PK is not auto — `seq` is loop-controlled
  so that `derive` can reason about ranges (`seq > ?`, `covers_start..covers_end`)
  without the auto-increment semantics getting in the way.

**Aux table operations:**

| Operation | `id` | business key |
|-----------|------|--------------|
| Insert | auto-assigned | must be unique (UNIQUE enforces) |
| Lookup | rare (internal joins) | common (`SELECT … WHERE hash = ?`) |
| Update | forbidden (append-only) | forbidden (append-only) |
| Delete | forbidden; GC is the exception | forbidden; GC deletes dedup rows by `client_generation` |

The `id` column exists for **physical storage** (cluster key
in SQLite B-tree) and **internal joins**; the business key is
the API. Future code never says `WHERE id = 42` — it says
`WHERE hash = 'sha256:abc…'` or `WHERE client_generation = 'uuid-…'`.

---

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