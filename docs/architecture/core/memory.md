# Memory Layer — Design

> **Memory is durable cross-session continuity.** Runtime process state,
> presentation surfaces, and caches are disposable; admitted Memory revisions
> and their evidence are not. This is a mixed-maturity design record: M0/M1 and
> the M2 control-plane boundary are normative only through their focused Specs.
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

The 8 angles are **ordered**: each builds on the previous. All eight are
explored here. Exact types, authority, lifecycle, retrieval, maintenance,
quality evidence, and the audit/edit workflow live in M0, M1, and M2 Specs;
unpromoted numeric policies and mechanisms in this document remain research.

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
| **Mutation** | Append-only facts | Append-only revisions; lifecycle may archive, promote, supersede, or erase through receipts |
| **Discovery** | Sequential — replay the round | Search — query by similarity / time / scope |
| **Schema** | Strict versioned fact catalog | Strict record/lifecycle contracts with extensible versioned type policy |

Memory borrows the Ledger's durable addressing. Each evidence item binds exact
Session, position, fact identity, and payload digest, so Runtime can verify the
origin against a fixed prefix. Memory does not invent a parallel truth address.

## ② Authority dual-source + ownership boundaries

### Authority dual-source — preference vs agent memory

Memory entries come from **two fundamentally different
authorities**. Treating them as one thing creates a real
safety bug — see "Write authority + read direction"
below. Each authority gets its own read path:

| Content | Mechanism | Reason |
|---------|-----------|--------|
| **Preferences (`user_declared`)** | **Framework push** — `derive`'s injector, always on every turn | Law must take effect unconditionally; can't rely on agent "remembering to check"; preferences are small (dozens of entries), cheap to push |
| **Memory (`agent_learned`)** | **Agent pull** — `memory_search` / `recall` tool; agent decides when to call | Memory is large and scenario-scoped; pushing all of it blows the context window; pull is the right shape |

**The pull-mode catch**: agent doesn't know what it doesn't
know. Pure tool-call recall fails because if the agent
doesn't think to call the tool, the memory is invisible.
**Fix: framework pushes a small "memory index" every turn**
— a list of *what kinds exist* and *which recent entries
matter*. The agent sees the menu and decides when to pull
the details.

### Write authority + read direction

The split is **strict**: writes and reads go through
different paths with different authority.

#### Write authority

| Who | Can write | Authority |
|-----|-----------|-----------|
| **User** | Explicit declarations / settings / corrections | `user_declared` — law |
| **Agent** | Dream extraction, exit_summary, session-end flush | `agent_learned` — hypothesis |

If the agent **observes** something that looks like a
preference ("user keeps asking for Chinese"), the correct
action is to **suggest**: "want me to set Chinese as the
default?" → user confirms → becomes `user_declared`.
**Observation never upgrades to law on its own.**

#### Read direction

| Content | Direction | Frequency | Budget |
|---------|-----------|-----------|--------|
| Preferences | **Push** (`derive` injector) | Every turn | Tens of entries; cheap |
| Memory index | **Push** (small catalog) | Every turn | ~5 % of surface |
| Memory detail | **Pull** (tool call) | Agent decides | Varies by query |

The "menu injection" makes the memory's existence **always
on the agent's radar** without forcing it onto the agent's
context. The agent pulls the details when relevant. This
hybrid is also the answer to the "pure tool call" failure
mode — if the agent never calls the recall tool, the memory
is still noticed via the menu.

