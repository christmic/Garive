# Derive 4-dimension Test Contract — design

> `derive` is a **pure function** —
> `(current_surface, new_entries, masking_timeline,
> projection_args) → derived_surface` (see `loop.md` "Derive
> in Detail"). Pure functions admit a strong test contract.
> This document specifies the **four dimensions** the test
> suite must cover and the design of each.
>
> The constitutional rule (4 dims must exist) lives in
> `.agents/testing.md`. The detailed design is here.

## TL;DR

| Dim | Question it answers | Cadence |
|-----|----------------------|---------|
| 1. **Golden snapshot** | "Does `derive` produce exactly the surface we expect for a known ledger?" | Per-PR (CI) |
| 2. **Property tests + matrix** | "Do the invariants hold under arbitrary inputs?" | Per-PR |
| 3. **Fact retention rate** | "Does compression + derive actually preserve the model's ability to recall?" | Nightly |
| 4. **Cross-language equivalence** | "Do Rust and Kotlin produce byte-equal surfaces for the same input?" | Per-PR |

The four dims together form a closed loop: every class of
regression lands in at least one bucket.

## Dim 1 — Golden snapshot regression

A snapshot is a `(ledger, masking_timeline) → derived_surface`
triple, stored as a fixture. The test is "the runtime's
`derive` produces **exactly** the expected surface". Snapshots
are **hand-written** by a maintainer who:

1. Constructs a small ledger (5–20 entries) with a known
   masking timeline.
2. Walks the 6-step pipeline by hand, recording the expected
   surface.
3. Writes the ledger, the timeline, and the expected surface
   as a fixture.

Snapshots live in `engine/core/tests/snapshots/derive/` and
are loaded by the test runner. Each snapshot is named for the
case it covers:

```
golden__pure_append/
golden__compaction_summary_replaces_prefix/
golden__undo_masks_suffix/
golden__redo_restores_suffix/
golden__branch_adopted/
golden__branch_discarded/
golden__redaction_placeholders/
golden__multiple_instructions_stacked/
```

When a snapshot fails, the test runner prints a diff
(`expected_surface vs derived_surface`) so the maintainer
sees **what** changed. The diff is small enough to read
visually because the snapshots are small.

## Dim 2 — Property tests + instruction interaction matrix

The **invariants** property tests must hold on every random
ledger / random instruction:

- **Pair completeness** — every `tool.call` row has a
  paired `tool.result` row visible on the surface (or marked
  redacted; never dangling).
- **`pinned` always on the surface** — `goal.*` and `system`
  rows are never masked, never evicted, always in
  `PROMPT_FOR_MODEL`.
- **Budget honoured** — when the surface fits, the count
  matches; when it overflows, the `new` part is **trimmed
  first** (and a `needs_summary` flag is set on the
  `assemble` receipt).
- **Masking instructions are cumulative** — applying undo
  + branch-discard + redact yields a surface that is the
  intersection of all three masks, in that order.
- **Reordering never happens** — the surviving `seq` order
  matches the order in the original ledger.

The **instruction interaction matrix** is the heart of
dimension 2. For every pair `(mask_a, mask_b)`, the test
constructs a ledger that exercises both, runs `derive`, and
asserts the combined effect matches the manually-computed
intersection. Then for every triple, every quadruple, up to
the **fan-out limit** of `derive` (the 6-step pipeline).
Fuzz coverage drives the matrix: each pair is hit at least
once, and known-interaction edge cases (`compaction.rewrite`
after a `session.undo` that covers the rewrite target) have
dedicated snapshots.

## Dim 3 — Fact retention rate (model-in-the-loop)

This is the **only** dimension that requires a real LLM. It
is a **pipeline test**, not an agent test:

```
┌─ ledger (with N facts) ─┐
│  user.text: "X is 42"    │
│  tool.result: "test passed"│
│  text.assistant: "Y"     │
│  ...                     │
└─────────────────────────┘
         │
         │ 1. force compaction
         ▼
┌─ compacted ledger ──────┐
└─────────────────────────┘
         │
         │ 2. derive
         ▼
┌─ surface (the "what model sees") ─┐
└──────────────────────────────────┘
         │
         │ 3. send to model + N questions
         ▼
┌─ answers ─────────────────┐
│  "X is 42"        ✓   │
│  "test passed"     ✓   │
│  "Y"               ✓   │
│  ...                    │
└──────────────────────────┘
         │
         │ 4. score
         ▼
retention = correct / N
```

The test's **three properties**:

- The N facts are seeded at **different positions in the
  ledger** (early / mid / late). Compression hits mid- and
  late-position facts harder than early ones; the test
  measures **position-dependent retention**.
- The compression is **forced** at a specific point, so
  retention is measured against a known compression rate.
- The model is asked the N questions in a **randomised
  order**, so the test does not bias toward position in the
  prompt.

**Pass criteria** (initial, before tuning): `retention ≥ 0.8`
for facts that fit in the surface, `retention ≈ 0` for facts
that were compressed away (the test verifies the **threshold**
between kept and dropped). These thresholds move as the
compression policy improves; the test records the threshold
it asserts against.

**Probes** — the test asks questions using a structured
"probe" format, not a free-form chat. Each probe is
`{question_id, expected_answer, surface_position}`. The
model's response is scored by exact match or by an
LLM-as-judge that checks semantic equivalence. The test
records:

- `retention` — the headline metric
- `compression_rate` — what fraction of the original ledger
  made it through compression to the surface
- `position_histogram` — retention as a function of where
  the fact was seeded
- `failure_modes` — categorised reasons (e.g. "summarised
  out", "lost in middle", "wrong number from deriv error")

The pipeline test is **not** the agent loop; it does not
generate a model intent, run a tool, or call governance. It
is the narrowest possible chain that still exercises
compression + derive + the LLM. Run nightly, not per-PR.

## Dim 4 — Cross-language equivalence

`derive` is implemented twice (Rust + Kotlin). The
conformance suite already covers proto-level byte equivalence;
dimension 4 extends that to **surface-level** byte equivalence
for the same `(ledger, masking_timeline)`:

```sql
-- pseudo: a small fixture that drives both languages
SELECT derive(ledger_id, masking_timeline_id) FROM sessions
WHERE id = 'conformance-derive-fixture'
```

Both languages run `derive`; the **entire surface** (kinds,
seq, body_inline / body_hash, surface_visible flag, pinned
flag, branch_path) is compared field-by-field. Differences
are diff'd for the maintainer. The test does **not** check
specifics — any divergence is a bug, period.

## Why four dims, not one

A single-dim suite misses bugs. The four dims together form a
**closed loop** of coverage:

- Dim 1 catches "you broke the obvious case" (specific
  snapshots).
- Dim 2 catches "you broke an interaction I didn't think of"
  (random + invariant).
- Dim 3 catches "compression lost facts the model needs"
  (end-to-end through the LLM).
- Dim 4 catches "the languages diverged" (Rust vs Kotlin).

A regression that survives dim 1 + dim 2 is rare; one that
also survives dim 3 (model can still recall) and dim 4
(languages are byte-equal) is very rare. When it happens, the
diagnosis is usually "the snapshot library is missing a
case" — and the fix is to add the snapshot, not to relax a
dim.

## When a dim fails

A failure in any of the four dims is **load-bearing** —
none of them is "advisory". The bug that dim 1 misses, dim 2
catches; the bug dim 2 misses, dim 3 catches; the bug dim 3
misses, dim 4 catches. The four dims together form a closed
loop: every class of regression lands in at least one bucket.

## See also

- `loop.md` "Derive in Detail (common path)" — the function
  being tested.
- `loop.md` "Change-locality matrix" — the split between
  `derive` (content) and `assemble` (serialisation). Dim 1–3
  test `derive`; dim 4 tests both languages' `derive` for
  equivalence.
- `.agents/testing.md` — the constitutional rule this design
  implements.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: **draft (possible mechanism)** — the 4-dim structure
  is settled; the specific fixture corpus, invariants, and
  probe format land with the slice. No final code.