# Memory Layer — Design

> **Memory is the physical existence of `self`.** Runtime
> is disposable (process / surface / cache are derivatives);
> memory is not. This is the principle already decided at
> the top of the design (per the earlier conversation).
>
> The memory layer is the **next continent** after
> `turn_loop` closes. This document lays out the design
> across **eight angles**, then deep-dives them in order.
> The "kind philosophy" recurs three times (kind registry in
> `ledger.md`, EventCatalog in `loop.md`, MemoryTypeRegistry
> here) — same governance pattern, different vocabulary.

## The 8 angles (overview + order)

| # | Angle | One-line |
|---|-------|----------|
| ① | **Location + classification** | What memory is, the 4 types |
| ② | **Authority dual-source + ownership boundaries** | Whose memory, vs project knowledge; user vs agent authority |
| ③ | **Distillation + write paths** | How memory is produced; the pyramid; the type registry |
| ④ | **External framework survey** | Mem0 / Letta / Zep / codex CC lessons |
| ⑤ | **Data model + storage** | Row shape + `memory.db` separate SQLite |
| ⑥ | **Read paths** | Recall into the surface |
| ⑦ | **Maintenance policies** | ADD/UPDATE/DELETE/NOOP decisions |
| ⑧ | **Retrieval quality** | Recency × relevance × importance ranking |

The 8 angles are **ordered**: each builds on the previous.
This document deep-dives ① through ③; ④–⑧ are placeholders
for follow-up commits.

## ① Location + classification — what memory IS

Memory is **the agent's cross-session continuity itself**.
It is not "data the agent stores" — it is the substrate that
lets "the next session pick up where the last left off".
Runtime (process, surface, cache) is **disposable**; memory
is **persistent**.

### Four types — borrowed from cognitive science

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
- **Episodic** — auto-compressed by `dream` distillation
  (see angle ③).
