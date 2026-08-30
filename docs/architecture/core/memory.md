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
| ⑤ | **Data model + storage** | Durable facts + rebuildable transactional projection |
| ⑥ | **Read paths** | Recall into the surface |
| ⑦ | **Maintenance policies** | ADD/UPDATE/DELETE/NOOP decisions |
| ⑧ | **Retrieval quality** | Recency × relevance × importance ranking |

The 8 angles are **ordered**: each builds on the previous. All eight are
explored here. Exact types, authority, lifecycle, retrieval, maintenance,
quality evidence, and the audit/edit workflow live in M0, M1, and M2 Specs;
unpromoted numeric policies and mechanisms in this document remain research.

### Normative mapping for the deep-dive vocabulary

| Design phrase | Accepted contract | Status |
|---|---|---|
| Four write sources | M1 `MemoryCandidateSource` | Implemented as bounded proposals; no source writes trusted Memory directly. |
| Confidence | M1 `EvidenceTally` plus a versioned display calibration | Floating or composite confidence is not portable truth, authority, or a write permission. |
| Candidate / active / cold / archived / graduated | M1 `HypothesisState`; graduated maps to `Promoted` | Implemented. Superseded/tombstoned remain orthogonal M0 revision states. |
| Vector / FTS / recency | Versioned Runtime candidate ports | Admitted as replaceable ports; no fusion formula or backend is selected by M1. |
| Menu / detail push-pull | M1 committed recall products plus C2 derive | Implemented under shared item and UTF-8 budgets. |
| Reality feedback | M1 obligation and observation facts | Implemented; model citations alone are never verification. |
| `recall.event` / `recall.apply` / `recall.outcome` | `memory.recall_recorded` / `memory.obligation_opened` / `memory.observation_recorded` | Implemented as one attributable durable chain; these prose aliases are not additional stores or fact kinds. |
| Success / failure / censored | M1 `Verified` / in-scope `Falsified` / no conclusive observation | Portable truth remains `EvidenceTally {verified, falsified, neutral}`. Greek-letter counters are explanatory notation only. |
| Memory error / context mismatch | in-scope falsification / out-of-scope neutral observation plus optional narrowing Candidate | Implemented. A mismatch never edits scope in place or counts as falsification. |
| Three swimlanes / feedback loops | Runtime orchestration over committed facts and rebuildable projections | Conceptual view only; it adds no side channel, schedule, database, or direct write authority. |
| Risk-action lesson recall | Future Governance × Memory contract | Research until an exact purpose, authority, redaction, durable fact, and `AskUser` integration Spec is accepted. |
| Numeric schedules, percentages, thresholds, decay and fusion weights | Versioned measured policy | Research until reproducible evaluation admits exact values. |

### Maturity rule for formulas and diagrams

The formulas, framework comparisons, timing examples and diagrams below are a
research notebook. They describe hypotheses worth evaluating, not defaults.
In particular, Beta priors, RRF constants, query-expansion counts, top-K values,
similarity thresholds, freshness equations, ranker calibration, hourly/daily/
weekly schedules and a physically separate `memory.db` are **not accepted
configuration**. A production implementation must name a versioned policy,
freeze its inputs in the effective D0 snapshot, commit its result before use,
and pass a reproducible quality/privacy/recovery evaluation. Where an example
conflicts with M0/M1/M2, the focused Spec wins.

## ① Location + classification — what memory IS

Memory is **the agent's cross-session continuity itself**.
It is not "data the agent stores" — it is the substrate that
lets "the next session pick up where the last left off".
Runtime (process, surface, cache) is **disposable**; memory
is **persistent**.

### Five shapes of knowledge — what storage is for

Memory is not one thing. The system admits **five shapes
of knowledge**, each with its own storage substrate and
recall mechanism. The Memory bank is the *integration
layer* over all five; no single substrate covers all five.

| Shape | What it captures | Where it lives | How it's recalled |
|-------|-------------------|-----------------|-------------------|
| **Tacit intuition** | "This is what usually works" — the implicit pattern the LLM / human carries but cannot fully articulate | **LLM weights + human brain** | **Never in the system** — by definition, tacit knowledge is what cannot be externalised. Memory works around it; it does not capture it. |
| **Similarity knowledge** | "This looks like that" — semantic / topical neighbours | **Vector index** + FTS | Hybrid recall — `B = α_v · vector_score + α_t · fts_score`, RRF fusion (⑥) |
| **Structural knowledge** | "A caused B, which contradicts C, which distilled from D" | **Memory graph** (typed links in `MemoryTypeRegistry`) | One-hop / two-hop traversal under the recall profile (⑤ Memory graph) |
| **Normative knowledge** | "This is the rule / contract / schema / policy" | **Schemas + Specs + contracts + configs** | Versioned references — bound by exact digest, not by similarity |
| **Episodic / raw** | "This is what actually happened, in what order, with what evidence" | **Ledger** (durable facts) | Source-of-truth replay — `source_session + source_seq`, no transformation in the recall chain |

The five are **not peers**:

- **Tacit** is the limit of the system — the goal is to
  externalise enough of it that the system does not have
  to guess, not to capture it.
- **Similarity** is for **fuzzy retrieval** — find entries
  that *look like* what the query is about.
- **Structural** is for **causal traversal** — find
  entries that *connect to* what the query is about.
- **Normative** is for **constraint enforcement** —
  answers "what is allowed?", not "what is similar?".
- **Episodic** is for **ground-truth replay** — the
  ledger is the raw material the other four are built
  from, and the audit trail they all trace back to.

#### Where Memory sits in the five

Memory is the **integration layer** over three of the
five shapes, the **adjacent reference** to a fourth, and
the **explicit non-capturer** of the fifth:

| Shape | Memory's relation | Why |
|-------|-------------------|-----|
| Similarity | **Native** — vector + FTS index, ⑥ hybrid recall | Primary fuzzy retrieval mechanism |
| Structural | **Native** — typed link fields, one-hop / two-hop traversal | Causal / lineage reasoning |
| Normative | **Adjacent** — entries reference norms by exact digest, never interpret them | Constraints are contracts, not recall candidates |
| Tacit | **Out of scope** — by definition, cannot be externalised | The system works around it; the LLM weights carry what cannot be written down |

Memory entries are therefore built from **similarity +
structural + episodic**, with **normative as a digest
reference** and **tacit excluded**. This is the design
boundary — it is not "we cannot add normative / tacit"
but "they belong to different substrates with different
contracts".

#### Why this matters for recall design

Each shape demands a different recall mechanism; the
Memory bank is what unifies them under one query surface:

- **Similarity recall** needs Thompson exploration,
  conf gating and the exploration slot (⑥).
- **Structural recall** needs one-hop / two-hop budget,
  typed-edge vocabulary and edge-versioned re-binding
  (⑤ Graph storage strategy).
- **Episodic recall** needs the exact `source_session +
  source_seq` reference to bind the entry to its durable
  fact; no transformation, no summarisation in the
  recall chain (⑤ Entry shape).
- **Normative recall** is **not** a recall problem at all
  — it is a contract binding. The agent asks "what is the
  current version of policy X?" and the system answers
  with the exact digest; no similarity, no traversal.

> **Memory is one bank with five shapes behind it.**
> The system's power is in the **integration**, not in
> any single shape. A bank with only vectors is
> similarity-blind to causes; a bank with only graphs is
> similarity-blind to paraphrase; a bank with only the
> ledger is similarity-blind to memory itself. The five
> shapes are why memory is hard, and why the integration
> is worth doing.

### Four types — borrowed from cognitive science

| Type | Content | Primary source | Lifetime |
|------|---------|-----------------|----------|
| **语义 / 偏好·事实 (semantic)** | User's stable facts and preferences: "use SDKMAN for Java", "reply in Chinese" | `dream` extraction, explicit user statement | Long, updated on contradiction |
| **情景 (episodic)** | Which session did what — a light-weight index, not full text | Session-end candidate source | Medium, down-weighted after admitted distillation |
| **教训 (lessons)** | "This path didn't work because X" | `exit_summary`, user correction | Long (negative knowledge is the most valuable) |
| **程序 (procedural / playbook)** | Reusable workflow: "diagnose cache misses in 5 steps" | Repeated successful tool sequences | Long, versioned, requires use-feedback |

### Why four, not three

Psychology's three (semantic / episodic / procedural) is the
classics. The **lessons-learned** type is agent-specific — it
captures the "don't try X, it failed because Y" knowledge
that no other type covers. Without it, an agent re-tries
failed approaches every session.

### Lifetime rules — each type has a different "death"

- **Semantic** — changes through an explicit versioned policy. When
  contradicted, a new immutable revision may supersede the old one while
  preserving its history; no extractor silently edits it in place.
- **Episodic** — admitted scheduled distillation may propose condensed
  candidates and an explicit maintenance policy may later cool/archive the
  episode; distillation itself does not erase it.
- **Lessons** — have no time-only tombstone. Falsification updates exact
  evidence and an explicit policy may change lifecycle; user forget,
  supersession, corruption handling, and legal erasure still apply.
- **Procedural** — binds a toolchain revision. A toolchain change makes the
  procedure eligible for an explicit fallback to Candidate/Cold; it is not
  silently expired.

### Memory vs ledger — the boundary

Memory is a distinct semantic/projection boundary, while Ledger facts remain
the durability SSOT for admitted changes:

| | Ledger | Memory |
|---|---|---|
| **Scope** | Session truth prefix | Authorized cross-session namespace and scope class |
| **What it carries** | Everything that happened in this session | What **this agent** has learned across sessions |
| **Mutation** | Append-only facts | Immutable revisions plus receipted lifecycle, supersession, tombstone, and erasure facts |
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
| **Preferences (`user_declared`)** | Bounded mandatory candidate when admitted by the exact snapshot | User authority is preserved without bypassing C2 hierarchy or budgets. |
| **Memory (`agent_learned`)** | **Agent pull** — `memory_search` / `recall` tool; agent decides when to call | Memory is large and scenario-scoped; pushing all of it blows the context window; pull is the right shape |

**The pull-mode catch**: agent doesn't know what it doesn't
know. Pure tool-call recall fails because if the agent
doesn't think to call the tool, the memory is invisible.
**Fix: a committed bounded menu may enter C2** — a list of *what kinds exist*
and *which admitted entries matter*. The agent sees the retained menu and
decides when to request details; dropped references remain auditable.

### Write authority + read direction

The split is **strict**: writes and reads go through
different paths with different authority.

#### Write authority

| Who | Can write | Authority |
|-----|-----------|-----------|
| **User** | Authenticated explicit declarations / settings / corrections | `user_declared` — law, after the Runtime receipt |
| **Agent** | Distillation, exit-summary, and session-end proposals | `agent_learned` — hypothesis Candidate |

If the agent **observes** something that looks like a
preference ("user keeps asking for Chinese"), the correct
action is to **suggest**: "want me to set Chinese as the
default?" → user confirms → becomes `user_declared`.
**Observation never upgrades to law on its own.**

#### Read direction

| Content | Direction | Frequency | Budget |
|---------|-----------|-----------|--------|
| Preferences | **Push candidate** (`derive`) | Per admitted snapshot | Explicit item/byte bounds |
| Memory index | **Push candidate** (small catalog) | At most one per Kernel iteration | Shared C2 item/byte bounds |
| Memory detail | **Pull** (tool call) | Agent decides | Varies by query |

When C2 retains a menu candidate, Memory becomes discoverable without forcing
full content onto the surface. The agent can pull details when relevant. This
hybrid addresses the pure-tool-call failure mode while preserving one shared
selection hierarchy and budget.

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

`user_declared` and `agent_learned` never collapse into one truth score. When a
user corrects an agent hypothesis, Runtime must authenticate the correction
before creating `user_declared` authority. Conflict remains explicit and Core
derive preserves system/policy/current-input hierarchy with a complete audit
trail.

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
                  ↑  session-end may propose a bounded episode candidate
              session ledger (truth, base of the memory tower)
