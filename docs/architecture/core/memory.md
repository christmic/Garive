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
| Risk-action lesson recall | Future Governance × Memory contract | Research until an exact purpose, authority, redaction, durable fact, and `AskUser` integration Spec is accepted. |
| Numeric schedules, percentages, thresholds, decay and fusion weights | Versioned measured policy | Research until reproducible evaluation admits exact values. |

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

### RRF (Reciprocal Rank Fusion) — the formula

For query `q`, each ranker returns a ranked list:

```
RRF(q, entry) = Σ_r  weight_r / (k_r + rank_r(entry))
```

- `rank_r(entry)` is entry's position in ranker `r`'s list (1-indexed; 0 if absent)
- `weight_r` is the ranker's weight (calibrated; default 1.0 each)
- `k_r` is a constant per ranker that dampens the contribution of low-rank items (Cormack's constant k=60 is the canonical default)

The fused ranking is `RRF_score(q, entry)` summed across all
rankers. Top-k is the **k entries with the highest fused
RRF score**. This is **closed-form math** — no LLM in the
loop during fusion.

### Two-stage — coarse + fine

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

### Query expansion — multiple phrasings

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

The `llm_expand` is **lightweight** — the LLM is asked to
produce "3 different ways a user might phrase this query",
not to extract anything substantive. The expansion is **the
LLM's** contribution; the **retrieval and ranking** are pure
math.

If query expansion is too expensive, a **zero-LLM fallback**:
use the entry's known metadata — `mtype`, `confidence`,
`last_verified`, `source_session` keywords — as expanded
query forms. **Pure math**, no LLM.

### Recall surface — the menu (always-on)

The recall menu is the **single thing** the framework injects
into the surface every turn:

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
- High-confidence items first
- Sorted by `mtype` × `last_used`

The **menu's only job** is to make recall possible: the
agent sees **what's available** and decides **whether to pull**.
Detail is on demand (recall tool / knowledge lookups).

### Read-path audit — three observation rows per recall

Every read goes through the three-way observability
contract (angle ⑧):

| Row | Captures |
|-----|----------|
| `recall.event` | Which entries were retrieved, fused RRF score, `E × R × B × F` per entry |
| `recall.apply` | Which entries the model actually cited (`[mem:xxx]`) in its reply |
| `recall.outcome` | Did the cited entry match reality? → β +1 / -1 / censored |

The `recall.outcome` row is the **weekly calibration input**
for `R` (Beta-Binomial) and for the ranker weights.

### Recall quality — measured by the feedback loop, not by offline eval

Recall precision is **not** measured by a separate
benchmark. It is **measured by the same feedback loop** that
calibrates `R` — the chain

```
recall.event → recall.apply → recall.outcome → β update → conf recompute → ranker weight recalibrate
```

**The link** is the unit of evidence:

| Link state | Meaning | What it does |
|-----------|---------|--------------|
| `event` only | The model was shown the entry but did not cite it | **Censored** — no signal |
| `event + apply` (no outcome yet) | The model cited it but the world hasn't checked | **Pending** — outcome arrives |
| `event + apply + outcome.success` | The model cited it AND the world confirmed | **Confirmed** — β +1 |
| `event + apply + outcome.failure` | The model cited it AND the world contradicted | **Falsified** — β +1 (failure counts) |
| `event + apply + outcome.conflict` | Cited entry contradicted another cited entry | **Conflict** — both entries lose β, high-conf wins on recall |

The chain's **density** is the signal:

- High density of `event + apply + success` chains per query
  pattern → that query pattern has good recall precision.
- Low density → recall is failing on that pattern; the
  query expansion + ranker weights need adjustment.

> **The loop is the metric.** Offline recall-precision@k
> benchmarks are a **snapshot**; the production feedback
> chain is a **stream**. The stream is the ground truth —
> the snapshot is for catching regressions when the stream
> drifts.

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
                  β + 1 (success)  or  β − 1 (failure)
                       │
                  conf recomputed via formula
                       │
                  state-machine transition (candidate → active)
```

Three **observability points** are mandatory:

1. **Recall-side** — which entries were retrieved, with
   conf. `recall.event` row in memory.db.
2. **Apply-side** — which entries the model actually used
   (`[mem:abc]` citation). `recall.apply` row.
3. **Output-side** — which β updates happen, against which
   sessions. `recall.outcome` row.

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
