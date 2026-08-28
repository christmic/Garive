# Ledger — Append-only Round Log

> **The ledger is the single source of truth for an
> `agent_turn`'s state.** It is append-only, durable, and
> replayable. The surface the LLM sees is a **lossy
> projection** of the ledger; the loop never *deletes* from
> the ledger. Together with the loop (`loop.md`), the ledger
> implements "the model never loses information that matters,
> and never re-pays for information it once saw."

This document describes the ledger **as a design**. Specific
storage backends (SQLite, file, remote), row layouts, and
index definitions are **policy** that land with the slice —
the *mechanism* (append-only, replayable, seq-monotonic) is
what the codebase ships.

## Context

The loop needs to answer four questions reliably:

1. **Recoverable:** if the process dies mid-round, where
   should it resume? Answer: read the ledger.
2. **Auditable:** why did this round call this tool at this
   iteration? Answer: read the ledger.
3. **Bounded:** the model's context window is finite; how
   do we give it the right slice? Answer: derive from the
   ledger.
4. **Multi-language:** Rust and Kotlin both need to read the
   same data with the same semantics. Answer: the ledger is a
   sequence of typed entries; the language reads what its
   `derive` needs.

These together rule out an in-memory-only store (no recovery)
and a hand-rolled binary log (no schema, no cross-language).
A typed, append-only log with a small, well-defined entry
schema fits.

## Options Considered

### A. In-memory only

`Vec<Entry>` in process memory.

Rejected. Loses everything on crash. Cannot recover.

### B. Plain text log (JSONL)

One JSON object per line.

Rejected. Cheap to write, but parsing on every read is slow
without an index; cross-language readers would need to agree
on serialisation order; no schema enforcement.

### C. SQLite

A small embedded database, one row per ledger entry, indexed
by `seq` and `kind`.

Considered. Pros: durable, queryable, well-understood, native
to most language ecosystems. Cons: schema migration overhead;
not trivial to share across processes.

**Selected as the default.** The schema lives in
`spec/proto/` (or a sidecar `.sql`) so both Rust and Kotlin
generate against it.

### D. CRDT / event-sourced framework

e.g. `durable-streams`, `eventfold`, custom CRDT.

Considered. Overkill for a single-process ledger. Future
option if multi-process writers become a requirement.

### E. Custom binary log with sidecar index

Tempting for performance; rejected for the same reasons as
B + maintenance cost.

## Decision

The ledger is an **append-only log of typed entries**, one
append-only stream per `agent_turn` (per-turn segments). Each
entry has a monotonically increasing `seq` (per turn). The
ledger is durable, replayable, and queryable by `(turn_id,
seq_range)` and `(turn_id, kind)`.

### Top-level shape

```
Ledger (process-wide)
└── TurnSegment  one per agent_turn
    ├── id          turn_id (UUID)
    ├── started_at  wall-clock
    ├── ended_at    wall-clock (or null while Suspended)
    ├── state       snapshot of `state` at the most recent append
    │              (phase, iteration_count, tokens_used, ...)
    └── entries     append-only list of Entry

Entry {
    seq           u64, monotonic per turn
    kind          string, see "Entry Kinds Catalog" below
    produced_by   enum { model, runtime, governance, system }
    produced_at   wall-clock
    turn_seq      sequence number within the turn
    payload       structured per kind
    covers        optional seq_range (only on summary entries)
    provenance    opaque token for "where this came from"
}
```

A turn segment is the **resumable unit**. `Resume` re-attaches
to the in-flight turn's segment; a fresh user message starts
a new turn segment. Ledger is append-only — segments are
created lazily but never destroyed.

### Entry Kinds Catalog

Each kind is the unit of `derive` filtering. Future kinds
land by adding a row to this catalog and a corresponding
typed payload — nothing else changes.

