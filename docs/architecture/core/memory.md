# Memory Layer — Design

> **Memory is the physical existence of `self`.** Runtime
> is disposable (process / surface / cache are derivatives);
> memory is not. This is the principle already decided at
> the top of the design (per the earlier conversation).
>
> The memory layer is the **next continent** after
> `turn_loop` closes. This document lays out the design
> across **eight angles**, then deep-dives the first. The
> rest follow in subsequent commits.

## The 8 angles (overview + order)

| # | Angle | One-line |
|---|-------|----------|
| ① | **Location + classification** | What memory is, the 4 types |
| ② | **Ownership boundaries** | Whose memory, vs project knowledge |
| ③ | **Write paths** | How memory is produced |
| ④ | **Data model + storage** | Row shape + `memory.db` separate SQLite |
| ⑤ | **Read paths** | Recall into the surface |
| ⑥ | **Maintenance policies** | ADD/UPDATE/DELETE/NOOP decisions |
| ⑦ | **Retrieval quality** | Recency × relevance × importance ranking |
| ⑧ | **External framework survey** | Mem0 / Letta / Zep / codex CC lessons |

The 8 angles are **ordered**: each builds on the previous.
This document fills in **①** deeply today; **②–⑧** are
placeholders for follow-up commits.

## ① Location + classification — what memory IS

Memory is **the agent's cross-session continuity itself**.
It is not "data the agent stores" — it is the substrate that
lets "the next session pick up where the last left off".
Runtime (process, surface, cache) is **disposable**; memory
is **persistent**.

### Four types — borrowed from cognitive science

The classification follows the classic psychology distinction
(episodic / semantic / procedural), augmented with a
lessons-learned type that is agent-specific:

| Type | Content | Primary source | Lifetime |
|------|---------|-----------------|----------|
| **语义 / 偏好·事实 (semantic)** | User's stable facts and preferences: "use SDKMAN for Java", "reply in Chinese" | `dream` extraction, explicit user statement | Long, updated on contradiction |
| **情景 (episodic)** | Which session did what — a light-weight index, not full text | Session-end auto-generation | Medium, down-weighted after `dream` distillation |
| **教训 (lessons)** | "This path didn't work because X" | `exit_summary`, user correction | Long (negative knowledge is the most valuable) |
| **程序 (procedural / playbook)** | Reusable workflow: "diagnose cache misses in 5 steps" | Repeated successful tool sequences | Long, versioned, requires use-feedback |

### Why four, not three

Psychology's three (semantic / episodic / procedural) is the
classics. The **lessons-learned** type is agent-specific — it
captures the "don't try X, it failed because Y" knowledge
that no other type covers. Without it, an agent re-tries
failed approaches every session.

### Lifetime rules — each type has a different "death"

- **Semantic** — never auto-expires; **updated** when
  contradicted. "User moved from Mac to Linux" → new fact
  supersedes old fact, but old fact stays in history.
- **Episodic** — auto-compressed by `dream` distillation.
  The full session log lives in the ledger; the memory entry
  is just the **index**, not the body.