### Authority is not a confidence score

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
authority = organisation_published # externally published, receipt required
```

`user_declared` always wins over `agent_learned` at recall
time. When a user corrects an agent's memory ("no, I don't
use X anymore"), the correction **is itself** a `user_declared`
entry — law overwrites hypothesis, with a complete audit trail.

This is the **provenance philosophy** of `ledger.md`
(per-kind producer field) applied to memory — every
memory entry declares its authority source.

### Namespace isolation + scope classes

Product composition supplies opaque authorized namespaces. Portable Memory
uses the admitted scope classes without exposing tenant or user identifiers:

```
MemoryScopeClass = Session | AgentInstance | User | Project | Platform
```

**Visibility rules at recall time:**

| Layer | Belongs to | Example |
|-------|-----------|----------|
| `agent` layer | One Agent instance | Instance-specific lessons and playbooks |
| `user` layer | The individual user | Personal preferences and reusable lessons |
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
| `kind` | The schema / payload shape | `{situation, action, consequence, evidence[]}` |
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

## ⑤ Data model + storage

M0 owns stable record/revision/query/proposal identities, exact content and
evidence bindings, authority, scope, sensitivity, and revision status. M1 adds
the orthogonal hypothesis lifecycle and integer evidence tallies; portable
state does not use floating confidence as truth or authority.

The production adapter currently persists Memory projections transactionally
with the Runtime SQLite store so Memory facts and visibility obey
commit-before-context and restart recovery. A future physically separate
`memory.db` is an adapter choice, not an accepted domain boundary; it must first
prove atomic coordination and recovery without a cross-database transaction.

## ⑥ Read paths

Recall is **one of `derive`'s injection paths** — the same
slots `harness.feature` and the reminder channel use:

- **When** — turn start (cold-scan episodic), intent-triggered
  (hot-scan semantic + lessons), periodic (procedural
  refresh)
- **Budget** — `memory` share of the surface is bounded by
  `MemoryTypeRegistry.recall_profile.budget`
- **Two-stage retrieval** — bounded menu push followed by explicit detail pull
- **Ranking** — exact deterministic score/tie-break policy plus a replayable
  exploration choice; no implicit vector/FTS dependency is part of M1

## ⑦ Maintenance policies — promotion + anti-bloat

### Promotion channel — memory graduates to knowledge

The four types are not peers; they're a **distillation tower**
(see ③). The top of the tower (`playbooks`) is **hand-curated
from proven memories** — a memory entry graduates to
knowledge when it's been verified enough times.

```
memory entry (with verified durable fact evidence)
    ↓  "verified N times, on stable topic"
    ↓  promoted by dream or by user audit
    ↓
knowledge entry (in `engine.proj.md` / wiki / shared knowledge base)
    ↓
original memory entry downgrades to "promoted_to: <knowledge-id>"
```

**Concrete example:**
- `memory.lesson` — "SDK X has a caching bug" (with an exact verified fact
  reference for Session S42 position 317)
- After N independent sessions reproduce the same lesson →
  dream (or user audit) writes a `wiki:project/sdk-cache`
  entry in the **project knowledge base** (per ② — agent
  only references, doesn't own).
- The original `memory.lesson` entry is **downgraded** to
  `status = promoted_to: <wiki-id>` — kept for audit, but no
  longer surfaces in recall.

**Without this promotion channel**, memory and knowledge
**duplicate the same fact** — two storage locations, each
rotting at its own rate. With it: **memory is the raw-material
library; knowledge is the graduation destination** — a
**single-direction pipeline**, no duplication.

### Anti-bloat — six defenses

The chronic disease of memory systems: every session
produces memory → no discipline → infinite growth → recall
precision drops → inject cost rises → noise drowns signal.
**Memory health = recall precision**, not entry count.

Six defenses:

| # | Defense | What it does | Where it lives |
|---|---------|--------------|----------------|
| 1 | **Distillation (debulk)** | Episodes distilled to conclusions; raw entries down-weighted. The tower structure IS the debulk. | `dream` watermark |
| 2 | **Quota (hard ceiling)** | Explicit per-type count/byte caps force ongoing priority judgement. Numeric values are Runtime policy and require measured admission; they are not defaults in this design. | `MemoryTypeRegistry` retention policy |
| 3 | **Admission filter (write gate)** | Three questions before entry: **can it generalise?** (no → reject, one-off detail); **is it stable?** (uncertain → defer + observe); **already present?** (dedup). Borrowed from Mem0's NOOP decision. | `dream` candidate → ADD/UPDATE/DELETE/NOOP pipeline |
| 4 | **Use feedback (natural selection)** | Recalled and helped → boost score. Never recalled → slow decay. Recalled but linked to failure → downrank. Use it or lose it; entries earn their place. | Recalled entry's `confidence` adjusts based on outcome |
| 5 | **Memory lint (periodic audit)** | Scheduled task: find duplicates, find contradictions (two memory entries conflict — pick one), find expired (`last_verified` over threshold → flag), find low-score. Output an **audit report** — the user is the ultimate curator. | Scheduled cron job + user-visible report |
| 6 | **Forgetting right (first-class)** | Delete and `redaction` are equivalent first-class operations. "Forget this" must be legal — both for hygiene (low-quality memory) and privacy (user's right to be forgotten). | `memory.delete(entry_id)` |

**The total principle**: four gates, each at a different stage
of the memory lifecycle:

```
入口收紧     (admission filter, write gate)
内部压缩     (distillation + quota)
出口淘汰     (use feedback + memory lint)
用户兜底     (audit right + forgetting right)
```

Bloat is intercepted at **every** of the four gates, with exact decisions and
bounds owned by M1 rather than prose defaults here.

## ⑧ Retrieval quality

"Stored but unrecalled = dead". "Recalled at the right time =
useful". M1 pins a deterministic synthetic regression for menu/detail recall,
exposure, evidence and scope behavior. Representative empirical quality,
production thresholds, and a knowledge-graph comparison remain external
evidence gates; synthetic conformance is not a product-quality claim.

## Landscape — three swimlanes + one extraction channel

The memory layer touches **three swimlanes** that flow in
parallel. Only **three coupling points** exist between them;
the rest is fully decoupled.

```
┌─────────────────────┐    ┌──────────────────────┐    ┌──────────────────────┐
│  Conversation       │    │  Effect layer        │    │  Observation lane    │
│  (turn loop)        │    │  (governance +       │    │  (OutcomeObserver)   │
│                     │    │   dispatcher)        │    │                      │
│  user.msg           │    │                      │    │                      │
│       ↓             │    │                      │    │                      │
│  derive             │    │                      │    │                      │
│   query = msg + goal │    │                      │    │                      │
│       ↓             │    │                      │    │                      │
│  Memory recall      │    │                      │    │                      │
│   top-K (4-way + RRF │    │                      │    │                      │
│   + gate + Thompson │    │                      │    │                      │
│   sampling)         │    │                      │    │                      │
│       ↓             │    │                      │    │                      │
│  surface            │    │                      │    │                      │
│       ↓             │    │                      │    │                      │
│  model.invoke       │    │                      │    │                      │
│       ↓             │    │                      │    │                      │
│  response           │────► governance.judge    │    │  app-signal ①        │
│  (references        │     ↓                  │    │  (model cited         │
│   [mem:xxx])        │     dispatcher         │    │   [mem:abc])          │
│       ↓             │     ↓                  │    │       ↓               │
│  ledger append      │     tool execution    │    │  obligation ticket    │
│  (reply / verdict / │     → result          │    │  (check against       │
│   tool.result)      │                        │    │   real events)        │
└─────────────────────┘    └──────────────────────┘    └──────────┬───────────┘
                                                                ↓
                                                          verify / falsify /
                                                          neutral
                                                                ↓
                                                        Beta (α, β) → conf
                                                        recompute → state
                                                        machine transition
                                                          ↓
                                                        usage record
                                                        (weekly regression
                                                         calibration)