| Kind | Produced by | Payload (key fields) | Notes |
|------|--------------|----------------------|-------|
| `user.message` | runtime | `{text, attachments[]}` | One entry per user turn entry. |
| `assistant.message` | model | `{text, intents[]}` | Model's reply before judge. |
| `assistant.tool_call` | model | `{tool, args, call_id}` | One per model-intent. Pairs with `tool_result`. |
| `tool_result` | executor | `{call_id, output, status}` | The result of a tool call. Pairs with `assistant.tool_call`. |
| `tool_result.rejected` | executor | `{call_id, reason}` | When governance denied. Carries the reason back to the model. |
| `verdict` | governance | `{intent_seq, decision, rewrite?}` | Governance's call on an `assistant.tool_call`. |
| `effects` | executor | `{verdict_seq, items[]}` | What the executor did. |
| `approval_request` | runtime | `{verdict_seq, question}` | AskUser verdict. Round pauses here. |
| `approval_response` | runtime | `{approval_request_seq, answer}` | Human's reply on Resume. |
| `summary.v1` | model | `{covers, fields: {goal_progress, confirmed_facts, actions_taken, state_progress, open_questions}}` | See `loop.md` "Summary Entry Schema". |
| `rewrite_directive` | runtime | `{summary_seq, covers}` | Triggers `derive`'s reset path. |
| `goal` | system | `{text}` | Frame kind — always-loaded, never summarised. |
| `system` | runtime | `{text, version}` | Frame kind. |

Kinds split into three load classes (see `loop.md` for
details):

- **Always-loaded** (`goal`, `system`): always on the surface,
  never summarised.
- **Body** (`user.message`, `assistant.message`,
  `assistant.tool_call`, `tool_result`, `tool_result.rejected`,
  `verdict`, `effects`, `summary.v1`, `rewrite_directive`,
  `approval_request`, `approval_response`): subject to
  compression + eviction.
- **Meta** (`turn.start`, `turn.end`): boundary markers,
  always present, never on the surface.

### Summary Entry Payload (the structured fields)

A `summary.v1` payload is **not** a blob — it is the
`Summary Entry Schema` from `loop.md`:

```python
class SummaryV1:
    covers:                # the seq_range it replaces
        start_seq: u64
        end_seq:   u64      # inclusive
    fields:
        goal_progress:    str
        confirmed_facts:  list[Fact]
            # each Fact = { tool, call_id, key_result: str }
        actions_taken:    list[Action]
            # each Action = { tool, args_summary: str, outcome: str }
        state_progress:    Phase   # mirrors state.phase
        open_questions:    list[str]
```

Two consequences:

1. The LLM that produces the summary is **structured-output
   constrained** — it returns the fields, not a paragraph.
   The schema is enforced by the wire contract, not by
   post-hoc parsing.
2. The next surface reads `summary.v1.fields.*` directly;
   no LLM is needed to **parse** the summary, only to
   **reason** about it.

### Boundary Invariants

The cover boundary of a `summary.v1` must be **logically
clean**. See `loop.md` "Boundary Invariants" for the full
rules. Briefly:

- **Tool calls must appear as a pair**: a `summary.v1`'s
  `covers.seq_range` must contain both the
  `assistant.tool_call` and its paired `tool_result`.
- **Pending `approval_request`** must be fully inside the
  covered range (with its eventual `approval_response` and
  `effects` back-filled) or fully outside it.
- **Respect iteration boundaries**: a summary should end at
  an iteration boundary, not mid-iteration.

The `summarize(prefix)` implementation is responsible for
**extending the prefix until the boundary is clean**. The
mechanism doesn't *enforce* it — the policy is what extends.

### Persistence

The default backend is **SQLite**, one file per process:

```
<data_dir>/ledger.db
```

Tables:

```sql
CREATE TABLE turn (
    id          TEXT PRIMARY KEY,         -- turn_id (UUID)
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER,                  -- NULL while Suspended
    state_blob  BLOB                      -- JSON snapshot of state
);

CREATE TABLE entry (
    turn_id      TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    produced_by TEXT NOT NULL,             -- model | runtime | governance | system
    produced_at INTEGER NOT NULL,
    payload     BLOB NOT NULL,             -- protobuf-encoded per kind
    covers_start INTEGER,
    covers_end   INTEGER,
    provenance  BLOB,
    PRIMARY KEY (turn_id, seq)
);

CREATE INDEX entry_turn_kind ON entry(turn_id, kind);
CREATE INDEX entry_turn_seq   ON entry(turn_id, seq);
CREATE INDEX summary_covers   ON entry(turn_id, covers_start, covers_end) WHERE kind LIKE 'summary.%';
```

The `summary_covers` partial index makes the
`rewrite_directive` lookup fast even in long rounds.