```

The base is high-volume (a session may produce an episodic candidate or Noop);
the top is expensive (Knowledge publication is separately reviewed/receipted).
The `dream` op is the **reboiler**: episodes become semantic +
lessons; after distillation, episodes are down-weighted
(details fade, conclusions remain).

**This structure directly drives storage / recall:**

- **Top** (playbooks / lessons / facts) — small and **hot**
  (eligible for bounded recall under the exact policy).
- **Bottom** (episodes) — large and **cold** (lookup-on-demand).

### MemoryTypeRegistry — the kind philosophy, third application

The same governance pattern that gave us the **kind registry**
(`ledger.md`) and the **EventCatalog** (`loop.md`) gives us the
**MemoryTypeRegistry** here — every memory type and admitted policy revision is
registered; arbitrary strings are rejected.

```text
MemoryTypeDescriptor {
  type, allowed_roles, admitted_authorities,
  lifecycle_policy_revision, recall_profile_revision,
  retention_policy_revision, surface_kind
}
```

Per-type registration (one row in the table):

| Field | What it declares | Example (`memory.lesson`) |
|-------|------------------|-----------------------------|
| `type` | Closed portable classification | `Lesson` |
| `allowed_roles` | Admitted M0 content roles | versioned exact set |
| `admitted_authorities` | Authorities legal for this type | versioned exact set |
| policy revisions | Lifecycle, recall, and retention implementations | immutable admitted identifiers |
| `surface_kind` | How an admitted entry is typed for C2 | budgeted/selectable lesson envelope |

Adding a memory type requires a versioned enum, descriptor, policies, and
fixtures. A registry row selects admitted code; it cannot inject code or let
Runtime invent an undeclared type.

This is the **third application** of the kind philosophy:

- `kind` registry (`ledger.md`) — governs **what's in the ledger**.
- `EventCatalog` (`loop.md`) — governs **what's broadcast on the wire**.
- `MemoryTypeRegistry` (here) — governs **what's in memory**.

All three are append-only vocabularies. All three reject
undeclared entries. The runtime **does not invent**.

### Four write sources — by **value density**

Memory candidates are produced by **four sources**, ordered by how
**expensive** the failure is to lose (high value density
first):

| Source | When | What it processes | Reason |
|--------|------|--------------------|--------|
| **Hot capture** | `exit_summary` fires | **Lessons** — failures, paths-don't-work, runways | Produces a bounded `ExitSummary` candidate promptly; authority and durability gates still apply asynchronously. |
| **Explicit user statement** | User types "remember this" | **User declaration** — preferences, rules, corrections | Produces an authorized `ExplicitUserCommand`; it still passes retention, sensitivity, and revision checks. |
| **Session-end light extraction** | Session end | **Episode index + salient facts** | Produces a bounded `SessionEnd` candidate or an explicit Noop; a session is not guaranteed to create a record. |
| **dream deep distillation** | Explicit Runtime schedule | Episode → facts / lessons batch distillation | Produces `ScheduledDistillation` candidates over an exact prefix and watermark. Time/session thresholds have no accepted default. |

> **Principle: high-value-low-volume hot-captured;
> low-value-high-volume batch-distilled.**

### Who extracts — accuracy + confidence

An extractor may use a neutral model role resolved by the immutable Agent
Definition snapshot. The role, capabilities, model target, bounds, and
extractor revision are explicit configuration; there is no built-in cheap or
vendor-specific default. Extraction produces schema-constrained candidates,
not trusted Memory revisions.

**Three pillars of accuracy:**

1. **Evidence-mandatory** (anti-hallucination core):
   Every automatic extraction carries ordered durable fact references inside
   one authorized Session prefix. Extractions **without an anchor are rejected**.
   Same philosophy as `governance.judge`'s evidence and
   `compaction.summary`'s structured fields: no anchor, no
   entry.

2. **Evidence grading**: portable state retains exact verified, falsified, and
   neutral tallies plus durable evidence. A versioned calibration may derive a
   display score. It cannot turn correlation, repetition, or model text into
   authority.

3. **Candidate-period regime** — new entries don't take
   effect as ordinary recall immediately. Agent-learned entries enter as
   **Candidate** and are eligible only when the exact recall request admits
   Candidate exploration. A committed verified observation may activate them;
   user confirmation remains a Runtime-authenticated observation, not model text.

> **"Extract wide, trust slow."** The admission gate is
> open (four-triggers cover most signals); the trust gate is
> strict (real-world use must validate).

**Correlation is not solved at extraction time** — extraction
catches **"remember"**, recall catches **"remembered"**.
The write-side admission check uses only the three
questions (does it generalise? is it stable? is it already
present?) to filter obvious irrelevance.

### First-use guarantee — three ways a new entry gets observed

A new memory entry starts life with **no observations** and
therefore no calibrated posterior. Without intervention it
would never enter the top-K — Thompson sampling widens the
posterior but doesn't **force** an observation. The
extraction pipeline binds **three layered guarantees** so
a new entry has at least one chance to produce an
observation before being demoted.

#### 1. Extract-time verification

The bounded extractor runs an internal verification step
**before** emitting a Candidate. An entry that fails the
verification is rejected at the extractor and never
reaches the candidate queue; an entry that survives is
"smell-checked" but **not** truth-attested. The
verification is a cheap plausibility filter, not a reality
check — it catches obvious falsehoods without claiming the
survivor is true.

#### 2. Thompson sampling at rerank

Once admitted as a Candidate, the entry enters the normal
rerank pool. Its wide posterior makes `p ~ Beta(α, β)`
volatile enough to occasionally win a top-K slot.
**No new mechanism needed** — exploration is automatic
through the same sampler that drives ⑥'s rerank stage.

#### 3. Candidate bonus + transparent injection

When a Candidate entry wins a top-K slot, the inject
stage applies a **bounded presentation bonus** and the
surface carries an explicit `first-use` /
`awaiting-verification` label. The model sees the entry
is **tentative** — a fresh hypothesis that needs
world-checking, not a certified fact. The bonus is a
surface-level marker (e.g. `mtype = memory.candidate`),
**not a ranking weight** — the entry's position in top-K
is still governed by Thompson + relevance.

The three guarantees are **layered**, not exclusive:

- Extractor filter cuts the obviously-wrong candidates.
- Thompson sampling surfaces the plausible-but-uncertain
  ones.
- The candidate label makes the uncertainty visible so the
  model can decide whether to verify.

> **"Extract wide, trust slow"** also means **"make the
> first use observable"**. A new entry that never gets
> recalled can never produce an outcome, and an entry
> that never produces an outcome can never earn trust —
> the three guarantees are how the trust gate ever opens.

### Item state machine — promotion / demotion / archival

```text
Candidate --Verified--> Active --Cool--> Cold --Archive--> Archived
                         |             |
                         +-------------+--Promote(receipt)--> Promoted

M0 revision state independently becomes Superseded or Tombstoned.
```

| Transition | Trigger | Rule |
|------------|---------|-------|
| Candidate → Active | Committed `Verified` observation | Ordinary recall eligibility; `AgentLearned` remains a falsifiable hypothesis. |
| Active → Cold | Explicit `Cool` maintenance event | Down-ranked but still searchable. No time threshold is built in. |
| Cold → Archived | Explicit `Archive` maintenance event | Excluded from menu/ordinary recall and available only to admitted detail queries. |
| Active or Cold → Promoted | Exact Knowledge publication receipt | Excluded from normal recall while retaining the publication binding. |
| M0 Superseded | A new immutable revision replaces the active revision | Preserves prior history; this is not an M1 lifecycle state. |
| M0 Tombstoned | Authorized forget/retention/corruption path | Stops retrieval and starts separately receipted physical erasure where required. |

> **Discipline:** every transition leaves a trail
> (when, by what trigger). The memory's own history is
> auditable.



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

## ⑤ Data model + storage — governed registry and lifecycle projections

### MemoryTypeRegistry — kind philosophy's third application

The same governance pattern that gave us the **kind registry**
(`ledger.md`) and the **EventCatalog** (`loop.md`) gives us the
**MemoryTypeRegistry** here — third application of the pattern:

```
kind registry      → ledger kind governance
EventCatalog        → event vocabulary governance
MemoryTypeRegistry  → memory entry type governance
```

All three are versioned closed vocabularies. A registry row selects admitted
code and policy; it cannot inject a class, script, or executable type. Adding a
type requires a versioned enum, compatible policy implementations and shared
fixtures, not only a configuration row.

### Exact descriptor fields

| Field | Question it answers |
|-------|----------------------|
| **Allowed roles / authorities** | Which M0 roles and exact authority bindings this type admits. |
| **Lifecycle / retention policy revisions** | Which implemented, versioned policies govern state and retention. |
| **Recall profile revision** | Which implemented selection profile may produce bounded candidates. |
| **Surface kind** | Which admitted C2 item kind represents the retained product. |

```python
MemoryTypeDescriptor {
  type, allowed_roles, admitted_authorities
  lifecycle_policy_revision, recall_profile_revision
  retention_policy_revision, surface_kind
}
```

The memory type is the handle into this closed registry. Runtime does not invent
types or interpret unknown policy strings as behavior.

### Lifespan rules — semantic level (memory-specific)

The ① angle sets four memory types with different lifecycle intent. Durations,
decay functions and thresholds below are research questions, not defaults:

| Type | Lifetime | Decay | Retirement |
|------|----------|-------|------------|
| `memory.episode` | Medium intent | Distillation may propose derived Candidates; it never silently down-weights the source. | Explicit lifecycle policy only. |
| `memory.fact` | Long intent | Reality-backed observations update the exact tally. | Supersession/user correction remains explicit. |
| `memory.lesson` | Long intent | No time-only tombstone. | Falsification, forget, corruption or explicit policy. |
| `memory.playbook` | Toolchain-bound | Toolchain revision change makes it eligible for Candidate/Cold. | Explicit lifecycle policy only. |

The descriptor selects an implemented policy revision; arbitrary table values
cannot introduce new lifecycle arithmetic.

### Distillation relations — research map

```
memory.episode  ──(dream)──>  memory.fact
memory.episode  ──(dream)──>  memory.lesson
memory.fact     ──(verified)─>  memory.playbook
memory.playbook ──(superseded)>  memory.playbook (newer version)
```

The direction is useful as a candidate-extraction map. Each edge can only emit
an evidence-bound Candidate or Noop through an admitted extractor revision; it
does not activate, merge, supersede, delete or promote a revision.

### Memory graph — entry-level plus relation-level

#### Truth, derivatives, and views — the SSOT hierarchy

The system's **truth** is exactly two layers, in this
order:

```
truth  =  ledger  ∪  memory entries (with evidence anchors)
```

Anything else is a **derivative** — a projection built on
top of truth, used for fast access or for
representation, but **never the source of authority**.
The Memory graph is one such derivative:

```
truth    =  ledger + memory entries (evidence anchors)
              │
              ├──► similarity index  (vector + FTS)         ← view
              ├──► memory graph      (typed links)          ← view
              └──► menu / shadow / search surfaces          ← views
```

#### The graph is a view

The graph does **not** store facts. It stores **typed
references** to facts the ledger has already committed.
An edge is `(source_ref, relation_type, target_ref)`,
where each `Ref` resolves to an M0 `record_id +
revision_id` and a durable fact position. If the graph
disagrees with the ledger, the ledger wins — the graph
is re-bound, not the other way around.

This is why the graph can be **demand-driven and
rebuildable**. Because it is a view:

- A new edge type enters through the demand loop (below)
  and is **built from existing facts**. The graph never
  invents relations the ledger hasn't seen.
- A failed / outdated edge is **dropped and rebuilt**
  from the ledger. The graph does not accumulate "stale
  facts" because the graph stores no facts.
- The worst case — total graph corruption — is
  **rebuild from ledger**. Every typed edge can be
  re-extracted by replaying the ledger's committed
  facts through the admitted extractor revision. The
  graph is **regenerable**; the ledger is not.

#### Why this matters for the recall chain

The recall chain trusts the **truth layer** and treats
graph outputs as **advisory**:

- Entry-level recall + ledger references are
  **authoritative** — they are facts the system has
  committed.
- Graph traversal results are **suggestive** — they
  surface connections that may or may not be load-
  bearing for the current query. The recall chain may
  use them to **expand** a candidate set, not to
  **replace** one.
- When graph and entry-level disagree, entry-level +
  ledger win. The graph's lossiness is structural; the
  ledger's auditability is structural.

This is the **SSOT contract** that keeps the system
honest:

> **Truth = ledger + memory entries (evidence anchors).**
> Everything else (vector index, graph, menu, shadow,
> surface) is a **view**. A view can be rebuilt from
> truth; truth cannot be rebuilt from a view.
> **When in doubt, rebuild.**

---

Entry-level recall finds entries by **similarity**. Causal
recall finds entries by **how they connect**. The two are
complementary views of the same bank:

- **Entry-level** — `memory_search` returns the entries
  whose vector / FTS match the query, with Thompson
  exploration and conf gating. This is angle ⑥.
- **Relation-level** — `memory_graph_query` returns the
  entries reachable from a seed through admitted edge
  types: distillation lineage, supersession, causal links,
  evidence chain, contradiction. This is a **causal /
  structural** traversal, not a similarity search.

The two coexist. Similarity finds "what looks like this";
the graph finds "what caused this, what did this cause,
what contradicts this, what distilled into this". A
typical recall benefits from both: similarity finds the
relevant entry, the graph traces **why** the entry was
created, **what** it contradicts, and **what** distilled
from it.

#### Edge types — admitted vocabulary

```
# Distillation lineage  (admitted in v1 — ledger mirror)
episode    --distilled_to-->   fact
episode    --distilled_to-->   lesson
fact       --promoted_to-->    playbook
playbook   --superseded_by-->  playbook (newer revision)

# Evidence chain        (admitted in v1 — ledger mirror)
entry      --evidence_ref-->   DurableFactReference