- **Lessons** — **never** auto-expires; expires only when
  explicitly falsified ("later Y was fixed, this lesson
  no longer applies"). Negative knowledge is the most
  valuable — don't throw it away.
- **Procedural** — versioned; expires when the underlying
  tool chain changes (use-feedback: "tried this playbook,
  it failed because tool X is gone").

### Memory vs ledger — the boundary

Memory is **separate** from the ledger:

| | Ledger | Memory |
|---|---|---|
| **Scope** | Session-scoped | **Agent-scoped** (cross-session) |
| **What it carries** | Everything that happened in this session | What **this agent** has learned across sessions |
| **Loss tolerance** | None — append-only | Lower — semantic/lessons/procedural live forever; episodic is distilled away |
| **Discovery** | Sequential — replay the round | Search — query by similarity / time / scope |
| **Schema** | Strict (12 categories) | Looser — entries have `kind` + `confidence` + `last_verified` |

Memory borrows the **ledger's global addressing**: each
memory entry carries `source_session` + `source_seq` so the
agent can trace a memory back to the canonical origin in
the ledger. No new addressing scheme.

## ② Authority dual-source + ownership boundaries

### Authority dual-source — preference vs agent memory

Memory entries come from **two fundamentally different
authorities**. Treating them as one thing creates a real
safety bug:

| | User preference / statement | Agent memory (learned) |
|---|---|---|
| **Source** | User declares it | Agent learned it (may be wrong) |
| **Nature** | **Law** — user decides, agent cannot override | **Hypothesis** — agent derived it, may be falsified |
| **Who can change** | User only | Agent updates; user can veto |
| **Trust** | Unconditional | Must be verifiable + correctable |

> **Mixing them is a real safety bug**: an agent that learned
> something wrong, but stored it as "user preference", would
> treat a hypothesis as law. The hypothesis impersonates
> the law.

The fix: **one storage / recall infrastructure, two authority
tags**:

```yaml
authority = user_declared    # user law (preferences, rules, corrections)
authority = agent_learned    # agent hypothesis (lessons, inferred facts, playbooks)
authority = org_shared       # org-shared (team knowledge, platform)
```

`user_declared` always wins over `agent_learned` at recall
time. When a user corrects an agent's memory ("no, I don't
use X anymore"), the correction **is itself** a `user_declared`
entry — law overwrites hypothesis, with a complete audit trail.

This is the **provenance philosophy** of `ledger.md`
(per-kind producer field) applied to memory — every
memory entry declares its authority source.

### Platform type — namespace isolation + three-tier scope

Personal type has one self; **platform type** must slice by
tenant:

```
MemoryEntry.scope = {tenant, user, project}
```

**Visibility rules at recall time:**

| Layer | Belongs to | Example |
|-------|-----------|----------|
| `user` layer | The individual user | Personal preferences, personal lessons, personal playbooks |
| `project` layer | The project (agent only references) | Project knowledge, project conventions |
| `platform` layer | The platform (requires authorised aggregation) | Anonymised cross-user patterns |

- **`user` entries** — visible **only** to that user. A's
  lessons never enter B's context. Hard isolation.
- **`project` entries** — visible to project members. Belongs
  to the project, not a person (per the earlier principle
  "project knowledge → project, not person").
- **`platform` entries** — visible only after authorised
  aggregation / anonymisation. Without that, cross-user
  access is a privacy incident.

> **Isolation is the recall layer's hard constraint** (scope
> filter forced at the recall entry point), not a discipline.
> Privacy is enforced by mechanism, not by good intentions.
> Same nature as `ledger.md` "privacy.redact".

## ③ Distillation + write paths

### Storage strategy — the distillation pyramid

The four types are not peers; they form a **distillation
tower**:

```
                     playbooks (rare, compounding)
                  ↑  repeated successful patterns → promoted to playbook
                 lessons + facts (few, essence)
                  ↑  dream distillation
                episodes (many, raw material)
                  ↑  each session auto-produces an episode entry
              session ledger (truth, base of the memory tower)
```

The base is **cheap** (every session produces one episodic
entry); the top is **expensive** (playbooks are hand-curated).
The `dream` op is the **reboiler**: episodes become semantic +
lessons; after distillation, episodes are down-weighted
(details fade, conclusions remain).

**This structure directly drives storage / recall:**

- **Top** (playbooks / lessons / facts) — small and **hot**
  (always-on candidates for recall).
- **Bottom** (episodes) — large and **cold** (lookup-on-demand).

### MemoryTypeRegistry — the kind philosophy, third application

The same governance pattern that gave us the **kind registry**
(`ledger.md`) and the **EventCatalog** (`loop.md`) gives us the
**MemoryTypeRegistry** here — every memory type is registered,
not extended.

```python
class MemoryType:
    kind:           str         # 'memory.fact' / 'memory.lesson' / 'memory.episode' / 'memory.playbook'
    lifetime:       Lifetime    # retention + decay + retirement rules
    distillation:   list[Transition]  # who can distil into what
    recall_profile: RecallProfile   # default trigger + budget + rank
    surface_kind:   str         # kind used when injected into surface
```

Per-type registration (one row in the table):

| Field | What it declares | Example (`memory.lesson`) |
|-------|------------------|-----------------------------|
| `kind` | The schema / payload shape | `{situation, action, consequence, source_session, source_seq}` |
| `lifetime` | Retention + decay + retirement | "never auto-expires; falsified only by explicit override" |
| `distillation` | Who can distil into what | "episodic → lesson on 3+ failures" |
| `recall_profile` | Trigger / budget / rank | "trigger on task-type match; budget = 5 % of surface" |
| `surface_kind` | How the entry shows up in the surface | `memory.lesson` (pinned, always-loaded) |

> **Adding a new memory type = one table row + one policy.**
> The runtime does not invent types that aren't registered.
> The dispatch loop in angle ⑤ reads the table to decide
> recall behaviour; the persistence layer reads it to apply
> lifetime rules. Code stays stable; new types are config.

This is the **third application** of the kind philosophy:

- `kind` registry (`ledger.md`) — governs **what's in the ledger**.
- `EventCatalog` (`loop.md`) — governs **what's broadcast on the wire**.
- `MemoryTypeRegistry` (here) — governs **what's in memory**.

All three are append-only vocabularies. All three reject
undeclared entries. The runtime **does not invent**.

### Four write sources

Memory is produced by **four sources**:

| Source | Pipeline | Trigger |
|--------|----------|---------|
| **`dream` extraction** | `dream` watermark walks ledger, runs ADD/UPDATE/DELETE/NOOP four-decision per candidate, writes to memory | Background, scheduled |
| **Explicit user statement** | "remember this" → direct insert | When user types it |
| **`exit_summary` deposit** | Round-end → runways / plans → lessons | At every `agent_turn` end |
| **Session-end flush** | Episode index → episodic memory entry | At session end |

The **`dream` pipeline** is the load-bearing one — it does
candidate → extract → dedupe → conflict-resolve → store, with
**four decisions** (ADD / UPDATE / DELETE / NOOP) per candidate.
The four-decision model is borrowed from **Mem0** (see
angle ④).

## ④ External framework survey (what we borrow, what we don't)

| Framework | Core idea | What we borrow | What it lacks |
|-----------|-----------|----------------|----------------|
| **Mem0** | Fact extraction + ADD/UPDATE/DELETE/NOOP four-decision | **The four-decision model** for angle ③ write paths | Only semantic memory; no layers, no distillation |
| **Letta / MemGPT** | `core memory` (always-on context) vs `archival` (retrieval-only) | **The hot/cold split** in angle ⑤: hot layer (always-on candidates), cold layer (lookup-on-demand) | Framework-integration focused; no lessons / procedural types |
| **Zep** | Temporal knowledge graph (facts with validity intervals) | **Time validity modelling**: `last_verified` + invalidation markers in ① lifetime rules | Graph is too heavy for personal-agent use case |
| **codex / CC** | Pure file-based (`memories.instructions/CLAUDE.md`), user hand-writes | **Reference**: fully-explicit, no auto-extraction (the opposite of what we are) | No extraction, no distillation, no recall ranking |

### Our combination

> **Mem0's four-decision pipeline** × **Letta's hot/cold split**
> × **Zep's temporal validity** × **our own lessons pipeline**
> (the bit none of the four have).

The **lessons pipeline** is what's unique: `exit_summary`
is a raw material none of the four frameworks have access to
— their memory systems don't see "this failed", they see
"this happened". ProgressGuardian (per `loop.md`) feeds the
failures to memory; without this pipeline, an agent's memory
is **shallow** by definition.

## ⑤ Data model + storage — placeholder

(Follow-up commit.)

### Shape (preview)

```python
class MemoryEntry:
    id:              uuid        # global identity (uuid v4 / ULID)
    mtype:           MType       # one of the registered kinds
    content:         Value       # mtype-specific payload (JSON / text)
    provenance:      Provenance   # source_session + source_seq + authority
    confidence:      float       # 0..1, decays with last_verified
    last_verified:   int64       # unix ms; when the entry was last checked
    scope:           Scope       # {tenant, user, project}
```

### Storage choice (preview)

**`memory.db` as a separate SQLite**, not a cross-db view over
the ledger. Rationale: memory crosses sessions; the ledger
is per-session. The two are different in lifecycle, query
pattern, and retention policy — forcing them into one db
would conflate concerns.

## ⑥ Read paths — placeholder

(Follow-up commit.)

Recall is **one of `derive`'s injection paths** — the same
slots `harness.feature` and the reminder channel use:

- **When** — turn start (cold-scan episodic), intent-triggered
  (hot-scan semantic + lessons), periodic (procedural
  refresh)
- **Budget** — `memory` share of the surface is bounded by
  `MemoryTypeRegistry.recall_profile.budget`
- **Two-leg retrieval** — vector (semantic) + FTS (lexical),
  fused by re-rank; borrowed from the earlier decide
- **Ranking** — relevance × recency × importance (per ⑦)

## ⑦ Maintenance policies — placeholder

The four-decision model (ADD / UPDATE / DELETE / NOOP) lives
in ③ and lands in angle ⑤'s pipeline. Contradiction resolution,
falsification of lessons, versioning of procedures — all
follow-up commits.

## ⑧ Retrieval quality — placeholder

"Stored but unrecalled = dead". "Recalled at the right time =
useful". The metrics for "right time" are the difference
between a memory layer and a graveyard. Measurement layer
lands as follow-up.

## Cross-references

- `loop.md` "Two protocols" — recall is one of the
  injection paths into the surface; storage is the
  ledger-side counterpart.
- `loop.md` "ProgressGuardian" — feeds the failures
  pipeline; without it, the memory is shallow by definition.
- `loop.md` "Convergence audit" — memory is the **next
  continent** after `turn_loop` closes.
- `ledger.md` "Entry Kinds — ten categories" — memory
  borrows the `uid` + `ref.session` global addressing
  scheme (`source_session` + `source_seq`).
- `compression.md` "Layer 3 — Overflow" — `dream`
  watermark is the same mechanism that drives memory
  extraction; both consume the `memory_watermark` row.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: **draft (possible mechanism)** — angles ①, ②, ③,
  ④ settled; angles ⑤–⑧ are placeholders for follow-up
  commits. The four-type classification, distillation
  pyramid, authority dual-source (law vs hypothesis),
  platform-scope isolation, four-decision write pipeline,
  hot/cold split, and lessons pipeline are the load-bearing
  parts landed this round.