**Cross-language:** both Rust (`engine/ledger/`) and Kotlin
(`engine-kt/ledger/`) generate the same SQLite schema from a
shared description. The conformance suite asserts schema
equivalence across languages.

### API Surface

The ledger exposes a small, query-shaped API. Every
operation has a **pure** form (over an in-memory snapshot)
and an **impure** form (reads from SQLite). Tests run against
the pure form; production uses the impure form.

| Method | Returns | Notes |
|--------|---------|-------|
| `append(turn_id, entry)` | `seq` (the new one) | The only write op. Monotonic per turn. |
| `entries_since(turn_id, seq)` | list[Entry] | `seq > seq`, ordered ascending. |
| `latest_seq(turn_id)` | u64 | Highest seq written, or 0 if turn is empty. |
| `latest_active(kind)` | Entry or null | Most recent non-superseded entry of the given kind. |
| `rewrite_directive_since(turn_id, seq)` | Entry or null | Most recent `rewrite_directive` after `seq`. (Per `loop.md`, derive inlines this.) |
| `range(turn_id, start, end)` | list[Entry] | Half-open `[start, end)`. |
| `open_turn()` | TurnSegment | Most recent turn that has not ended yet. |
| `turn(id)` | TurnSegment | By id, or null. |

`append` is the only mutation. `entries_since` and
`rewrite_directive_since` are the two reads the loop actually
calls (per `loop.md`).

### Multi-turn Segments

The ledger is one process-wide database, but turns are
**segments**: a `turn` row plus its `entry` rows. Multiple
turns coexist; older turns are immutable but their data is
preserved for replay / audit.

A round's `agent_turn` is bound to a single `turn_id`; cross-
turn reads are explicit (e.g. "find the most recent `goal`
in the user's history"). Default `derive` reads only the
current turn.

## Consequences

### Positive

- **Recoverable by construction.** `Resume` re-attaches to the
  in-flight `turn_id`; the loop re-derives the surface from
  the ledger. Nothing in memory is required to survive a
  crash.
- **Auditable.** Every decision — model intent, governance
  verdict, executor effect, approval request — is in the
  ledger with provenance. Reconstruct any past iteration.
- **Cross-language.** SQLite is the lingua franca. Both Rust
  and Kotlin share the schema; the conformance suite asserts
  semantic equivalence at the entry-level, not just the
  proto-level.
- **Lossless projection.** Surface is derived; ledger never
  forgets. `seq_pointer` re-resolves any dropped content.
- **Compression is safe.** Three-mechanism stack (structured
  summary + clean cover boundary + seq-pointer back-reference)
  lets compression scale without the model re-doing work.

### Costs

- **SQLite per turn.** A long-running agent that opens
  thousands of turns accumulates one file. GC is a future
  concern.
- **Schema migration.** When a new `kind` lands, the SQLite
  schema doesn't change (payload is opaque BLOB), but
  readers must learn the new kind. Documented via the
  "Entry Kinds Catalog" above.
- **Single writer.** Two `agent_loop` instances in one
  process would race. Today's design assumes one writer; a
  future multi-loop process needs a different ledger (this is
  one of the Open Questions).
- **Bulk-export.** Migrating a turn off the device requires
  the SQLite file + a schema version + the matching readers.
  Not a daily operation, but not free either.

## Open Questions

1. **Multi-process writers.** Today: one loop = one ledger
   writer. When does this need to change? (Fork, multi-agent
   coordination, distributed runtime.) And what's the
   consensus model — Raft, CRDT, leader-follower?
2. **Retention.** When does a turn segment get archived or
   deleted? Per-turn TTL? User-driven? Cold-storage tier
   (cold SQLite, S3, etc.)?
3. **Schema versioning.** When `kind` adds a new field, how
   do readers that don't know the new field react? (Default
   `serde(default)`-style fallback in Rust/Kotlin? A separate
   `version` field per kind?)
4. **Cross-language round-trip tests.** Conformance currently
   covers `spec/proto/*.proto`. Should it also cover ledger
   entries (encode → decode → re-encode, bytes match)?
5. **Append throughput.** A busy agent might append
   hundreds of entries per iteration. SQLite WAL handles this,
   but bench it.

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
- Status: **draft (possible mechanism)** — entry kinds
  catalog and SQLite schema are candidates; specific payload
  encodings, indexes, and retention policy land with the
  slice. No final code.