# Causal / structural   (demand-driven — not pre-admitted)
entry_a    --causes-->         entry_b       # "X fails because Y"
entry_a    --contradicts-->    entry_b       # incompatible claims
entry_a    --supports-->       entry_b       # reinforces a claim
```

Each edge is an **admitted durable fact** —
`memory.relation_recorded` with the exact source / target
revision, the relation type (closed vocabulary, not free
text), the evidence reference, and the versioned
extractor revision that emitted it. Edges do not invent
relationships — they are admitted through the same
evidence-anchored admission gate as entries, and they are
revisioned so a superseded entry's outgoing edges can be
re-bound to the new revision without rewriting history.

#### Why it is worth doing

- **Causal traversal**: "lesson A was distilled from
  episode X, which was contradicted by observation Y,
  which came from session S42 position 317" — the agent
  follows the chain, not the similarity. Without the
  graph, this reconstruction is impossible; with it, it's
  one bounded query.
- **Multi-hop reasoning**: "find all lessons that
  contradict this fact AND were distilled from episodes
  in the same topic cluster" — pure vector / FTS cannot
  answer this; graph traversal can.
- **Audit trail**: every entry has a known provenance
  chain. The graph is the **explicit lineage** the audit
  needs to verify distillation integrity, and the path
  the reactivation recheck follows when an Archived
  entry is lifted back to Active.

#### What stays in v1 vs demand-driven

- **In v1 (ledger mirrors only)**: distillation lineage
  edges (episode → fact / lesson, fact → playbook),
  supersession edges (playbook → playbook), and
  evidence-ref edges are admitted because they are
  **already implicit in the M0/M1 ledger facts** — the
  graph is the typed projection of facts the system has
  already committed, not a new semantic layer. No new
  semantic edge type is admitted in v1 ahead of
  measured demand.
- **Demand-driven**: causal / contradiction / support
  edges are **not pre-admitted**. They enter the
  vocabulary only when the recall chain surfaces a
  repeated failure pattern that the new edge type
  would close (see "Demand-driven growth" below). The
  registry stays closed; the proposal pipeline stays
  open. New edge types are grown, not built.

> **The graph in v1 is the typed mirror of ledger facts
> the system already has. Every other edge type must
> earn its place through recall failure, proposal,
> admission, and regression — never through pre-design.**

> **Entry-level recall finds similarities; the memory
> graph finds connections.** The two views are queried
> separately and composed by the recall chain — a hybrid
> query can ask "give me the top-K similar entries, then
> for each, list its causal predecessors and
> contradicting siblings". Entry recall is the **what**;
> the graph is the **why**.

### Graph storage strategy — typed links, not a graph engine

Graph storage is **not** a separate graph database
(Neo4j, Memgraph, etc.). The graph is built on top of
**typed link fields** declared in the `MemoryTypeRegistry`:

- Each entry kind declares the link field types it
  admits: `distilled_from: Ref[]`, `supersedes: Ref`,
  `contradicts: Ref[]`, `evidence_refs: Ref[]` — **closed
  vocabularies**, not free text. The vocabulary lives in
  the same registry that already governs `inject_kind`,
  `retention_policy_revision` and the rest.
- Link fields are typed exactly like other M0 fields
  (canonical JSON, JCS digest, versioned schema). An
  edge is just another durable field, not a parallel
  database.
- Traversal is a bounded query type over the existing
  ledger; one-hop / two-hop walks run under the same
  recall profile that gates entry recall (budgets,
  truncation, exact integers).

#### Why typed links, not a graph engine

- **SSOT for ledger durability**: graph state shares the
  same commit / restart / receipt guarantees as entry
  state. A separate engine would require cross-database
  coordination that the current M0 adapter does not
  guarantee (⑤ footnote on `memory.db`).
- **Versioned type vocabulary**: adding a new edge type
  is a `MemoryTypeRegistry` schema change, not a database
  migration. The closed vocabulary keeps edge meaning
  portable across Rust / Kotlin and keeps it from drifting
  into free-text noise.
- **Bounded traversal cost**: one-hop / two-hop queries
  have a known fan-out bound. A general graph engine
  invites unbounded pattern matching; typed links keep
  the traversal shape explicit and budgetable.

#### Traversal depth — one-hop and two-hop

The recall graph query admits **two depths** under a
frozen profile:

- **One-hop** — return the seed entry plus every entry
  directly linked through an admitted edge type. Used
  for "what does this contradict / what distilled into
  this / what supersedes this" — single-step lineage.
- **Two-hop** — from the seed, follow one edge, then
  from each neighbour follow another. Used for "lesson A
  contradicts fact B, which was distilled from episode X"
  — multi-step reasoning.

Deeper traversals remain evaluation-gated. The graph
itself is **deeper than two hops**; the recall profile
only exposes **what the budget can afford**. Going from
two hops to three hops requires a versioned profile
revision and measured regression on precision / cost.
This is the budget boundary that keeps graph queries from
silently turning into unbounded pattern matches.

#### Where to borrow, where to build

- **Borrowed (design)**: closed-vocabulary edge
  semantics (RDF / property-graph literature), the
  one-hop / two-hop budget pattern (knowledge-graph
  query research), the typed-link-as-field shape (schema-
  driven relations, not free-form triples).
- **Built (infrastructure / algorithms)**: the durable
  storage itself (M0 ledger + typed link fields), the
  traversal implementation (bounded query type over the
  ledger), the admission / lifecycle integration (graph
  edges pass the same evidence-anchored gate as
  entries), and the cross-language Rust / Kotlin parity.
- **Open for deeper design**: deeper traversal patterns
  (three+ hops, conditional walks), multi-entry query
  language (graph + similarity composition), conflict-
  resolution strategies (when traversal surfaces
  contradicting subgraphs), hot-cache vs cold-traversal
  split for popular subgraphs, and representation of
  **negative knowledge** — "this fact was attempted and
  failed" — as a first-class edge.

> **Graph storage is a typed-field problem, not a
> graph-database problem.** The vocabulary is a
> `MemoryTypeRegistry` concern; the storage is an M0
> concern; the traversal is a recall-profile concern.
> Three ownerships, one bank.

### Limits of the graph — what graphs don't do

The graph is **necessary but not sufficient**. Admitting
it into v1 does not mean it solves memory; it means
memory gains a complementary mechanism. Six honest
limits frame what the graph can and cannot do.

#### 1. Extraction boundary — only entity-relation pairs

Only content that can be cut into **(subject,
relation, object)** triples enters the graph. Sentences
that resist this cut — long narratives, conditional
prose, qualitative reasoning, explanations with
embedded metaphor — become **orphans**: they remain in
entry-level recall (vectors) but never produce a typed
edge.

This is not a defect of the extractor; it is the **form
of the data**. Memory admits this through admission — an
entry whose content cannot yield an admitted edge still
has its `evidence_refs` edges and its `mtype` lineage,
but it cannot acquire a `causes` / `contradicts` /
`supports` edge because no admitted extractor revision
can produce one with evidence grounding.

#### 2. Formalisation is lossy

`natural language → triple` **drops** tone, condition,
hedging, register, and the qualitative middle. A lesson
like "method X usually fails under condition Y, but
sometimes works in odd contexts" becomes
`(X, causes, fail) | (Y, condition_of, fail)` — the
**usually**, the **sometimes**, the **odd** all
disappear. The entry-level text still carries them; the
graph does not. Recall chains that depend on tone /
hedge / register must route through **entry-level +
text**, not through the graph.

The graph's lossiness is **structural**, not a bug to
fix. The fix is to **route around** it: keep the graph
for what it captures cleanly, keep the entry text for
everything else.

#### 3. Ontology drift

The `MemoryTypeRegistry` vocabulary is **predefined**,
closed, versioned. Real content does not respect closed
vocabularies — it invents new categories faster than
any registry can enumerate them. An extractor that meets
an un-named relation type has three choices, all bad:

- **Force-fit** — emit an edge with the closest existing
  type, producing a silent semantic drift. Rejected —
  this corrupts the closed vocabulary.
- **Reject** — drop the edge, lose the signal. Acceptable
  when the lost edge is not load-bearing.
- **Propose** — emit a `relation_type_proposed` Candidate
  for the registry to admit. This is the **admitted**
  path. The proposal carries the extractor revision,
  evidence references and a draft digest; registry
  admission is an explicit, versioned decision.

The registry stays closed; the **proposal pipeline**
stays open. New types enter only through admission,
never through silent vocabulary expansion.

#### 4. Reasoning boundary — resultative, not full

The graph supports **resultative** reasoning — what
directly causes / contradicts / supports what. It is
**weak** at:

- **Default reasoning** — "what is usually true when
  nothing else is known" is not in the graph; it lives
  in similarity (entry-level) or in tacit knowledge.
- **Analogical reasoning** — "this is like that" is
  similarity's job; the graph has no analogue matcher.
- **Provenance reasoning** — "where did this fact come
  from, and what does its history tell me" is partly
  ledger-backed (Episodic shape) and partly entry-level
  (Evidence chain); the graph's `evidence_refs` is one
  signal, not the whole story.

A recall chain that asks for default / analogical /
provenance reasoning **must compose** graph with
entry-level + ledger. Pure-graph reasoning under-
delivers on these three.

#### 5. Maintenance cost — graph rots without feedback

A graph without a feedback loop **rots**: stale edges
stay, contradictions accumulate, dead branches grow.
The Memory graph does not escape this — it inherits the
same feedback mechanisms the entry-level bank already
has:

- **Reactivity from observations** — when an
  observation falsifies a lesson, the graph edges from
  that lesson (`causes`, `supports`) are re-evaluated; a
  confirmed falsification may invalidate downstream
  `supports` edges transitively.
- **Audit-driven re-binding** — when the periodic audit
  (⑦ Lessons) re-evaluates quiet lessons, the outgoing
  edges are re-bound to the new revision.
- **Versioned re-bind on supersession** — every
  supersession emits a new edge that re-points from the
  new revision, so the graph never silently refers to a
  tombstoned entry.

A graph without these mechanisms is just a frozen
snapshot of one extractor run. With them, the graph is
**continuously re-grounded** in observations and audit,
which is what keeps it from rotting.

#### 6. Graph is supply, not consumption

The graph is **supplied to the LLM**, not used by it
directly. The LLM composes the answer from entry-level
recall + ledger references + graph traversal +
similarity, all surfaced through the same recall
contract. The graph itself does not output prose; it
**feeds** the recall chain that feeds the surface that
feeds the model.

This also means **personalised knowledge resists
graphisation**. A user's preferences, habits, hedges,
pet peeves, conversational patterns — these live in
similarity (vector) and in entry-level (text), not in
typed triples. The graph captures the **structural
backbone** of memory; the personal layer rides on top
through other shapes.

> **The graph is necessary for causal traversal; it is
> not sufficient for memory.** Admitting it into v1 is
> admitting one shape of five; the other four
> (similarity, normative, episodic, tacit) carry what
> the graph cannot. The limits are not failures to fix
> — they are the boundary that defines what the graph
> is for.

### Demand-driven growth — the graph is grown, not built

The graph is **demand-driven**, not pre-built. v1 admits
only the edges that are **already implicit in the M0/M1
ledger facts** — `distilled_from`, `supersedes`,
`evidence_refs` — because they are mirrors of facts the
ledger has already committed. **No new semantic edge
type** is admitted in v1 ahead of measured demand. The
graph's vocabulary in v1 is the **trace of facts the
system already has**, not the seed of facts it might
one day want.

The growth path runs in a closed loop:

1. **Recall failure surfaces the gap.** A repeated
   failure pattern — "this query keeps returning the
   right entry but the model never connects it to its
   cause" / "two contradicting entries keep both
   surfacing" / "the lineage from episode to lesson is
   missing in the menu" — is logged through the recall
   chain's outcome record. The failure is
   **demand signal**, not a hypothesis.
2. **Proposal emitted.** The recall-failure analysis (or
   an admitted extractor revision observing the same
   gap) emits a `relation_type_proposed` Candidate
   naming the missing edge type, the evidence
   references, and the specific failures it would
   close. The proposal carries its extractor revision
   and a draft digest.
3. **Registry admits.** The `MemoryTypeRegistry`
   admits the edge type through a versioned schema
   change. Only at this point does the typed link
   field become available in entry shapes; only at this
   point do extractors start emitting it.
4. **Regression proves the closure.** The new edge type
   is exercised under a frozen recall profile;
   precision / cost improvements are measured against
   the prior baseline. The edge type **stays admitted**
   only if regression is positive; otherwise the
   proposal is reverted and the edge is dropped.

The graph therefore grows **one typed edge at a time**,
each backed by a recall failure it closes. The registry
never expands without an explicit admission gate. New
edge types are **earned**, not declared.

This is the **opposite** of building a full ontology
first and finding uses for it. The ontology is the
**trace** of the recall failures the system has
already encountered; the graph is its **distillation**.
Pre-built ontologies rot because nothing in them is
connected to a measured problem; demand-driven edges
persist because every admitted type has a closure it
can claim.

> **The graph is not designed top-down; it is grown
> bottom-up from the system's own recall failures.**
> v1 admits only the edges that already exist in the
> ledger. Every other edge type must earn its place
> through the closed loop: failure → proposal →
> admission → regression. The vocabulary is the
> **trace** of problems the system has already solved;
> nothing enters ahead of measured demand.

### Recall profile — questions bound by a versioned policy

| Profile field | What it controls |
|--------------|------------------|
| **Trigger** | Which effective-snapshot purpose may request a product; risk-action remains unadmitted. |
| **Budget** | Exact item/UTF-8/token limits shared with C2; there is no default percentage. |
| **Ranking** | Exact policy revision and integer components; no RRF weight is selected by M1. |
| **Inject kind** | surface-level kind (e.g. `memory.lesson`, `memory.fact`) |

### Inject kind — surface-level shape

When a memory entry is recalled, the surface sees it as an
entry with an **`mtype` → `inject_kind` mapping**. The user
sees the entry's **kind** (not the raw mtype) — so `memory.lesson`
appears as a `memory.lesson` block in the surface.

### Add a type = code, policy, registry row and fixtures

The following sketch is only an intuition. The normative M1 tests additionally
require exact type coverage, canonical order, allowed roles/authorities and
known policy revisions:

```python
def test_memory_type_registry_consistent():
    for mtype, type_def in registry.items():
        assert type_def.inject_kind in entry_kinds, \
            f"{mtype} injects {type_def.inject_kind} not in entry_kinds"
        assert type_def.lifespan.retention >= 0
        assert type_def.lifespan.retention <= type_def.lifespan.decay_threshold