┌─────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
│  Memory bank       │  │  Extraction channel   │  │  (same lanes above)   │
│                    │  │                       │  │                        │
│  active (hot —     │  │  4 triggers:          │  │                        │
│   injects)         │  │  ① session-end        │  │                        │
│  candidate (verif.  │  │    → extractor       │  │                        │
│   bounty)          │  │  ② exit_summary       │  │                        │
│  cold (searchable)  │  │    → hot capture     │  │                        │
│  archived (query    │  │  ③ user "记住 X"     │  │                        │
│   only)             │  │    → direct write     │  │                        │
│  (lessons exempt    │  │  ④ dream (hourly)    │  │                        │
│   from decay)       │  │    → episode distill  │  │                        │
└─────────────────────┘  └──────────────────────┘  └──────────────────────┘
```

### Three coupling points — only three

| # | From | To | What flows |
|---|------|----|-------------|
| **①** | Conversation (turn loop) | Memory recall | `top-K` entries injected into `surface` (memory pushes its index every turn; agent pulls details on demand) |
| **②** | Conversation + Effect layer | Observation lane | Application signal — `model cited [mem:abc]` and `tool.result`/`verdict` |
| **③** | Effect layer | Observation lane | Real-world results — `tool.result`/`error`/`test pass/fail` |

**Everything else is decoupled**. The observation lane and
the extraction channel are **async side-branches**; the
conversation's hot path has **only one sync action** — the
look-up at recall time, in milliseconds. **Updates,
distillation, decisions all run side-channel**. The
conversation never waits for memory.

### Walkthrough — a lesson's full life (T0–T6)

| When | Lane | What happens |
|------|------|--------------|
| **T0** | Conversation + Extraction | Method X fails during a session. `exit_summary` fires → extraction-channel **hot-captures** the lesson → enters `memory.lesson` as **candidate** (evidence + scope). Cheap verification runs in-line (e.g. error-signature reproducibility check). Unverifiable → stay candidate. |
| **T1** | Conversation + Memory | Next session, similar task starts. `derive` recall fires → **Thompson sampling** lets the new lesson surface even with a low prior → injected into surface as `[mem:abc] pending-verification lesson: X fails under C due to Y`. Model switches to method Z, **cites `[mem:abc]`** to declare the avoidance. |
| **T2** | Effect + Observation | Method Z **succeeds** → obligation ticket opens: "if X avoided Y, did Z succeed?" Event arrives → **level-1 deterministic verdict** (avoidance succeeded + substitute succeeded) → `α+1` → `conf ↑` → `candidate` → **`active`**. First full use closes the loop. |
| **T3** | Conversation + Memory | A later **high-risk** action → `governance.judge` triggers an **informed-approval** flow → recall **the same lesson** into the approval context. User sees "you've avoided this before; confirm again". |
| **T4** | Effect + Observation + Memory | The lesson applied in a new context, **fails** → attribution-loop check: was the failure in-scope? If **out of scope**, **don't falsify** — narrow scope to "applies to C1, not C2". The lesson becomes **more precise** (CBR theory). |
| **T5** | Extraction + Memory | `dream` batch runs at hour boundary. Episodes distilled → lessons cited as distillation evidence → episodes down-weighted. Lessons stay evergreen. |
| **T6** | Memory | Weekly **calibration loop**: "of memories with `conf = 0.8`, what was the actual success rate?" → regression re-calibrates the confidence mapping. |

## Our four moats — what makes this hard to copy

The memory layer is **not** a better kind-registry or recall
algorithm. Those parts are similar to existing systems. The
moat is the **feedback signal source** — what's unique to
Garive is *where the updates come from*.

| # | Moat | Why it's hard to copy |
|---|-------|------------------------|
| **1** | **Real-world reconciliation loop** — the **OutcomeObserver** captures *what actually happened in the world* (tool results, test pass/fail, error signatures) and feeds `β` to `dream` / promotion. Everyone else's memory updates from **dialogue** ("user said X"). Ours updates from **reality**. The confidence machine only works because reality is the input. |
| **2** | **Lessons pipeline** — `exit_summary` + **hot-capture** + **risk-action recall** + **informed-approval**. ProgressGuardian + memory = closed loop. Other systems' memories have no place for "this failed". |
| **3** | **Scope attribution (CBR-style narrowing)** — failure in scope → falsify, failure out of scope → narrow scope. The lesson gets **more precise** with use, not stale. Few product implementations. |
| **4** | **Explore-exploit math for recall** — Thompson sampling + extract-time verification + candidate-bounty. Recall is not just similarity top-K; it's a **bandit problem** with cold-start + exposure-bias corrections. |

## Honest gaps — where others are stronger

| Gap | Who's stronger | Garive's stance |
|-----|----------------|-----------------|
| **Knowledge-graph structure** (multi-hop relational inference) | Zep | **Acknowledged** — entries + vectors; relational inference is weak. v2 candidate, not in personal-agent scope yet. |
| **Memory editability / transparency** (`CLAUDE.md`-style) | codex / CC | M2 is accepted and active: bounded Markdown snapshot, explicit dry-run plan, authority-safe atomic import, and erasure receipts. Product UI evidence remains open. |
| **Representative product evidence** | mature memory products | Core semantics and synthetic quality gates exist; representative longitudinal recall quality and the M2 user workflow remain open evidence. |

## Positioning conclusion

The industry splits memory into three camps:

| Camp | Tool | What memory is |
|------|------|------------------|
| **File-based** | codex / CC | A user-maintained document. Transparent but no intelligence. |
| **Search-based** | grok | A history of past sessions. Has retrieval, no distillation. |
| **Product-based** | Mem0 / Letta | A library of dialog-extracted entries. Has distillation, no verification. |

Garive's memory is **none of these**:

> **Memory is a hypothesis library tested by reality.**
>
> Distillation + verification + attribution + explore-exploit.
> The differentiation is **not** storage or recall — those
> parts are roughly the same as everyone else's. It's the
> **feedback-signal source**: everyone else updates from
> dialogue; we update from **reality**. That's the part
> hardest to copy, because it requires the ledger +
> effect-layer + governance combination — none of which
> any memory system today has.

## Cross-references

- `loop.md` "Two protocols" — recall is one of the
  injection paths into the surface; storage is the
  ledger-side counterpart.
- `loop.md` "ProgressGuardian" — feeds the failures
  pipeline; without it, the memory is shallow by definition.
- `loop.md` "Convergence audit" — memory is the **next
  continent** after `turn_loop` closes.
- `ledger.md` — Memory evidence binds exact durable fact references and digests.
- `compression.md` "Layer 3 — Overflow" — `dream`
  watermark is the same mechanism that drives memory
  extraction; both consume the `memory_watermark` row.
- `../../../spec/design/memory-capability.md` — accepted M0 records and authority.
- `../../../spec/design/memory-hypothesis-lifecycle.md` — accepted M1 lifecycle.
- `../../../spec/design/memory-control-plane.md` — accepted M2 audit/edit workflow.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: **mixed maturity** — M0/M1 are implemented and verified; M2 is
  accepted and active. Knowledge-graph structure, representative longitudinal
  quality, and unpromoted numeric/mechanism proposals remain research.