- **Lessons** — **never** auto-expires; expires only when
  explicitly falsified ("later Y was fixed, this lesson
  no longer applies"). Negative knowledge is the most
  valuable — don't throw it away.
- **Procedural** — versioned; expires when the underlying
  tool chain changes (use-feedback: "tried this playbook,
  it failed because tool X is gone").

### Memory distillation pyramid — the four types stack

The four types are not peers; they form a **distillation
tower**:

```
                     playbooks (rare, compounding)
                  ↑  repeated successful patterns
                 lessons + facts (few, essence)
                  ↑  dream distillation
                episodes (many, raw material)
                  ↑  each session auto-produces
              session ledger (truth, base of the tower)
```

The base is **cheap** (every session produces one episodic
entry); the top is **expensive** (playbooks are
hand-curated). `dream` is the **reboiler** that turns
episodes into semantic + lessons; after distillation,
episodes are down-weighted (details can fade, conclusions
remain).

This structure directly drives the **storage and recall
strategy**:

- **Top** (playbooks / lessons / facts) — small and hot
  (always-on candidates for recall).
- **Bottom** (episodes) — large and cold (lookup-on-demand).

### Memory vs ledger — the boundary

Memory is **separate** from the ledger:

| | Ledger | Memory |
|---|---|---|
| **Scope** | Session-scoped (one session = one directory, one SQLite db per `ledger.md`) | **Agent-scoped** (one agent across many sessions) |
| **What it carries** | Everything that happened in this session | What **this agent** has learned across sessions |
| **Loss tolerance** | None — append-only, complete | Lower — semantic/lessons/procedural live forever; episodic is distilled away |
| **Discovery** | Sequential — replay the round | Search — query by similarity / time / scope |
| **Schema** | Defined (12 categories) | Less rigid — entries have `kind` + `confidence` + `last_verified` |

Memory borrows the **ledger's global addressing**: each
memory entry carries `source_session` + `source_seq` so the
agent can trace a memory back to the canonical origin in
the ledger. No new addressing scheme.

### ②–⑧ — placeholders

The remaining seven angles are filled in by follow-up
commits, in this order:

- **②** Ownership boundaries — whose memory, vs project
  knowledge (`engine.proj.md` style); draw the line between
  memory and the project knowledge base.
- **③** Write paths — the four write sources (dream
  extraction, explicit "remember this", `exit_summary`,
  session-end flush); the candidate-extract-dedupe-conflict-
  store pipeline; MemΘ's ADD/UPDATE/DELETE/NOOP four-decision
  model as the reference.
- **④** Data model + storage — the memory row shape
  (content / kind / source_session+source_seq / confidence /
  last_verified / scope); `memory.db` as a separate SQLite
  (vs cross-db view over the ledger); the rationale
  (memory crosses sessions, ledger is per-session).
- **⑤** Read paths — recall as one of `derive`'s injectors;
  when (turn start / intent-triggered); ranking (relevance ×
  recency × importance); budget (memory's slice of the
  surface); two-leg retrieval (vector + FTS, as decided
  earlier).
- **⑥** Maintenance policies — ADD / UPDATE / DELETE /
  NOOP per candidate; the **contradiction-resolution**
  pipeline; the **falsification** path for lessons; the
  **versioning** rules for procedures.
- **⑦** Retrieval quality — what "good recall" means in
  practice; metrics; the gap between "stored but unrecalled"
  (memory dead) and "recalled at the right time" (memory
  useful); the test/measurement layer.
- **⑧** External framework survey — Mem0 (fact extraction +
  update decisions), Letta / MemGPT (core-memory + archival),
  Zep (temporal knowledge graph), codex/CC (file-based
  memory). What we borrow, what we don't.

Each is a follow-up commit; the **8 angles** form a
checklist for "memory layer design is done".

### Where memory lives in the existing design

Memory is **per-agent**, **separate** from the per-session
ledger:

- The ledger is session-scoped; memory is agent-scoped.
- Memory entries reference their **source** in the ledger
  via `source_session` + `source_seq` (the same global
  addressing scheme the ledger already uses).
- Memory is **extractable** from the ledger by `dream` (the
  watermark mechanism already designed) and by other write
  paths.

Memory is **read into** the loop surface via `derive`'s
injection slots — the same slots `harness.feature` and the
reminder channel already use. Recall is **one** injector
among several; it has its own kind, its own budget, and
its own retention policy. Detailed design lives in angle
⑤; this section only anchors it.

## Cross-references

- `loop.md` "Two protocols" — recall is one of the
  injection paths into the surface; storage is the
  ledger-side counterpart.
- `loop.md` "Convergence audit" — memory is the **next
  continent** after `turn_loop` closes.
- `ledger.md` "Entry Kinds — ten categories" — the ledger
  already has `uid` + `ref.session` for global addressing;
  memory borrows that scheme (`source_session` +
  `source_seq`).
- `compression.md` "Layer 3 — Overflow (`overserved_max`)" —
  the `dream` watermark is the same mechanism that drives
  memory extraction; both consume the same
  `memory_watermark` row.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: **draft (possible mechanism) — angle ① settled**.
  Angles ②–⑧ are placeholders; each lands in a follow-up
  commit. The four-type classification and the distillation
  pyramid are the load-bearing parts of angle ①.