```

### Entry shape — research notation mapped to M0/M1

```python
class MemoryEntry:
    id:              uuid          # global identity
    mtype:           str           # 'memory.lesson' / 'memory.fact' / ...
    content:         Value         # mtype-specific payload
    provenance:      Provenance    # source_session + source_seq + scope markers
    evidence_tally:  EvidenceTally # exact verified/falsified/neutral integers
    last_observed:   uint64        # durable position, not a wall-clock guess
    scope:           ScopeBinding  # opaque Runtime-authorised owner
    boundary_cases:  Candidates    # immutable evidence-bound proposals
```

The actual identity is M0 `record_id + revision_id`; provenance is one or more
exact `DurableFactReference` values containing Session, position, fact ID and
payload digest. Boundary cases do not mutate an entry in place. Floating
confidence may be a versioned display projection but is not stored truth,
authority or lifecycle permission.

### Hot / cold split — lifecycle projection, not another state machine

The ordinary eligible set contains `Active` entries. A bounded menu/detail
product may be requested under the effective snapshot and offered to C2; it is
not injected every turn.

`Cold` entries require explicit detail lookup or policy-governed reactivation.
`Archived` entries are excluded from menus and require an admitted detail
purpose that explicitly includes them. `Candidate` requires frozen exploration.
`Promoted` is excluded from normal Memory recall because Knowledge owns the
published revision.

The split is Letta-inspired product vocabulary implemented by M1 state plus a
versioned recall profile. Counts and storage sizes are explicit configured
bounds, not "tens" or "hundreds" contracts.

```
┌────────────────────────────────────────────────────────┐
│           ORDINARY ELIGIBLE SET                         │
│   active entries under frozen scope/policy/bounds       │
│   committed product -> C2 retained or dropped           │
└────────────────────┬───────────────────────────────────┘
                     │ explicit lifecycle policy
                     ▼
┌────────────────────────────────────────────────────────┐
│           COLD LAYER (retrieval-only)                    │
│   cold; archived only for admitted detail purpose        │
│   promoted is owned and recalled by Knowledge            │
│   Runtime candidate ports remain replaceable             │
└────────────────────────────────────────────────────────┘
```

### Reactivation — application-driven promotion out of Cold/Archive

`Cold` and `Archived` entries are **not** forgotten — they
are **outside the injection window**. Forget means
"withdraw from injection"; it does not mean "delete" or
"un-queryable".

| State | Framework injects? | Tool / UI queries? |
|-------|--------------------|--------------------|
| **Active** | ✅ under the frozen recall profile | ✅ |
| **Cold** | ❌ | ✅; **successful application → promoted to Active** |
| **Archived** | ❌ | ✅ only through an admitted detail purpose that names Archive as eligible; successful application → promoted through the receipted reactivation path |

**Successful application triggers reactivation**. The
recall chain records `recall.apply` against a Cold or
Archived entry; the outcome (Verified / in-scope
Falsified) drives a normal β update **and** emits a
lifecycle event:

- **Cold → Active** is the **direct** path. An admitted
  `recall.apply` whose outcome is Verified or in-scope
  Falsified lifts the entry back to Active. The
  reactivation is receipted but does not require a fresh
  admission check — Cold is a recall-eligibility
  projection, not a trust loss.
- **Archived → Active** is the **indirect** path. The
  recall itself must arrive through an **admitted detail
  purpose** that already names Archive as eligible —
  ordinary recall cannot reach Archive. The successful
  application triggers an admission-shape recheck
  (anchors valid, scope granted, recall profile permits)
  before the entry is lifted, because Archive implies the
  entry was deliberately demoted and should not silently
  re-enter the active set without policy acknowledgement.
  This recheck is **the "mechanism inside"** that keeps
  Archive from leaking back into Active through an
  unrelated recall.

**Reactivation is not implicit**. The lifecycle event is
explicit, not a side effect of a successful outcome. The
recall chain emits a receipted `memory.lifecycle_event`
fact that re-runs the admission checks; the entry is not
silently rewritten.

> **Forget is a withdraw-from-injection, not a delete.**
> Cold and Archived entries are queryable through ordinary
> tools and UI. A successful application is the natural
> promotion path back to Active — direct for Cold,
> admission-validated for Archive.

### Distillation sources — dual proposals, one admission path

Two sources may propose candidates asynchronously:

- **Session-end source** — after the durable Session prefix closes, a bounded
  extractor may emit a `SessionEnd` Candidate or safe Noop. It is best effort
  and cannot block Session closure or guarantee capture.
- **Scheduled-distillation source** — consumes one exact Session prefix,
  extractor revision, watermark and batch digest. Cadence and volume gates are
  explicit Runtime configuration with no built-in defaults.

Both sources are asynchronous to a completed Turn. Either uses the ordinary
configured model/Provider boundary or an admitted deterministic extractor port;
there is no built-in cheap/medium model.

```
session end ────────► bounded extract ────────► Candidate / Noop
                                                       │
                                                       ▼
                                              candidate (queued)
                                                       │
                          exact watermark ──► batch extract   │
                          + fixed prefix/revision             ▼
                                              Candidate / Noop
                                                       │
                                                       ▼
                                              normal M0/M1 admission
```

The two sources converge only at the normal decision/admission boundary. Neither
directly writes, merges, activates, archives, deletes or promotes a revision.

The production adapter currently persists Memory projections transactionally
with the Runtime SQLite store so Memory facts and visibility obey
commit-before-context and restart recovery. A future physically separate
`memory.db` is an adapter choice, not an accepted domain boundary; it must first
prove atomic coordination and recovery without a cross-database transaction.

## ⑥ Read paths — four ways × five timings

### Four recall **ways** (the methods)

| Way | What it solves | Failure mode covered |
|-----|----------------|------------------------|
| **Vector semantic search** | Fuzzy recall — "the deployment issue from last time" | Query paraphrases the entry; exact terms don't match |
| **FTS keyword** | Exact terms — tool name, error string | Vector search drifts to "similar" items; user wants the specific one |
| **Recency scan** | Recent context continuity — what's been on the surface lately | "I just saw it" — vector/FTS miss because the entry isn't indexed recently |
| **Menu index (always-on)** | Discoverability — framework injects a lightweight catalog so the agent knows **what's available** | The agent didn't think to call `memory_search` — at least the menu is on the surface |

The three "real" retrieval methods — **vector, FTS, recency**
— have **complementary failure modes**. The menu is the
always-on overlay that solves "I don't know to look".

### Hybrid retrieval — three failure modes are complementary

Each retrieval method has its own blind spot:

- **Vector** fails on **specific terms** — a tool name or error
  string doesn't have semantic neighbours; the closest
  vector hit is "similar concept", not "this thing".
- **FTS** fails on **paraphrased queries** — if the user
  asked "deployment thing" and the entry is "CI cache
  invalidation", no keyword match.
- **Recency** fails on **deliberate lookup** — old but
  important items are downweighted by their `last_used`.

**Hybrid recall** runs all three and **fuses** the result
sets. Each method's hits are unioned with the others; the
fusion reranks.

### RRF (Reciprocal Rank Fusion) — research candidate

For query `q`, each ranker returns a ranked list:

```
RRF(q, entry) = Σ_r  weight_r / (k_r + rank_r(entry))
```

- `rank_r(entry)` is entry's position in ranker `r`'s list (1-indexed; 0 if absent)
- `weight_r` is the ranker's weight (calibrated; default 1.0 each)
- `k_r` is a constant per ranker that dampens the contribution of low-rank items (Cormack's constant k=60 is the canonical default)

The fused ranking would be `RRF_score(q, entry)` summed across all rankers.
This is a useful deterministic candidate, but the weights, constants, source
rankers and tie-breaks require a versioned Runtime policy and evaluation before
they can replace M1's admitted integer baseline.

### Two-stage — research candidate

The four-stage pipeline:

1. **Coarse retrieval** (the three recall methods fused via
   RRF) — returns top-K candidates where `K` is generous
   (e.g. `K = 200`).
2. **Rerank** — the K candidates are scored against the query
   by a stronger model or a richer feature set (cross-encoder,
   or an LLM-based relevance judge). The rerank returns the
   final top-k.
3. **Filter** — apply the 5 recall gates (confidence,
   freshness, etc.) on the rerank top-k.
4. **Inject** — the final top-k enters the surface as memory
   entries with provenance + conf + staleness markers.

This is the standard two-stage retrieval pattern — coarse
recall is cheap and recall-oriented (high recall); rerank
is expensive and precision-oriented. The two together give
the best of both.

### Recall chain — conf + staleness are computed, not stored

The recall chain reads `conf` from each candidate and lets
it participate in two distinct roles: **gating** (drop or
demote) and **annotation** (mark the surface entry). The
freshness factor `F` is **lazy**: it is computed at recall
time from `now − last_verified`, never persisted as a
stored column.

| Stage | Reads | Computes | Role |
|-------|-------|----------|------|
| **Coarse** (vector / FTS / recency) | entry identity, `mtype`, content index, `last_verified` | per-query `B` only | RRF fusion produces top-K |
| **Rerank** | identity, per-entry `α`, `β` | Thompson sample `p ~ Beta(α, β)` blended with `B` | orders top-K by exploration-aware relevance — **ranking does not see conf** |
| **Filter** | full `conf = E × R × B × F`, `F` | gate 1 (conf threshold), gate 2 (freshness) | **gating** — drop below-threshold or mark `low-confidence`; mark `stale` if `F < threshold`; lessons exempt from time-only staleness |
| **Inject** | `conf`, `staleness` | gate 3 (transparent injection) | **annotation** — surface entry carries `conf`, `staleness`, and provenance |

**Why lazy `F`**: `F = 1 − (now − last_verified) / F_max_age`
is a function of the recall clock. Storing it would freeze
the value to write-time, which is meaningless when the
recall clock advances. `last_verified` itself is a durable
position (M0 `last_observed`), not a wall-clock guess —
only an admitted `memory.observation_recorded` fact may
advance it.

### E is admission-time only

`E` is **structural**, not temporal — it cannot decay or
go stale, but it can be **voided** by admitted lifecycle
events. The four cases where `E` is re-evaluated:

| Event | What happens to `E` |
|-------|---------------------|
| **Admission** | `E = 0` (anchor-less) → entry rejected at the M0/M1 boundary |
| **Supersession** | New revision replaces the old; the new entry's `E` is recomputed from its own anchor; the old entry is tombstoned, not refreshed |
| **Forget / corruption** | Authorized forget, retention or corruption event → entry tombstoned and removed from the eligible set; `E` is not "decayed" but **nullified by event** |
| **Falsification** (`β + 1`) | `E` unchanged; `R` drops; the anchor is still valid — what changed is the claim's truth-rate, not its evidence |

`E` is never recomputed at recall. The recall chain reads
`E` from the durable fact (or from the entry's admitted
revision); recall is **read-only** with respect to evidence
structure. If the anchor is retracted, the entry's
lifecycle state removes it from the eligible set before
`E` is even consulted — recall never has to ask "is this
anchor still good?", because admission and lifecycle
already answered that question.

**Two roles, one value**:

- **Gating** is a binary/ternary decision: above threshold
  → inject; below threshold → drop or mark
  `low-confidence`.
- **Annotation** is a label the surface carries: `conf`,
  `staleness`, `mtype`, provenance. The model sees
  testimony, not certification.

The two roles are independent: a below-threshold entry that
is still injected must be **explicitly** marked
`low-confidence` so the model knows. A high-`conf` entry
with expired `F` is gated as `stale` (or refreshed via
re-verify) without affecting its `R`. `E` (evidence
anchor) cannot go stale because it is structural, not
temporal — `E = 0` is rejected at admission, not at
recall.

> **Recall chain reads conf from the durable fact and
> computes F lazily at recall. The conf participates in
> gating (drop / demote) and annotation (surface label).**
> Storing F would corrupt the calibration — the formula is
> a function of the recall clock, not a write-time snapshot.

### Exposure bias — the cold-start trap

A new memory entry starts with `R = 1 / (1 + 1) = 0.5`
(uniform prior) and an anchor-driven `E`. **If ranking
uses the posterior mean `R` as a weight**, low-`R` entries
are sorted behind high-`R` entries, never produce a
`recall.event`, never update β, and never move `R`. This
is the **exposure-bias death loop**:

```
conf low → ranked low → not recalled → no outcome → β stays low → conf stays low
```

The recall chain breaks this loop through **three role
separations** that keep ranking, gating, and exploration
independent.

#### 1. Ranking ≠ gating

`conf` is **not** a ranking weight. The rerank stage uses
only **relevance** (`B`) and a Thompson sample of the
posterior `R` (below). `conf`'s role is **only gating** at
the filter stage. The two paths do not share a formula:
a high-`conf` entry with poor `B` does not outrank a
low-`conf` entry with strong `B`, and a low-`conf` entry
with strong `B` is not excluded from the top-K just
because its `R` is low.

#### 2. Thompson sampling at rerank

Instead of using the posterior mean `R = α / (α + β)` as
a sorting weight, the ranker draws **one** sample per
candidate per recall:

```
p_i ~ Beta(α_i, β_i)
score_i = w_B · B(query, entry_i)  +  w_T · p_i
```

- **Few observations** → posterior is wide → sampled `p`
  is volatile → occasionally high → occasionally wins a
  top-K slot.
- **Many observations** → posterior narrows → sampled `p`
  converges on `R` → stable, evidence-driven ranking.

New entries get exploration **for free** without an
explicit cold-start policy. The exploration rate is
governed entirely by the data — the width of each
entry's posterior — not by a hand-tuned schedule.

#### 3. Exploration slot in top-K

One top-K position is **reserved** for the
highest-uncertainty candidate that did not already win a
slot, regardless of its Thompson score. This prevents
Thompson draws from being dominated by high-traffic
entries and guarantees every entry a periodic chance to
surface. The slot covers **Thompson variance, not
Thompson mean** — its purpose is to **probe the tail** of
the posterior, not to substitute for relevance.

### Monitoring — never-recalled ratio as recall health

The exposure-bias trap is **observable**: entries that
never produce a `recall.event` are systematically
invisible to the feedback loop. The recall-health metric
is:

```
never_recalled_ratio = entries_with_zero_recall_events / total_active_entries
```

- A **rising** ratio signals ranking is too concentrated
  on popular items.
- A **non-zero floor is expected** — some entries are too
  niche, too stale, or too duplicate-of-existing to ever
  match. The metric is the **rate of change**, not the
  absolute value.
- Weekly regression correlates this ratio with the
  Thompson `w_T` weight and the exploration-slot coverage
  to find the calibration that keeps the bank's coverage
  healthy.

> **Ranking uses relevance + Thompson exploration, not
> conf. conf gates, Thompson explores, monitoring closes
> the loop.** The death loop is broken because the way
> entries get observed is independent of how confident
> they currently are. Thompson weights, exploration-slot
> coverage, and the never-recalled metric remain
> evaluation-gated policy candidates — they enter the
> recall profile only after measured regression proves
> they beat the current integer baseline.

### Query expansion — research candidate

A single query often misses relevant entries because the
phrasing doesn't match the entry's wording. **Query
expansion** generates **multiple phrasings** of the same
intent and runs the hybrid retrieval against each:

```
expansions = llm_expand(query, n=3)
# e.g. ("deployment issue", "CI failure", "release broken")

