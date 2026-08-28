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

#### `blob` — dedup / registry

A blob lives at `<session>/blobs/<sha256>` on disk and is
referenced by hash from `entry.body_hash`. This table is
the **single index** that maps hashes to file paths and
records metadata.

| Column | Type | What it holds |
|--------|------|---------------|
| `hash` | TEXT PRIMARY KEY | `sha256:<hex>` — the content hash. |
| `size` | INTEGER | Bytes. |
| `mime` | TEXT | Best-guess MIME type. |
| `first_seen_seq` | INTEGER | First `entry.seq` that referenced this blob. |
| `last_seen_seq` | INTEGER | Most recent `entry.seq` that referenced this blob. |
| `refcount` | INTEGER | Live references (entry rows with this hash and `superseded_by IS NULL`). For GC. |
| `path` | TEXT | Relative path under the session directory (`blobs/<hash>`). |

Multiple `entry` rows referencing the same blob hash **do
not** duplicate bytes on disk — that's the point of the
dedup table. `refcount` tells GC when a blob can be deleted
(no live entries reference it).

#### `dedup` — body-level dedup

Independent of the blob store: even inline bodies may be
duplicated (e.g. the model emits the same `assistant.text`
twice in a retry loop). `dedup` lets a future compaction
pass find duplicates cheaply.

| Column | Type | What it holds |
|--------|------|---------------|
| `hash` | TEXT PRIMARY KEY | Content hash (sha256) of the **value**, regardless of how it's stored (inline or external). |
| `first_seen_seq` | INTEGER | First entry that produced this value. |
| `count` | INTEGER | How many entries reference this value. |

(`dedup` is used for analytics / future compaction. The
authoritative record is still `entry`.)

#### `meta` — session metadata

Session-scoped tags. Mostly written once at session start,
read often.

| Column | Type | What it holds |
|--------|------|---------------|
| `key` | TEXT PRIMARY KEY | Tag name (e.g. `agent`, `model`, `created_at`, `pid`, `session_kind`). |
| `value` | TEXT | Tag value. |
| `wall_ts` | INTEGER | When this tag was set. |

`meta` is the equivalent of `meta.json` but inside the db, so
it joins naturally with the rest.

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