results = {}
for q' in expansions + [query]:
    for ranker in [vector, fts, recency]:
        results.update(ranker.search(q', k=20))

# deduplicate by entry_id
top_k = rrf_fuse(results, k=K)
```

The proposed `llm_expand` is **narrowly scoped** — the LLM is asked to
produce "3 different ways a user might phrase this query",
not to extract anything substantive. The expansion is **the
LLM's** contribution; the **retrieval and ranking** are pure
math.

This is not a built-in secondary model and has no implicit fallback. A future
policy must use the ordinary configured model/Provider boundary or an explicit
deterministic port, with its revision and output committed. One research option:
use the entry's known metadata — `mtype`, `confidence`,
`last_verified`, `source_session` keywords — as expanded
query forms. **Pure math**, no LLM.

### Recall surface — the menu (always-on)

The recall menu is the discoverability product the framework may offer to C2:

```
<memory_menu scope="user=abc">
  <index>
    <category kind="memory.lesson" entries=12 high_conf=8>
      "X fails because Y" (mem:abc, conf=0.9, T-3d)
      "Z returns null on empty" (mem:def, conf=0.7, T-1w)
      ...
    <category kind="memory.fact" entries=23 high_conf=18>
      ...
    <category kind="memory.playbook" entries=4 high_conf=3>
      ...
  </index>
</memory_menu>
```

- Compact (titles + meta only — no full body in the menu)
- Ordered by the frozen M1 selection policy
- Retained or dropped by C2 under the shared item and UTF-8 budgets

The **menu's only job** is to make recall possible: the
agent sees **what's available** and decides **whether to pull**.
Detail is on demand (recall tool / knowledge lookups).

### Menu shadow — existence signal for Cold / Archived

Cold and Archived entries are **outside the injection
window**, but their **existence** is still discoverable.
The menu adds a **shadow** layer that surfaces counts
without bodies, so the agent knows the bank has more than
what's in the menu:

```
<memory_menu scope="user=abc">
  <active>                                <-- menu as today
    <category kind="memory.lesson" entries=12 high_conf=8>
      "X fails because Y" (mem:abc, conf=0.9, T-3d)
      ...
  </active>
  <shadow>                                <-- existence markers
    <category kind="memory.lesson" archived=8 cold=5>
      "8 archived, 5 cold — pull on demand via memory_search"
    <category kind="memory.fact" archived=42>
      "42 archived facts (older context)"
    ...
  </shadow>
</memory_menu>
```

The shadow:

- Does **not** include titles or bodies — only the
  **count** and the **category**.
- Does **not** add ranking weight or conf — its sole
  purpose is **existence signaling**, not candidate
  injection.
- Lets the agent decide: "there are 42 archived facts;
  is what I'm looking for probably one of them?" — and
  call `memory_search` (cross-layer) to find out.

### Cross-layer search — memory_search spans every layer

The recall tool surface and the injection surface are
**distinct** views of the same bank:

| Surface | What it queries | Where it lives |
|---------|-----------------|----------------|
| `memory_search` | **Active + Cold + Archived** (Archive only via an admitted detail purpose) | Agent pull — any time |
| `memory_detail` | One specific entry by ID | Agent pull — drill-down |
| Memory menu `<active>` | Active only — titles + meta | Framework push at turn start |
| Memory menu `<shadow>` | Cold + Archived — counts only | Framework push at turn start |
| Memory detail injection | Body of selected entry | Framework push after agent pull / menu drill-in |

`memory_search` spans every layer **by design**. Cold and
Archived are not second-class citizens from the search
tool's point of view; they are second-class only from the
**injection window's** point of view. The admission-shape
recheck on reactivation (⑥ Reactivation) is what keeps
Archive from leaking back into injection without policy
acknowledgement — **not** what makes it unsearchable.

> **Forget is a withdraw-from-injection, not a delete.**
> The menu's shadow keeps Cold/Archived **discoverable**;
> `memory_search` keeps them **queryable**; only the
> injection window excludes them. Three independent
> surfaces, one bank.

### Read-path audit — three observation rows per recall

An admitted recall/application/outcome chain maps to existing durable facts:

| Row | Captures |
|-----|----------|
| `recall.event` | `memory.recall_recorded`: frozen request, selected revisions, exact integer score components and truncation. |
| `recall.apply` | `memory.obligation_opened`: an exact committed application fact, expected outcome, scope and attribution revision. Citation text alone is insufficient. |
| `recall.outcome` | `memory.observation_recorded`: typed reality evidence and `Verified`, in/out-of-scope `Falsified`, or `Neutral`. |

These facts are eligible inputs to a separately versioned calibration job. No
weekly schedule, Bayesian model or ranker mutation is implied by the chain.

### Recall quality — measured by the feedback loop, not by offline eval

Production feedback and pinned evaluation are complementary. The first detects
real-world outcomes; the second makes policy changes reproducible. The chain is

```
memory.recall_recorded → memory.obligation_opened → memory.observation_recorded
```

**The link** is the unit of evidence:

| Link state | Meaning | What it does |
|-----------|---------|--------------|
| `event` only | The model was shown the entry but did not cite it | **Censored** — no signal |
| `event + apply` (no outcome yet) | The model cited it but the world hasn't checked | **Pending** — outcome arrives |
| `event + apply + outcome.success` | The applied revision was reality-verified | Increment exact `verified`. |
| `event + apply + outcome.failure` | Reality falsified the revision in its declared scope | Increment exact `falsified`. |
| `event + apply + out-of-scope/uncertain` | Attribution is mismatch or inconclusive | Increment exact `neutral`; mismatch may propose a narrower Candidate. |

The chain's **density** is the signal:

- High density of `event + apply + success` chains per query
  pattern → that query pattern has good recall precision.
- Low density → recall is failing on that pattern; the
  query expansion + ranker weights need adjustment.

> **The durable chain is evidence, not automatically a metric.** A metric must
> publish its eligibility rule, exact integer numerator/denominator, policy
> revision and corpus/window binding. Production observations cannot replace
> pinned regression and privacy evaluation.

### Beta as indirect calibration

Recall precision is **not directly measured** by a held-out
test set. It is **indirectly calibrated** via the
`recall.outcome` → `β` update path:

- The **Beta-Binomial posterior** (angle ⑧) tracks per-entry
  reliability. `recall.outcome` (success/failure) updates `α`
  or `β`.
- The ranker weights `α_v, α_t, α_recency` in the RRF
  formula are **regressed weekly** against the chain density
  per query pattern: which ranker contributed the most
  *successful* `event → apply → outcome` triples?
- The **expansion prompts** for `llm_expand(...)` are
  regression-tested the same way: which expansions
  produce recall triples, which produce misses?

This is a **closed feedback loop**: the chain's outcome
feeds back into the weights that produced the chain. The
weights shift slowly; the loop is steady-state.

### What "good recall" means in practice

The user-visible signal of good recall is **not**
"the agent retrieved the right entry". It's:

- "The agent **cited** the entry in its reply" (apply),
- AND "the world **confirmed** the entry's claim was correct"
  (outcome.success),
- AND "the agent's action on the recalled entry **succeeded**
  in the world" (the actual outcome).

**Recall → apply → confirmed → world-success** is the
end-to-end chain. Recall quality is the **density** of this
chain across query patterns and over time. The weekly
regression computes per-query-pattern density and adjusts
ranker weights to maximise it.

### Anti-patterns the feedback loop catches

| Anti-pattern | How the loop catches it |
|--------------|--------------------------|
| Recall fires but the model never cites the entry | `recall.event` without `recall.apply` → chain density drops → ranker adjusted |
| Recall + apply, but cited entry contradicts reality | `recall.outcome = failure` → β +1 (failure counts) → entry demoted, ranker adjusted |
| Recall + apply, but entry is `stale` | `F` factor decays; downstream gates filter; `recall.outcome` is `censored` if entry wasn't used |
| New entry added but never recalled | `recall.event` count = 0; chronic lack → entry demoted via decay (lessons exempt) |

### Five recall **timings**

| Timing | Mechanism | Direction |
|--------|-----------|-----------|
| **Turn start** | C2 consumes a committed bounded recall product for the exact Turn prefix | Framework **push** within C2 budgets. |
| **Turn mid** | Agent calls `memory-search` tool when needed | Agent **pull** (entry detail on demand) |
| **Explicit ask** | User says "我们上次怎么定的" / agent asks | User-triggered / agent pull |
| **Risk action** | Proposed: Governance requests a separately authorized lesson product for `AskUser` | Not admitted; requires a focused cross-capability Spec. |
| **dream** | Distillation reads memory entries | Internal |

Risk-action recall remains a useful design target, but Governance cannot read
Memory through an implicit side channel. A future slice must bind the exact
risk class, Memory purpose, namespace/scope grant, fixed prefix, redaction,
durable recall fact, and `AskUser` presentation before dispatch.

### Mixing push and pull — the menu/index trick

The push half (menu index) keeps the memory's existence
**always on the agent's radar** without forcing the full
content onto the surface. The pull half lets the agent
choose when the detail is relevant. This hybrid avoids the
two pure-mode failure modes:

- **Pure push** (everything on every surface) — bloats the
  surface, dilutes attention, costs tokens.
- **Pure pull** (tool-only) — the agent doesn't know what it
> doesn't know; if it forgets to call the recall tool, the
> memory is invisible.

### Push/pull split by content type (re-stated for ⑥)

| Content | Push? | Pull? | Why |
|---------|-------|-------|-----|
| Preferences (`user_declared`) | bounded push | optional detail | May be mandatory candidates, but remain below system/policy/current input and inside C2 bounds. |
| Memory menu (index) | bounded candidate | — | If retained by C2, the agent sees the catalog and may drill in. |
| Memory detail | — | ✅ On demand | Body is large; agent decides relevance |
| Procedural (playbook) | partial — cached hint | ✅ On demand | Sometimes useful pre-loaded; full body on demand |

## ⑧ Retrieval quality — five gates

Memory's **delivered shape is "a hypothesis with epistemic
tags"**, not "an assertion". The recall contract is held by
five gates + one **confidence formula** + one **three-way
observability loop**.

### Confidence formula — `conf = E × R × B × F`

Recall uses **four composed factors**, not independent
sampling:

```
conf = E × R × B × F
```

The four are **multiplied**, not averaged — a single zero
factor (e.g. `E = 0` for an unanchored inference) makes the
whole conf zero. The product captures the **joint** hypothesis
strength, not "averaged opinions".

The **absolute** value of `conf` is **calibrated** against
historical data — the formula shape is fixed, but the
coefficients (e.g. the `r ≈ 0.6` prior) come from regression
on the memory bank's own outcome log. Calibration runs
weekly as part of the maintenance loop.

### E — Evidence

| Strength | Score |
|----------|-------|
| Direct quote of source ledger entry | 0.9 |
| Paraphrase of source | 0.7 |
| Inference (no direct source) | 0.5 |
| No source (anchor-less) | **0** — entry rejected at admission |

The **anchor-mandatory** rule (E = 0 → reject) is the same
principle that rejects unanchored extractions at admission
(angle ③). `E` is binary in the sense that the `0` case
short-circuits the formula.

### R — Reproducibility (Beta-Binomial conjugate)

`R` is **not** a heuristic. It's the posterior mean of a
**Beta distribution** with a uniform prior — the standard
Bayesian reliability score.

```
Prior:   Beta(α₀ = 1, β₀ = 1)   (uniform)
Update:  on success → α += 1; on failure → β += 1
R = α / (α + β)
```

The **uniform prior** (`α₀ = β₀ = 1`) means "we don't know
whether this entry is reliable — one piece of evidence
should move the estimate halfway". Subsequent successes and
failures **update the posterior**. This is the
**Beta-Binomial conjugate prior** — the Bayesian standard
for reliability scoring.

The **calibration** of `R`'s absolute scale happens against
historical data:

- The weekly regression computes, for each `conf ≈ 0.8`
  cohort, "of those entries, how often did they succeed?"
- If the predicted `0.8` matches the observed `0.78`, the
  prior's effective scale is right.
- If the prediction is off (e.g. predicted `0.8` but
  observed `0.5`), the prior and the mapping are recalibrated.

> **The formula shape is fixed. The coefficients are
> calibrated.** The hypothesis is that a `conf` value
> calibrated against observed outcomes is a usable
> ranking signal — the absolute number is the **output**
> of the calibration, not its input.

### B — Best-fit (per-query retrieval score)

`B` is the per-query retrieval score — how well this entry
matches the **current** query. Two-leg retrieval:

```
B = α_v · vector_score  +  α_t · fts_score
```

with weights `α_v` and `α_t` calibrated per query type (the
calibration data is the same outcome log as for R). `B` is
**the only per-query factor**; the other three are
context-independent.

### F — Freshness (last-verified-driven, **not** auto-decay)

`F` is **not** a simple "time since creation" decay. It is
"**time since last verification**":

```
F = 1 − (now − last_verified) / F_max_age
```

`F` is **reset to 1** whenever the entry is verified —
reproduced in another session, validated by a recall-and-apply
cycle, or audit-confirmed. An entry that hasn't been
re-verified for a long time decays; one that **is** being used
stays fresh.

> **Time alone doesn't kill memory.** Time **without
> re-verification** does. The decay is "**not proven
> recently enough**", not "**ancient**".

This is the principle that **lessons are exempt from
time-only tombstone** — a lesson about a failure that the
agent still re-discovers every time stays fresh because the
**use** itself is the re-verification.

### Censored observations — survival analysis

Right-censored observations (entry was recalled, but the
**model didn't act on it** in a verifiable way) are
**withheld** from the `β` update. **Recall ≠ use**.

```
if recall.outcome == applied:
    α += 1   # success — the entry was useful
elif recall.outcome == contradicted:
    β += 1   # failure — the entry was wrong
elif recall.outcome == recalled_not_used:
    # censored — no information, no update
    pass
elif recall.outcome == ignored:
    # censored — recall was offered, model chose not to use
    pass
```

> **"Wasn't recalled" ≠ "wasn't useful".** Wasn't recalled
> is just data we don't have; treating it as "useless"
> would penalise **high-traffic, high-avoidance** memories —
> exactly the ones we want to keep.

The `β` update is **only** for **observed outcomes** —
applied or contradicted. The `R` regression uses these same
observed outcomes for calibration.

### Three-way observability — the recall → apply → verify loop

Every link in the memory pipeline is a **queryable,
replayable, attributable** row:

```
recall event ──→ "X memory items retrieved; conf ∈ [low, high]"
                       │
                  model uses `[mem:abc]` in response
                       │
                  recall event ──→ "mem:abc applied to intent X"
                       │
                  reality gate runs (real event matches?)
                       │
                  verified + 1  or  falsified + 1
                       │
                  conf recomputed via formula
                       │
                  state-machine transition (candidate → active)
```

Three **observability points** are mandatory:

1. **Recall-side** — which entries were retrieved under the frozen policy.
   Durable `memory.recall_recorded` fact.
2. **Apply-side** — which entries the model actually used
   (`[mem:abc]` citation). `recall.apply` row.
3. **Output-side** — which exact evidence-tally update happened, against which
   obligation. Durable observation plus lifecycle facts.

The `recall.outcome` row is what feeds the `β` recalibration
loop — **directly attributable** to the recall event + the
apply event + the actual outcome. No vibes; everything is
in a row.

### Design principle — four properties

The observability + formula + loop together give the
memory layer **four properties** that distinguish it from
"store and forget":

> **可观测 (observability)**, **可计算 (computable)**,
> **能归因 (attributable)**, **自适应 (adaptive)**.

- **可观测** — every recall is a row; every apply is a row;
  every outcome is a row.
- **可计算** — conf is a function of (E, R, B, F), not vibes.
- **能归因** — every conf value traces back to its inputs;
  every β update traces back to the recall + apply + outcome
  triple.
- **自适应** — β is observed after every use; the conf formula
  recalibrates weekly. The system learns its own reliability.

### Five gates — complement the formula

The formula computes conf; the five gates decide what
memory can do at recall time.

| # | Gate | What it does |
|---|------|--------------|
| 1 | **Confidence gate** | Below-threshold entries are not injected (or are tagged `low-confidence` so the model knows) |
| 2 | **Freshness gate** | `facts.last_verified` expired → marked `stale` or re-verified; Lessons exempt from time-only tombstone |
| 3 | **Cognitive-transparent injection** | Injection carries source + conf + "may be stale" wording so the model sees this is testimony, not certification |
| 4 | **Post-use verification loop** | Recall contradicts reality → automatic conf down + state transition; candidate re-verifies |
| 5 | **Conflict presentation** | Two contradicting entries → high-conf-first + conflict flag, or both surfaced |

### Three-party sharing — the shared accountability

| Party | What it owns |
|-------|--------------|
| **Framework** (gates 1, 2, 3, 5 + formula inputs) | Confidence + freshness + conflict surfacing + observability log |
| **Model** (using the memory) | Self-weighting; verifies when critical; treats low-conf as hypothesis |
| **Reality** (gate 4 + β updates) | Closes the loop — the test that makes memory honest |

**No single party guarantees accuracy** — and that's the
whole point. **`agent_learned` is a falsifiable hypothesis**.
It does not become law. The user's `user_declared` entries
are the only law.

### Three-party sharing — the shared accountability

| Party | What it owns |
|-------|--------------|
| **Framework** (gates 1, 2, 3, 5 + formula inputs) | Confidence + freshness + conflict surfacing + observability log |
| **Model** (using the memory) | Self-weighting; verifies when critical; treats low-conf as hypothesis |
| **Reality** (gate 4 + β updates) | Closes the loop — the test that makes memory honest |

**No single party guarantees accuracy** — and that's the
whole point. **`agent_learned` is a falsifiable hypothesis**.
It does not become law. The user's `user_declared` entries
are the only law.

### Real-world reconciliation — no user required for daily checks

Outcome judgment is **not** the user's daily feedback channel.
The user is the **final adjudicator** for edge cases and for
`user_declared` overrides; the **daily** truth is settled by
the agent itself against the world:

| Memory claim | Reconciliation against reality |
|--------------|-------------------------------|
| "File at path P" | `fs.exists(P)` after the model acts on it — objective falsification |
| "Lesson: method X fails because Y" | The next attempt at method X — if it succeeds, the lesson is **falsified** |
| "Fact: the project's runtime is X" | The next time the runtime is invoked — observable evidence |
| "Episode: session X did Y" | Verifiable from the ledger (source_session + source_seq) |

The truth-source is the **world**, not the user. The user is
the **adjudicator** for ambiguous cases (e.g. lesson that
keeps failing in different ways — was it always wrong, or did
the world change?). For the **common case** — verify against
reality and update β automatically.

This is the **load-bearing** difference between Garive's
memory and every other memory system: their feedback signal
is "the user said X"; ours is "the world did X". The
confidence machine only works because the world is the input.

### LLM vs math — the role split

The memory pipeline splits work cleanly between
**LLM-judged** and **mathematically-computed** steps. The
boundary is "what can a deterministic formula express?" —
LLM does the rest.

| Step | Who | Why |
|------|-----|-----|
| **1. Memory extraction** | **LLM** | Semantic judgement — "this failure is a lesson", "this fact is general". Only an LLM can do this; math can't. |
| **2. Semantic verification** (confirm / falsify) | **LLM or embedding similarity** | "Does this newly-encountered outcome match the existing memory?" — LLM for hard cases, embedding-similarity threshold for cheap ones. |
| **3. Score computation** (`E × R × B × F`) | **math** | The four-factor confidence formula is closed-form. Calibration runs as regression against the historical outcome log. |
| **4. Recall ranking fusion** (PRF, RRF, dedup) | **math** | Vector + FTS score fusion (Reciprocal Rank Fusion), top-k selection, deduplication — all deterministic. |

The boundary is **deterministic**:

- If the answer is "this failure is a lesson", ask the LLM.
- If the answer is "did the lesson match this outcome", ask
  the LLM or embedding-similarity.
- If the answer is "what's the conf score", compute it.
- If the answer is "what's the top-k", rank it.

The **calibration layer** ties them together: the LLM-judged
outcomes (step 1, 2) feed the math-computed scores (step 3,
4) via β updates and weekly regressions. LLM is the
**producer**; math is the **statistician**. The LLM's outputs
are the **rows**; math is the **ledger**.

### Role split — illustrated

```python
# 1. LLM: extract a lesson from a failure
entry = llm_extract(
    mtype = "memory.lesson",
    evidence = "tool.result{status: error, ...}",
    content = "method X fails under condition Y",
    source_session = session_id,
    source_seq = seq,
)

# 2. LLM or embedding: verify against a new outcome
verdict = llm_or_embedding_match(
    entry, observed_outcome,
    threshold = 0.7,   # calibrated
)
# → "matches" → α += 1
# → "contradicts" → β += 1
# → "unclear" → censored (no update)

# 3. math: compute the four-factor score
score = E(entry.evidence) * R(entry.alpha, entry.beta) * B(query, entry) * F(entry.last_verified)

# 4. math: rank the recall candidates
top_k = fuse([(b, entry) for entry in candidates], method="RRF")
```

This is the **contract** between LLM and math:
**LLM produces rows; math produces scores**. The boundary
is preserved at every step.

## ⑦ Maintenance policies — promotion + anti-bloat

### Promotion channel — memory graduates to knowledge

The four types are not peers; they're a **distillation tower**
(see ③). The top of the tower (`playbooks`) is **hand-curated
from proven memories** — a memory entry graduates to
knowledge only through an accepted publication policy and exact receipt.

```
memory entry (with verified durable fact evidence)
    ↓  accepted versioned promotion policy
    ↓  separately authorized Knowledge proposal/publication
    ↓
knowledge entry (in `engine.proj.md` / wiki / shared knowledge base)
    ↓
original Memory lifecycle becomes Promoted with the receipt digest
```

**Concrete example:**
- `memory.lesson` — "SDK X has a caching bug" (with an exact verified fact
  reference for Session S42 position 317)
- A versioned policy may admit a Knowledge proposal after enough verified and
  sufficiently few falsified observations. Knowledge still owns publication.
- Only the committed publication receipt moves the original Memory lifecycle
  to `Promoted`; normal recall excludes it while audit retains the binding.

**Without this promotion channel**, memory and knowledge
**duplicate the same fact** — two storage locations, each
rotting at its own rate. With it: **memory is the raw-material
library; knowledge is the graduation destination** — a
**single-direction pipeline**, no duplication.

### Anti-bloat — six defenses

The chronic disease of memory systems: unbounded candidate production without
admission discipline causes growth → recall
precision drops → inject cost rises → noise drowns signal.
**Memory health = recall precision**, not entry count.

Six defenses:

| # | Defense | What it does | Where it lives |
|---|---------|--------------|----------------|
| 1 | **Distillation (debulk)** | Episodes distilled to conclusions; raw entries down-weighted. The tower structure IS the debulk. | `dream` watermark |
| 2 | **Quota (hard ceiling)** | Explicit per-type count/byte caps force ongoing priority judgement. Numeric values are Runtime policy and require measured admission; they are not defaults in this design. | `MemoryTypeRegistry` retention policy |
| 3 | **Admission filter (write gate)** | Three questions before entry: **can it generalise?** (no → reject, one-off detail); **is it stable?** (uncertain → defer + observe); **already present?** (dedup). Borrowed from Mem0's NOOP decision. | `dream` candidate → ADD/UPDATE/DELETE/NOOP pipeline |
| 4 | **Use feedback (natural selection)** | Reality-backed observations update exact tallies; policy may later cool or reactivate an entry. Missing use alone is not evidence. | Obligation/observation facts + versioned lifecycle policy |
| 5 | **Memory lint (periodic audit)** | Bounded audit reports duplicates, supplied contradictions, stale and low-use identities without choosing a winner or mutating state. | Scheduled audit + user-visible report |
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

### Lessons — length cap and audit-protected exemption

`memory.lesson` is exempt from time-only tombstone — a
lesson about a failure that the agent still re-discovers
every time stays fresh because the **use** itself is the
re-verification. This is correct, but it has a side
effect: lessons accumulate. Without bound, the lesson
bank grows unboundedly and the menu's `memory.lesson`
category drowns in old-but-not-stale items. The lesson
type therefore has its **own three defenses**, layered on
top of the general anti-bloat table:

| # | Defense | What it does |
|---|---------|--------------|
| 1 | **Byte / count cap per scope** | Lessons have an explicit quota (count + UTF-8 bytes per scope). Hitting the cap forces a periodic merge or supersession decision. Numeric values are Runtime policy and require measured admission; they are not defaults here. |
| 2 | **Periodic merge** | Two semantically-similar lessons with non-conflicting content → one merged lesson with combined evidence references and `merged_from` lineage. Conflicting lessons → both retained with a `conflict_marker`. |
| 3 | **Periodic audit — reactivation probe** | The audit specifically re-checks whether long-quiet lessons still belong in Active. A lesson with `recall.event = 0` for `T_quiet` and no `recall.apply` is **not** auto-demoted (decay-exempt), but the audit **explicitly** takes one of three actions: probe-recall it to generate a fresh observation, retire it through an admitted `Cool` event, or document it as **retained-by-policy**. |

The audit closes the **pathological-forgetting** loop. A
lesson that is correct but rarely matched by current
recall must not be silently tombstoned by a memory-lint
pass that only checks `recall.event` counts. The audit is
the **explicit policy decision** that keeps the lesson in
the bank — a missing audit, not a missing recall, is what
removes a lesson. Pathological forgetting is the failure
mode where **the lint pass runs without an audit** and
treats "never recalled" as "no longer useful", which is
censored-data reasoning (angle ⑧).

> **Lessons are exempt from time-only tombstone, not
> exempt from length policy and not exempt from audit.**
> Length cap + merge keep the bank bounded; the periodic
> audit is the explicit policy mechanism that prevents
> pathological forgetting of correct-but-quiet lessons.
> The audit result is a durable `memory.audit_recorded`
> fact that names the action taken per quiet lesson.

## Anomaly handling — memory error vs context mismatch

When a failure happens, the first question is **why**:
is the memory wrong, or is the memory right but **the
current context is out of its applicability scope**? The
answer determines what to update.

### Scope is a first-class field — extracted at write time

The ⑦ MemoryTypeRegistry entry schema already has `scope` as
a first-class field. The **extraction prompt** must enforce
it:

```yaml
# memory.lesson extraction
- content:        "method X fails because Y"
- applicability:  "when input file is in the format X, with condition Z"
  # not just the conclusion — the APPLICABILITY conditions too
- scope:          {scope_marker_1, scope_marker_2}
  # the entry's claim is bounded by these scope markers
- evidence:      tool.result excerpt
- source_session: ...
- source_seq:     ...
```

**The structured extraction captures both the conclusion AND
the applicability conditions.** Not just "method X fails" —
but "method X fails **when input file is in format X with
condition Z**". Scope markers are first-class — they're a
field on the entry, not a side comment.

### Conditional update rule

When a failure happens, the causal attribution is:

| Signal | Outcome | What happens |
|--------|---------|--------------|
| Memory applies to current context + context is similar | **Memory is wrong** | β +1, mark as falsified; rule refinement recorded |
| Memory does not apply to current context, or context is very different | **Context is out of scope** | β unchanged; **narrow the entry's scope**; record the new failure case as a boundary example |
| Memory's applicability is unclear | **Uncertain** | Defer; `dream` re-evaluates with the new evidence |

> **Failure inside scope + similar context → falsify the
> rule.** **Failure outside scope or dissimilar context →
> narrow the rule.** The two responses are different.

This is **case-based reasoning** (Schank's theoretical core):
**a failure is a case**, and the case teaches the **boundary
of the rule**, not the rule itself. The fix adjusts the
**applicability**, not the conclusion — unless the conclusion
is itself wrong.

### Two failure modes are different updates

| Failure mode | What changes | β effect | Entry effect |
|--------------|------------|----------|--------------|
| Memory error | Conclusion might be wrong | β +1 (failure counts) | `status = falsified`, candidate re-derivation |
| Context mismatch | Conclusion stays, applicability narrows | β unchanged | `applicability` narrowed; new boundary case added |

The **two failure modes are different updates**. Conflating
them is the bug — a memory that fires only in similar
contexts but gets blamed in dissimilar ones accumulates
false-failure β over time, leading to "the memory is
unreliable" — when the real problem is "the memory is
over-confident about its applicability".

### Recall — context similarity gate

At recall time, compute:

```
context_similarity = sim(query_context, entry.source_context)
```

Where `entry.source_context` is the **provenance snapshot**
captured at write time (the state of the conversation when
the entry was created).

If `context_similarity ≥ threshold`:

- Normal recall; inject with standard `E × R × B × F`
- The entry's "applicability was right for this query".

If `context_similarity < threshold`:

- Recall **but** inject with a **scope-mismatch marker**:
  ```
  [mem:abc] POSSIBLY-OUT-OF-SCOPE: this entry was written
  under context X (source_session=S42, ...). Current
  context is Y. Treat as a hypothesis, verify before acting.
  ```
- Apply a **weight discount** to the conf (`B *= weight_discount`,
  `weight_discount < 1`).
- The model still sees the entry — we're not hiding useful
  information — but the epistemic label is honest.

> **The scope-mismatch is declared at injection, not at
> failure.** The agent knows the entry is from a different
> context and acts accordingly; the failure-after-the-fact
> attribution does not apply.

### Five-state outcome — extended

The earlier `recall.outcome` schema has four states
(applied, contradicted, recalled_not_used, ignored). The
anomaly handling adds a fifth:

| State | Meaning |
|-------|---------|
| `applied` | Model cited and used; outcome was success → α +1 |
| `contradicted` | Model cited; world disagreed → β +1 (failure) |
| `recalled_not_used` | Model was shown the entry but didn't cite it → censored |
| `ignored` | Model chose not to use it → censored |
| **scope_mismatch_warning** | Recall occurred; `context_similarity < threshold`; injection carried the scope-mismatch marker; **not a feedback signal, an observability row** |

The fifth state is **observability** — it's not used to
update β, but it tells us "this recall was across
contexts". It's the data the weekly regression uses to
**shift the ranker weights** away from cross-context
patterns.

### Counter-example as boundary case

When the conditional update rule narrows a memory's
applicability, the new failure is recorded as a
**boundary case**:

```
memory.lesson "method X fails because Y"
  applicability: "input file in format Z"
  examples:
    - success_in: [S3, S7]                # positive examples
    - failure_in: [S12 (output: Z)]      # boundary case
    - test_cases: [{ctx: "Z+other", result: success}]
```

The new failure becomes a **test case** — next time dream
distills this memory, the boundary case is one of the
inputs that constrains the rule's scope. **Counter-examples
teach the boundary** — that is the Schank theoretical core.

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
│   committed bounded │    │                      │    │                      │
│   selection-policy  │    │                      │    │                      │
│   result             │    │                      │    │                      │
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
                                                        exact evidence tally
                                                        → admitted lifecycle
                                                        transition
                                                          ↓
                                                        usage record
                                                        (optional versioned
                                                         display calibration)

┌─────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
│  Memory bank       │  │  Extraction channel   │  │  (same lanes above)   │
│                    │  │                       │  │                        │
│  active (hot —     │  │  4 sources:           │  │                        │
│   injects)         │  │  ① session-end        │  │                        │
│  candidate (bounded │  │    → extractor       │  │                        │
│   exploration)     │  │  ② exit_summary       │  │                        │
│  cold (searchable)  │  │    → hot capture     │  │                        │
│  archived (query    │  │  ③ user "记住 X"     │  │                        │
│   only)             │  │    → authorized       │  │                        │
│  (lessons: no       │  │  ④ scheduled         │  │                        │
│   time-only delete) │  │    distillation       │  │                        │
└─────────────────────┘  └──────────────────────┘  └──────────────────────┘
```

### Three coupling points — only three

| # | From | To | What flows |
|---|------|----|-------------|
| **①** | Conversation (turn loop) | Memory recall | One committed bounded menu/detail product enters C2; retained and dropped references remain auditable. |
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
| **T0** | Conversation + Extraction | Method X fails. `ExitSummary` may asynchronously propose an evidence-bound `AgentLearned` lesson Candidate. It neither blocks nor rewrites the completed Turn. |
| **T1** | Conversation + Memory | An explicit Candidate-exploration request may surface it under a frozen algorithm revision and seed. The committed product records identities and draws; ordinary recall excludes it. |
| **T2** | Effect + Observation | A bounded obligation binds the application and expected outcome. Admitted reality evidence yields an exact verdict and tally update; a verified Candidate may transition to Active. |
| **T3** | Governance + Memory | Risk-action recall is a proposed extension. It cannot enter `AskUser` until a focused Spec admits its purpose, grant, prefix, redaction, fact, and presentation. |
| **T4** | Effect + Observation + Memory | In-scope failure is Falsified. Out-of-scope failure is Neutral and may propose a narrower Candidate; it never silently rewrites the original revision. |
| **T5** | Extraction + Memory | Scheduled distillation consumes an exact Session prefix and watermark, then emits bounded candidates or Noops. No hourly default is implied. |
| **T6** | Evaluation | A future versioned calibration may derive display-only scores from exact tallies. It cannot change authority, eligibility, or lifecycle by itself. |

## Our four moats — what makes this hard to copy

The memory layer is **not** a better kind-registry or recall
algorithm. Those parts are similar to existing systems. The
moat is the **feedback signal source** — what's unique to
Garive is *where the updates come from*.

| # | Moat | Why it's hard to copy |
|---|-------|------------------------|
| **1** | **Real-world reconciliation loop** — the **OutcomeObserver** captures committed tool, test, effect, and authenticated correction evidence. Exact observations update tallies and lifecycle through admitted policy; dialogue or model citations alone never verify a hypothesis. |
| **2** | **Lessons pipeline** — `ExitSummary` produces evidence-bound lesson candidates and the observation loop can test them. Risk-action recall remains a separately gated extension, not an implicit side channel. |
| **3** | **Scope attribution (CBR-style narrowing)** — failure in scope is falsified; out-of-scope failure is neutral and may propose a narrower Candidate without mutating the original. |
| **4** | **Replayable exploration** — candidate exploration freezes the selector revision and seed and commits identities/draws. Thompson sampling, numeric thresholds, and exposure-bias correction remain evaluation-gated policy candidates. |

## Honest gaps — where others are stronger

| Gap | Who's stronger | Garive's stance |
|-----|----------------|-----------------|
| **Knowledge-graph structure** (multi-hop relational inference) | Zep | **Partial** — distillation lineage, supersession and evidence-ref edges are admitted in v1; causal / contradiction / support edges are evaluation-gated pending extractor revisions and measured regression. Entry-level recall remains the primary path; the graph is a complementary traversal. |
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

## Theory map — six research foundations

The memory layer's design rests on **six theoretical
pillars**. Each pillar answers a specific question; together
they give the memory layer its properties (observable /
computable / attributable / adaptive).

| # | Pillar | Where it lives | What it gives |
|---|--------|----------------|---------------|
| **1** | **Beta-Bernoulli Bayesian update** | `R` in `conf = E × R × B × F` (angle ⑧) | **Confidence is a posterior, not a heuristic.** Prior `Beta(1, 1)` (uniform unknown); each outcome shifts the posterior by one count. `R = α / (α + β)` is the posterior mean. Closed-form; no sampling. |
| **2** | **Survival analysis / censored data** | Right-censoring rule (angle ⑧) | **Recall that doesn't reach an outcome provides no information.** `recalled_not_used` and `ignored` are **censored** — they update neither `α` nor `β`. Without this rule, high-traffic memories are falsely penalised for "not being useful" — a death spiral. |
| **3** | **Ebbinghaus / freshness research** | Candidate `F` component (angle ⑧) | Time alone must not tombstone a memory. Only an admitted reality-backed verification may advance `last_verified`; use alone is not verification. The equation remains evaluation-gated. |
| **4** | **Calibration theory (Platt / isotonic)** | Weekly regression on `recall.outcome` (angle ⑧) | **Absolute confidence is calibrated against the world.** The Beta-Binomial posterior gives the right shape; **the coefficients are fit against the historical outcome log.** A memory with `conf = 0.8` should empirically succeed ~80% of the time — Platt scaling or isotonic regression fits this. |
| **5** | **Reciprocal Rank Fusion (RRF)** | Read paths, two-stage retrieval (angle ⑥) | **Failure modes of vector / FTS / recency are complementary.** RRF fuses ranked lists from multiple rankers without requiring score calibration. The constants `k_r` (Cormack k=60) dampen low-rank contributions. Closed-form math. |
| **6** | **Case-based reasoning (Schank)** | Anomaly handling — scope narrowing (this round) | **Counter-examples teach the boundary.** A failure inside scope + similar context → falsify (β + 1). A failure outside scope → narrow the applicability, record the new failure as a boundary case. The fix adjusts the **applicability**, not the conclusion. |

### What each pillar gives us — the design-load table

| Pillar | Without it | With it |
|--------|------------|---------|
| **1. Beta-Bernoulli** | Confidence is a hand-tuned heuristic; it drifts as the bank grows. | Confidence is the **posterior of a Bayesian reliability model**; updates as outcomes accumulate. |
| **2. Censored data** | "Recalled but not used" is treated as "useless"; popular memories get false-failure β. | Right-censored — no information → no update; memory's life is governed by **actual outcomes**, not by whether it got looked at. |
| **3. Ebbinghaus** | Time-decays by age — old-but-still-useful memories lose weight. | Time-decays by `last_verified` — old-but-active memories stay fresh; stale memories get demoted. |
| **4. Calibration (Platt)** | `conf = 0.8` is a number; we don't know if 0.8 means "succeeds 80%" or "succeeds 50%". | Weekly regression against the outcome log calibrates the absolute scale. **A `conf` value is a probability**, not a feeling. |
| **5. RRF** | One ranker wins / one ranker loses; pick one. | Three rankers complement; one fused ranking. Closed-form math, no calibration between rankers required. |
| **6. CBR (Schank)** | A failure = "the memory was wrong" → β + 1; no scope learned. | A failure can be "out of scope" → narrow applicability; the boundary sharpens with use. **Counter-examples teach rules**, not the other way around. |

### The pillars interlock — one feeds the next

```
   case-based reasoning          ← the failure tells us WHERE
            ↑
            │ (counter-examples sharpen the rule's boundary)
            │
   Beta-Bernoulli               ← the failure updates reliability
            ↑
            │ (each success/failure shifts the posterior)
            │
   Platt / isotonic calibration  ← the posterior's ABSOLUTE scale
            ↑
            │ (the regression against observed outcomes)
            │
   Ebbinghaus decay              ← time alone is not decay
            ↑
            │ (last_verified drives F, not "since creation")
            │
   censored data               ← "wasn't recalled" isn't "useless"
            ↑
            │ (right-censoring protects high-traffic memories)
            │
   RRF                          ← multiple rankers, fused
            ↑
            │ (complementary failure modes — vector / FTS / recency)
            │
   recall-event / recall-apply / recall-outcome
        ← the THREE-ROW chain is the unit of evidence
```

> **Each pillar fixes a defect that the previous one
> exposes.** Beta-Bernoulli without calibration → abstract
> scores; calibration without censored data → false β on
> un-recalled entries; Ebbinghaus without right-censoring →
> "ancient" bias; etc. The pillars interlock — each is
> load-bearing because it answers a question the previous
> pillar raises.

### What this means for new contributors

When evaluating a memory-layer mechanism, name the research question it
answers and the evidence needed to admit it. This map is a hypothesis checklist;
M0/M1/M2 and later focused Specs are the contracts.

## End-to-end flow — three swimlanes + three feedback loops

The whole memory system is backed by exact durable provenance. Every admitted
revision binds durable fact references; Runtime's repository projection is
rebuildable state, not a second truth database. The pipeline below is a logical
view of three concerns and proposed feedback loops, not an execution topology.

### The swimlanes

```
┌─────────────────────────────────────────────────────────────────────────┐
│  SWIMLANE 1 — Generation lane (low frequency, async)                │
│  Driven by four triggers that feed the candidate pool:                │
│                                                                         │
│   ┌─────────────┐   ┌───────────────┐   ┌────────────────┐             │
│   │ exit_summary│   │ session-end   │   │ user "记住 X" │             │
│   │ hot-capture │   │ light extract │   │ authorised cmd │             │
│   │ (lessons)   │   │ (episodes)    │   │ (user_declared)│             │
│   └─────┬───────┘   └───────┬───────┘   └───────┬────────┘             │
│         └─────────────────┬┴───────────────────┘                       │
│                           │                                          │
│                           ▼                                          │
│              ┌──────────────────────────────┐                        │
│              │  Four-decision pipeline        │                        │
│              │  ADD / UPDATE / DELETE / NOOP  │                        │
│              │  (Mem0 four-decision model)     │                        │
│              └──────────────┬───────────────┘                        │
│                             │                                          │
│              candidate → evidence-bound (E=0 rejected)            │
│              exact EvidenceTally + lifecycle initialised          │
│              scope, source_session, source_seq attached             │
│                             │                                          │
└─────────────────────────────┼────────────────────────────────────────┘
                              │
                              ▼
                    ┌──────────────────────────────────┐
                    │           MEMORY BANK              │
                    │  ┌─────────────┐                │
                    │  │ candidate    │ (verified →    │
                    │  │              │  active)        │
                    │  │ active       │ (use-feedback)  │
                    │  │ cold         │ (decayed)       │
                    │  │ archived     │ (retired)       │
                    │  │ graduated    │ → knowledge     │
                    │  └─────────────┘                │
                    └──────────────┬──────────────────┘
                              │
                              │  recall (turn-start)
                              │  menu candidate (when requested)
                              │  pull: detail on demand
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  SWIMLANE 2 — Conversation lane (hot path, ≤1 sync action)           │
│                                                                         │
│   ┌──────────┐    ┌──────────┐    ┌──────────────┐    ┌─────────────┐ │
│   │ user.msg │ -> │  derive  │ -> │  4-factor    │ -> │ assemble +  │ │
│   │          │    │ (surface │    │  conf + RRF  │    │ model.invoke │ │
│   │          │    │  cache)  │    │  5 gates     │    │             │ │
│   └──────────┘    └──────────┘    └──────────────┘    └─────────────┘ │
│         │            │             │                  │            │
│         └────────────┴─────────────┘                  │            │
│                           ▼                           │            │
│                  ┌────────────────┐                   │            │
│                  │  surface       │  <──  menu retained by C2 │
│                  │  (model sees) │                  │            │
│                  └────────────────┘                   │            │
│                                                       │            │
│   recall.menu.inject ──────► model sees [mem:abc] ──────────┘            │
│                            cites in reply ──────► model.usage.append │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              │ real-world results
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  SWIMLANE 3 — Observation lane (async, side-channel)                 │
│                                                                         │
│   tool.result / error signature / test pass / fail / verdict / etc.     │
│                              │                                         │
│                              ▼                                         │
│                  ┌─────────────────────────┐                          │
│                  │  OutcomeObserver        │                          │
│                  │  (β-update + outcome)    │                          │
│                  └────────────┬─────────────┘                          │
│                               │                                       │
│         ┌─────────────────────┼──────────────────────┐                │
│         ▼                     ▼                      ▼                │
│   Success → α+1           Failure → β+1          Uncertain            │
│   → active (verified)    → falsify / narrow       → censored           │
│                                                  (no update)         │
│                                                                         │
│   ┌────────────────────────────────────────────────────────────────┐ │
│   │  Per-entry → recall.outcome row →  β update →  conf recompute    │ │
│   │  Per-policy → versioned evaluation → candidate policy evidence │ │
│   └────────────────────────────────────────────────────────────────┘ │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### The three feedback loops

```
     ┌─── verify-loop (per use, async) ─────────────────────┐
     │                                                       │
     │                                                       ▼
   recall.outcome ─────> β-update ─────> conf recompute
                                          │
                                          ▼
                                       recall ranking
                                          │
                                          └──> next recall uses new conf

     ┌─── distillation-loop (configured trigger, async) ────┐
     │                                                       │
     │  dream watermark ─────> episode distil ─────> facts/lessons │
     │                              │                            │
     │                              └──> episode down-weight      │
     │                              └──> quota eviction          │
     │                              └──> audit report            │
     └────────────────────────────────────────────────────────┘

     ┌─── calibration-loop (evaluation-gated, async) ────────┐
     │                                                       │
     │  outcome log ─────> Platt / isotonic regression      │
     │                              │                            │
     │                              ▼                            │
     │  conf = empirical success rate, R → Beta mean,        │
     │  B-weights → regressed vs chain density                  │
     └────────────────────────────────────────────────────────┘
```

### The conversation hot path preserves one context-admission boundary

- Runtime may commit one bounded menu/detail product per admitted iteration;
  C2 decides whether it reaches the surface under shared bounds.
- Detail pull (when the agent decides to recall a specific
  entry) is part of `memory_search` tool — also **tool
  call**, not a separate sync channel.
- `verify`, `distillation`, `calibration`, `promotion` —
  **all** async, all off the hot path.

Asynchronous observation and maintenance never rewrite or block a completed
Turn. A configured recall port may still perform bounded work before its result
is committed and offered to C2; the system does not claim zero latency.

### Coupling between swimlanes

| From | To | What flows | When |
|------|----|-----------|------|
| **Swimlane 1** (generation) | Memory bank | New entries → candidate pool | On trigger |
| Memory bank | **Swimlane 2** (conversation) | committed recall candidate → C2 | When requested by the frozen snapshot |
| **Swimlane 2** | **Swimlane 3** (observation) | `[mem:abc]` citation → outcome link | Per use |
| **Swimlane 3** | Memory bank | β update + state transition | Per outcome |
| **Swimlane 1** | Memory bank → knowledge base | graduation | When verified |

These are the principal semantic flows. Runtime also owns authorization,
configuration, durable commit, recovery, erasure and Knowledge receipts.

### What each piece of the diagram protects

- **Generation lane** — `exit_summary` and `dream`
  produce the raw material; the four-decision pipeline
  enforces admission discipline.
- **Conversation lane** — recall + gate + inject; the menu
  carries provenance; the model decides what to cite.
- **Observation lane** — outcome judgement; **no user
  required**; the three-way observability chain is the unit
  of evidence.
- **Memory bank** — the five-state machine (`candidate` /
  `active` / `cold` / `archived` / `promoted`), orthogonal M0
  supersession/tombstone, explicit quotas and distillation policies.

### What this is **not**

- **Not** a CRDT or consensus algorithm — the bank is
  per-agent, single-writer; the ledger is the source of
  truth; the bank is a derived view.
- **Not** an event-sourced architecture — the bank is **not**
  a log; the ledger is. The bank caches the **distilled
  result**; the ledger keeps the raw material.
- **Not** a vector store with metadata — the bank is
  metadata-first; vectors are an **index** for recall, not the
  store. The store is row-shaped.

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
- Status: **mixed maturity** — M0 and M1-A through M1-H are verified; M2
  remains accepted and active. Knowledge-graph structure, representative longitudinal
  quality, and unpromoted numeric/mechanism proposals remain research.
