# Agent Loop — Two-Layer Driver / Turn / Iteration

> **Three nested layers**: a long-running `agent_loop` (driver),
> each user message starts an `agent_turn` (round), and each
> round is one or more `iteration`s of the **derive → invoke
> → judge → run** loop. The ledger is the **single source of
> truth** for state; the LLM never sees raw ledger, only a
> **budget-shaped surface** derived from it. **Governance is
> queried, not invoked** — every model-intent is judged, and
> the only way a round pauses is `AskUser`.

This is **deliberative**, not a spec. Type names, method
signatures, and exact field shapes will land in
`spec/design/agent-loop.md` once this design settles.

> **Heads-up: this document is a *possible mechanism*, not
> final code.** Every pseudo-code snippet, every field name,
> every threshold (the 0–3 / 4–10 / >10 tier boundaries; the
> 3 / 10 / 30 Edit-class numbers; the eager / late / never
> eviction triggers) is a **draft design choice** that may
> change as the slice lands. The *stable* part is the loop
> **shape**:
>
> - `agent_loop` / `agent_turn` / `iteration` nesting
> - Ledger as single source of truth
> - Governance as a queried port
> - `derive` as incremental + stateful
> - Three-pass `assemble` (tier / evict / format)
> - Mechanism / policy split
>
> The *specifics* — exact field shapes, threshold numbers,
> per-tool policy profiles — are **not** committed to. Treat
> this as a starting point, not a contract. When the slice
> lands, this design gets re-checked against what the code
> actually wants, and either the code or the design moves.

## Context

Garive's agent runtime has three properties the loop must
uphold:

1. **Recoverable.** A user message may pause the round for
   hours (human approval, network wait, rate limit). The
   runtime must pick up exactly where it left off.
2. **Bounded.** A round must not blow past token or wall-clock
   budgets. The model never sees a context window it can
   overflow; the loop owns the budget.
3. **Governed.** Every side-effect the model asks for is
   reviewed before execution. The reviewer is the only path
   to effect; the model is a *proposer*, never an *actor*.

A naïve single-loop driver satisfies none of these: a crash
loses state, an over-budget call blows the context window,
and an ungoverned call to a tool is a security hole.

## Options Considered

### A. Single big loop, no resumability

```
while True:
    surface = read_recent_messages()
    reply = model.invoke(surface)
    execute(reply)
```

Rejected. No way to pause; no place to keep state across
restarts; no budget discipline; no governance.

### B. Per-call checkpoints

Each model invocation writes a checkpoint; on restart we
re-load the last checkpoint.

Rejected. Checkpoints are eventually consistent with the
ledger; reasoning about "what state are we in" becomes a
two-source problem. The ledger IS the checkpoint; we don't
need a second one.

### C. Two-layer driver + turn + ledger (CHOSEN)

```
agent_loop  (driver, 1 instance, long-lived)
  └── agent_turn  (round: one user message → one final answer)
        └── iteration × N  (derive → invoke → judge → run)
```

This document.

### D. Event-sourced actor model

Treat every state change as an event; replay events to
recover state.

Considered. Close cousin to option C — the ledger is already
event-sourced. We chose C because the actor-model framework
would add a dependency and a runtime; the ledger + driver
loop gives us the same property for less.

## Decision

### The Nested Structure

```
agent_loop (driver, 常驻, 1 个)
  └─ agent_turn (回合: 一条用户消息 → agent 最终答复)
        标识: ledger 里的 turn 标记 (turn_id / seq 区间)
        └─ iteration × N  (while 一圈: derive → invoke → judge → run)
              模型还要调工具, 继续转
              iteration N → termination.done → Done
```

A round is **one** `agent_turn`. A round is **not** over until
`termination.done(state)` is true. A round may pause (return
`Suspended`) and resume later — still the **same** `turn_id`,
not a new round.

### The Core Skeleton

The following pseudo-code preserves the user's original
draft (intentionally, with its original wording and rough
edges) so the rationale stays traceable.

```python
# ◎ 入口协议：恢复时从账本推导位置
if entry is Resume:
    pos = derive_position(ledger)
    # 从 pos 对应的阶段进入
    #   - 未配对的审批应答     → 接受执行 / 拒绝
    #   - 未配对的 calls       → 补齐 (synthesize 缺失的 tool 结果)

while not termination.done(state):

    # ① 预算感知推导（预算是 derive 的输入）
    surface = ledger.derive(kinds_for(this_mode), budget)
    if surface.needs_summary:
        #   纯推理报告：需要新摘要
        ledger.append(summarize(prefix))          # 唯一不纯步骤
        #                                                   (LLM 摘要)
        ledger.append(rewrite_directive{
            覆盖: prefix.seq_range,
            代际: +1,
        })
        surface = ledger.derive(kinds_for(this_mode), budget)

    # ② 组装 + 调用（模型永不见超窗；唯一重试拥有者）
    request = assembly(surface)
    reply   = model.invoke(request)
    ledger.append(reply.items)                    # 每条目带 kind + provenance

    # ③ 效应：治理是被问询的端口
    for intent in reply.intents:
        verdict = governance.judge(intent)
        ledger.append(verdict)

        match verdict.decision:
            Approve:
                effects = executor.run(intent)
                ledger.append(effects.items)

            Deny(reason):
                ledger.append(tool_result.rejected(reason))
                #   拒绝原因回喂模型（不中断）

            ApproveWithRewrite(x):
                effects = executor.run(intent.with(x))

            AskUser(question):
                ledger.append(approval_request{
                    question,
                    引用: verdict.seq,
                })
                return Suspended                 # 唯一的中断原因：需要人介入
```

### Layered Semantics

| Layer | Identity | Lifetime | Concerns |
|-------|----------|----------|----------|
| `agent_loop` | one per process | forever | event source, dispatch, lifecycle |
| `agent_turn` | one per user message | until `Done` or `Suspended` | entry protocol, resume from ledger |
| `iteration` | one per `while` pass | sub-millisecond to minutes | derive, invoke, judge, run |

### The Ledger's Role

The ledger (a separate design — `docs/architecture/ledger.md`,
TBD) is the **single source of truth** for round state. It
records:

- Every model `reply` (items with `kind` and `provenance`).
- Every governance `verdict`.
- Every `executor.run` effect.
- Every `tool_result.rejected(reason)` — so the next iteration
  sees why a tool was denied.
- Every `rewrite_directive` — so future derives know an old
  summary is stale.
- Every `approval_request` — so a Resume can re-attach the
  human's answer.

The ledger has **one impure step** — `summarize(prefix)` —
because summarisation requires the LLM. Every other write is a
pure projection over the existing ledger.

### The Turn State

Per-`agent_turn` runtime state — the small set of values the
loop reads and writes **inside the `while`**. The state is
**per-turn** (lives across Suspended / Resume), not per-loop
or per-process. The state and the ledger are deliberately
separate: the **ledger** is the durable, replayable record
("what happened"); the **state** is the in-flight loop
control ("what's the loop doing right now"). Anything the
ledger can answer, the state must not.

| Field | Type | What it holds |
|-------|------|---------------|
| `iteration_count` | `u32` | Iterations since the turn started. Bumped once per `while` pass. |
| `phase` | enum `{Running, AwaitingApproval, AwaitingConfirm, Done, Suspended, Failed}` | Where the turn is right now. Drives the entry protocol on Resume. |
| `termination_reason` | enum `{NotDone, Answered, NoMoreToolCalls, BudgetExhausted, CircuitBroken, GovernanceDenied, Suspended, Failed}` | The verdict that closes the `while`. Set by the loop, never by the model. |
| `tokens_used` | `{in: u32, out: u32, total: u32}` | Cumulative across all `model.invoke` calls in this turn. Compared against `budget.tokens` on every iteration. |
| `wall_clock_used_ms` | `u64` | Wall-clock from `agent_turn` entry. Compared against `budget.wall_clock_ms`. |
| `last_confirm_response` | enum `{None, Approved, Denied}` | The human's answer to the most recent `AskUser`. `None` until a confirm is requested; `Denied` keeps the round alive (the denial is fed back to the model as a `tool_result.rejected`). |
| `circuit_breaker` | struct `{consecutive_failures: u32, last_failure_kind: enum, opened_at_ms: Option<u64>}` | Local throttle. Increments on `model.invoke` / `executor.run` failures; trips to `open` when it crosses a threshold. While `open`, the loop pauses / escalates rather than retrying. |

The state is **explicitly typed** — fields are named, not a
bag of attributes. Anything the loop wants to read or write
goes through this struct. The list above is a **starting
shape**; the actual field set will land with the slice and
must stay small (a dozen fields, no more — anything bigger
is the ledger pretending to be the state).

### Who Reads / Writes the State

| Field | Read by | Written by |
|-------|---------|------------|
| `iteration_count` | loop (`while`, telemetry), `termination.done` | loop (increment per pass) |
| `phase` | loop (entry protocol), telemetry, `governance.judge` | loop (phase transitions: `Running → AwaitingApproval → Suspended → AwaitingConfirm → Running → ...`) |
| `termination_reason` | loop (final write to ledger), telemetry | loop (when `termination.done(state)` becomes true) |
| `tokens_used` | loop (`ledger.derive(kinds, budget)` budget check), telemetry, `circuit_breaker` | loop (sum from `reply.usage` after each `model.invoke`) |
| `wall_clock_used_ms` | loop (budget check), telemetry, `circuit_breaker` | loop (sampled at the top of each iteration) |
| `last_confirm_response` | `governance.judge` (sees the answer on Resume), `assembly(surface)` (re-attaches it into the next prompt) | loop (after Resume's synthetic `entry` is processed) |
| `circuit_breaker` | loop (`while` body — guards `model.invoke` and `executor.run`), telemetry | loop (on each failure / cooldown tick) |

Three rules:

1. **The loop is the only writer.** `governance.judge`,
   `executor.run`, `assembly`, and `summarize` are pure
   functions — they take state as input and return values;
   they never mutate it.
2. **Reads happen at the top of an iteration**, not scattered
   throughout it. A `governance.judge` call sees the state as
   it was when the iteration started; any update from this
   iteration's `executor.run` lands **before** the next
   iteration's read.
3. **`state` survives Suspended → Resume.** It is part of the
   `agent_turn` resumable payload (along with the ledger
   segment for the turn). On Resume, the state is loaded as-is
   and the loop resumes from the `phase` it was in.

### State vs Ledger

| Question | Answer source |
|----------|---------------|
| "How many tokens did the round spend?" | `state.tokens_used` (fast read; loop-local) |
| "Which `model.invoke` calls happened, in order, with what output?" | ledger (durable; replayable) |
| "Are we currently waiting on the user?" | `state.phase` |
| "What was the human's last answer?" | `state.last_confirm_response` |
| "Why was this tool denied?" | ledger (the verdict + reason) |
| "Why is the loop refusing to retry?" | `state.circuit_breaker` |
| "What's the round's termination status?" | `state.termination_reason` (this turn's outcome) |

If the question can be answered from the ledger, it lives in
the ledger. State is **only** for in-flight loop control —
nothing that needs to outlive the turn, nothing the ledger
already knows.

### The LLM Never Sees the Ledger

`ledger.derive(kinds, budget)` projects the ledger into a
**budget-shaped surface** — a token-bounded slice of round
history. The LLM is given only this surface. This guarantees:

- The model cannot be poisoned by raw ledger entries.
- The model cannot ask the runtime to do something the
  runtime didn't already know.
- The context window is enforced before the call, not after.

### Derive in Detail (common path)

> **One-liner.** `derive` is a **pure function** —
> `(current_surface, new_entries, masking_timeline,
> projection_args) → derived_surface`. No side effects, no
> ledger writes, no I/O. The **caller** owns the cache and
> applies the returned `derived_surface`. `assemble` is
> **dialect serialisation** — it turns `derived_surface`
> into the exact bytes the provider will see.

This split is **load-bearing** for testability, replay, and
boundary hygiene. A pure `derive` is trivial to fuzz
(input space, output check) and to replay against historical
ledger snapshots. A dialect-aware `assemble` is the
**only** place provider differences live. Anything that
doesn't fit cleanly into one of these two responsibilities is
in the wrong layer.

`ledger.derive(kinds, budget)` is the projection the loop
calls every iteration. **It is not a full-ledger scan.** It
is an **incremental update over a cached surface**: when the
ledger hasn't been compressed, derive appends the new entries
to the cache and returns; when a `rewrite_directive` lands, the
rest of `derive`'s responsibilities kick in.

The **purity contract** is what makes the change-locality
matrix work. Specifically:

- `derive` **does not mutate** anything. The "cache" is the
  caller's pre-existing state; `derive` reads it, transforms
  it, returns a new surface. The caller then atomically
  swaps the cache reference.
- `derive` **does not write** to the ledger. The ledger is
  the source of truth; `derive` is a *read* of the ledger
  (through the cache). Writes are `summarize`,
  `governance.judge`, `executor.run`, the loop itself
  (appending `model.usage` and the per-round `ops_log`
  receipt).
- `derive` **does not call out** to the LLM, the executor,
  or any other I/O. The whole pipeline is a function of
  `current_surface` + the masking timeline + projection args.

```
def derive(current_surface, new_entries, masking_timeline,
          projection_args) -> DerivedSurface:
    """Pure function. No side effects. No I/O."""
    ...
```

This shape is why the **change-locality matrix** (below)
holds: every entry in the matrix lands in exactly one column
— `derive` *or* `assemble` — because every legitimate change
is either a content decision (derive) or a serialisation
decision (assemble). When a change spans both, it is **two
changes** (a kind+schema commit + an assemble dispatch
commit), not one.

`derive` is the **common path** — every consumer
(`PROMPT_FOR_MODEL`, `SUMMARIZE_INPUT`, `GOVERNANCE_INPUT`,
`FORK_BRANCH`, `AUDIT_REPLAY`) shares it. What each consumer

### Budget Projection (anchored to `model.usage`)

The earlier budget design had `derive` estimating the
surface's token cost and asking the loop to truncate when
estimated > budget. That was **pure estimation** — no ground
truth. The new design **anchors** the budget to the real
cost reported by the provider.

#### The formula

```
budget_projection(this_round) =
    last_actual       # anchor: real cost from previous round's model.usage
    + est(this_round.current_surface)      # today's surface, estimated
    - est(this_round.prev_actual_surface)   # yesterday's surface, estimated
```

- `last_actual` is **`model.usage` from the previous round**:
  `tokens_in + (cache_read + cache_write)`. This is the
  provider's *billed* number, not an estimate.
- `est(...)` is `derive`'s budget-aware estimate of the
  surface's token cost, in the same units the provider
  would count.
- The **difference** is the signal — if the new surface has
  more entries than the old, the budget grows; if it has
  fewer (compaction), the budget shrinks. The estimate's
  absolute accuracy doesn't matter, only its *change*.

#### Budget vs cost (two layers, both from `model.usage`)

- **Budget** = `tokens_in` (pre-cache, what the model
  receives as the prompt). Used to decide whether the
  surface fits the model's window.
- **Cost** = `cache_read + cache_write` (post-cache
  effective). The dollars the user actually pays.
- Both come from the same `model.usage` row; they're just
  different fields. The `tokens` payload carries both
  nested:

```python
class Tokens:
    in:          u32   # pre-cache input (budget gate)
    out:         u32   # output (the reserve)
    cache_read:  u32   # post-cache hit (cost reduction)
    cache_write: u32   # post-cache miss → write (cost)
    total:       u32   # in + out (computed)
```

#### Boundary conditions (fallback to pure estimation)

The anchored projection has **three fallback paths** to
pure estimation, when the anchor is unavailable or
invalid:

| Condition | Behaviour |
|-----------|-----------|
| **First round** of a session (no `last_actual`) | Pure `est(surface)` — the model.usage anchor doesn't exist yet. |
| **`model_id` changes mid-session** | The previous anchor was for a different tokenizer; the diff is meaningless. Reset to pure `est(surface)`. The next round will have a fresh anchor. |
| **Provider emits a malformed `model.usage`** (missing fields, garbage cache count) | The runtime flags the round's budget as **unanchored** and falls back to pure estimation. The error is logged to `ops_log`. |

#### Compression / undo delta absorption

The formula is **symmetric** — it doesn't need special
handling for compression or undo:

- **Compression** shrinks the surface → `est(new) <
  est(old)` → delta is **negative** → projection is **lower**
  than the anchor. The cache hit on the prefix + the
  smaller new part both count toward a smaller bill.
- **Undo** grows the surface (re-extends a previously
  masked suffix) → `est(new) > est(old)` → delta is
  **positive** → projection is **higher**. The model
  processes more entries; the user pays for them. The undo
  is **honest** in cost.
- **Branch verdict (adopt / discard)** — adoption is
  structurally similar to undo (surface grows); discard is
  similar to compression (surface shrinks). The formula
  handles both transparently.

#### Audit link

`model.usage` carries a `ref = {session, uid}` to the
`loop.receipt` entry that described the round's surface
(`loop.md` "Derive Receipt"). An audit query joins them:

```sql
-- "Show me what the model saw and what it cost."
SELECT r.notes AS saw, u.tokens AS paid
  FROM entry r
  JOIN entry u ON u.ref == r.uid
 WHERE u.kind == 'model.usage'
   AND r.kind == 'loop.receipt'
   AND r.round_id == ?
```

The audit query reconstructs **"what the model saw"** from
the receipt's `notes` (the cached surface structure) and
**"what it cost"** from the `model.usage` row. Together
they are the **full round record**.

#### Pending decisions (recorded, not resolved)

These are open for later iteration; the design above
**records** them but does not commit to a choice:

- **D1. Error propagation across the diff.** The formula
  computes a *delta* of two estimates, each with ~1 %
  error. The diff's noise is bounded, but how the noise
  propagates (additive, multiplicative) is not modelled.
  *Resolution deferred to a future iteration.*
- **D2. First-round and model-swap estimation precision.**
  When the anchor is unavailable, the fallback uses
  `est(surface)` — but at **which** error tolerance? 5 % (T4
  level) or 1 % (Dim 4a level)? *Resolution deferred.*
- **D3. Cross-model frequency.** Switching `model_id`
  mid-session is **uncommon** (the user implies); the
  fallback path is correct, no optimisation is warranted.
  *No action.*
- **D4. Provider field normalisation.** Anthropic /
  OpenAI / Gemini name cache fields differently. The
  core layer assumes a **unified** `tokens` schema; the
  per-provider **adaptation layer** (in the runtime
  provider abstraction) translates provider-specific
  fields into the unified shape. *Core layer stays
  simple; per-provider mapping is below.*
- **D5. Over-budget behaviour.** When
  `budget_projection > budget`, what does the loop do?
  Truncate tail, abort the round, ask the user, split into
  two calls? The decision is **not yet made**. *Recorded
  as a pending design choice.*
- **D6. Receipt ↔ `model.usage` link timing.** Does the link
  happen at **append time** (one transaction writes both)
  or **at query time** (join by `round_id`)? Append-time
  is stronger (one place to look) but requires `assemble`
  to know the receipt's `uid` before it exists. Query-time
  is simpler but the link is **eventual**. *Resolution
  deferred.*
- **D7. `state.tokens_used` semantics.** Is the
  per-round running total **summed across rounds** (full
  cost) or **the latest round only** (one-round cost)? The
  current field name is ambiguous. *Resolution deferred.*

**These decisions do not block the design** — each is
resolvable in its own follow-up commit. The design above
is the **stable** structure: anchor + delta + budget/cost
separation. The seven items above are the **policy** layer
on top.

### Change-locality (which side moves for which change)

The split between `derive` and `assemble` is a **boundary
test**: every legitimate runtime change should touch **one
side only**. A change that crosses both sides is a smell —
either the boundary is in the wrong place, or the change is
two changes pretending to be one.

| Change | Side that moves | What it does there |
|--------|----------------|---------------------|
| **Switch provider** (Anthropic ↔ OpenAI ↔ Gemini) | `assemble` | The provider translation step (responsibility 2) changes; the cache marker (responsibility 5) changes; the role mapping (responsibility 1) changes; **nothing in `derive` changes**. |
| **Add a new provider** (e.g. a local llama.cpp) | `assemble` | A new `provider.render()` + `count_tokens()` + `mark_cache()` implementation. The dialect plug-in lives entirely in `assemble`. |
| **Provider adds a new cache mechanism** (e.g. Anthropic extends `cache_control` with a 1h TTL) | `assemble` | The cache marker step (responsibility 5) updates; the prefix-stability check updates. **`derive` doesn't need to know about cache TTL.** |
| **Provider fixes a counting bug** (off-by-one in token estimation) | `assemble` | The budget step (responsibility 4) uses the provider's real counter. Fixing the counter fixes the budget. |
| **Switch mode** (coding agent ↔ chat agent) | `derive` | The masking-instructions walk (step 3) changes — which kinds are masked, when, why. Tier boundaries (step 5) change — what counts as "old" / "fresh" in this mode. The cache prefix is mode-agnostic; **assemble doesn't change**. |
| **Add a new masking instruction** (e.g. a "redact-on-share" kind for when the user shares a session externally) | `derive` | New kind in `spec/proto/`; new branch in the masking walk. The `assemble` projection treats the new kind like any other masked entry. |
| **Adjust a tier policy** (e.g. raise `tool.result` from tier-1 to tier-0 for the most recent 5 iterations) | `derive` | The tier-policy table (`.agents/loop.md` "Per-tool Policy Profiles") updates. `assemble` reads the tier from the cache and renders accordingly. |
| **Change the kinds of a projection** (e.g. `GOVERNANCE_INPUT` now needs `text.user` too) | `derive` | The projection's `kinds` set updates. `assemble` simply doesn't filter them out. |
| **Add a new kind** (`loop.receipt`, a new `compaction.*` flavour, etc.) | `derive` | Add the kind to the kind catalog; add a body schema in `spec/proto/`. The masking walk and tier policy decide how the new kind is projected; `assemble` doesn't need to know. |
| **Change the pinned block** (which kinds are always-loaded) | `derive` | The pinned set updates. `assemble` still renders the pinned block as the head; only the *contents* of the head change. |
| **Change the layout mode set** (add a new mode like `striped` for A/B testing) | `assemble` | The layout function updates. The pinning and tier decisions in `derive` don't change. |
| **Add a new projection** (a new view like `DIFF_VIEW` for round-vs-round comparison) | `derive` + `assemble` | A new branch in the dispatch; but the *content decisions* are the existing rules, only the *serialisation* is new. |
| **Change the delta-fragment policy** (e.g. seen part is the *last 2* iterations instead of the *last iteration*) | `derive` | `last_seen_seq` definition updates. `assemble` reads it; the new boundary is the new cache key. |
| **Change the loop boundary** (Suspend/Resume rules; `phase` machine) | `derive` | `state.phase` transitions update. `assemble` doesn't observe `phase`. |

The matrix's **shape** matters more than its size. The
invariant: every entry in this table fits in **one column**
— the cell under "Side that moves". When a change needs
both `derive` *and* `assemble`, the change is **two changes
disguised as one**, and the breakdown is the first thing to
do.

#### Where the matrix fails

Two cases cross the boundary cleanly (i.e. they belong to
both sides by design, not by mistake):

- **Add a new kind** (e.g. `privacy.share_redact`) — both
  sides move, but they move in **separate commits**: the
  kind + body schema is one commit in `spec/proto/`; the
  masking-walk branch that handles it is a second commit in
  `derive`; the role mapping that renders it is a third
  commit in `assemble`. Three commits, three reviews, one
  feature.
- **Add a new projection** — same pattern: the projection's
  *content* (kinds filter, masking rules) lands in `derive`;
  the projection's *serialisation* (layout, role map, cache
  marker) lands in `assemble`. Two commits, two reviews,
  one feature.

Anything that **can't be split** along the boundary
(content / serialisation) is a signal that the boundary
itself is in the wrong place. When that happens, the
fix is to **rename or move the line**, not to relax the
rule.
does *with* the cached surface is **assemble**'s job.

### Derive pipeline (six steps, in order)

`derive` is a **fixed-order pipeline** of six masking /
shrinking / projection steps. Each step consumes the output
of the previous one. The order matters: a step earlier in
the pipeline can hide rows that a later step would otherwise
shrink; a step later in the pipeline can shrink what an
earlier step's masking would not have hidden.

```python
def derive(surface, projection, budget):
    # 1. session.undo — masks suffix after a turn boundary
    surface = apply_undo(surface)

    # 2. branch.verdict — masks branches not adopted
    surface = apply_branch_verdict(surface)

    # 3. compaction.rewrite — masks prefix; reads compaction.summary
    surface = apply_compaction_rewrite(surface)

    # 4. privacy.redact — masks individual ranges / uids
    surface = apply_redaction(surface)

    # 5. Clipping rules — tier (age), volume (token size), kind policy
    surface = apply_clipping_rules(surface, budget)

    # 6. Kinds filter + pinned — categories that surface sees
    surface = project_by_kinds(surface, projection.kinds,
                                pinned=ALWAYS_LOADED)

    return surface, DeriveReceipt(
        applied_masks=[...],
        clipped=...,
        skipped=...,
        token_breakdown=...,
    )
```

**Why this order:**

| Step | Rationale |
|------|-----------|
| **1. session.undo** | Undo masks the **largest possible range** (the entire suffix after `target`). Doing it first means the later steps don't waste budget shrinking rows the user has already said to forget. |
| **2. branch.verdict** | Discarded branches are masked wholesale. Branch verdict operates on `branch_path`; doing it after undo (which is *not* branch-scoped) means an undo across branch boundaries still works correctly. |
| **3. compaction.rewrite** | The prefix has been summarised; the prefix rows are masked. After this, only the tail of the round is in the surface, plus the `compaction.summary` row itself (always loaded). |
| **4. privacy.redact** | Individual entries or ranges. The redacted entries are masked **as redacted placeholders**, preserving identity (`seq`, `kind`, `wall_ts`, `provenance`, `pair_ref`, `ref`) but not the body. |
| **5. Clipping rules** | Per-tier (age) + per-volume (token size) + per-kind (tool policy) shaping. These are **shrinking** operations on the surviving entries — they don't change *which* entries, they change *how much* of each entry. |
| **6. Kinds filter + pinned** | The projection. `kinds` selects which categories the projection cares about; `pinned` (always-loaded) is *added*, never replaced. The result is the surface. |

The masking family (steps 1–4) operates on **range**; the
shrinking family (step 5) operates on **content within a
range**; the projection (step 6) operates on **category**.
The three operate on different axes, so the order is
naturally forced: range first (biggest area), then content
within range, then category.

### Assemble (per-projection reshape)

`assemble(surface, projection, last_seen_seq?)` is the stage
that **serialises the derived surface into the exact bytes
the consumer sees**. It runs after `derive`, before
`model.invoke`. Where `derive` decides **what** the model
sees, `assemble` decides **how** — projection, layout,
delta fragment, and the final `pinned / seen / new` segment
shape.

> **One-liner split.** `derive` is **content selection** —
> provider-agnostic. `assemble` is **how to say it** — the
> serialisation dialect, including provider-specific layout.
> Content choice and serialisation format are different
> problems; this split keeps them clean.

#### What assemble does (five responsibilities)

`assemble` is **purely serialisation — the provider's
dialect for saying what `derive` already chose**. The
content decisions (masking, kinds filter, dedup, pinned
injection, position clip) are `derive`'s job (see
"Derive pipeline (six steps)" above). `assemble` only has
authority over **how to emit** what `derive` already chose.

The five responsibilities below run in this order: **role →
provider → layout → budget → cache marker**. Layout is
*between* provider and budget because the budget count is
post-layout; cache marker is *last* because the marker lands
on the final byte boundary.

1. **Role mapping + message shaping** — convert the
   internal model (kinds → roles) to the provider's role
   vocabulary: `text.user` → `user`, `text.assistant` →
   `assistant`, `harness.feature` → `system` (or `user`,
   depending on the harness signal), `tool.call` +
   `tool.result` → `assistant` `tool_use` block + `user`
   `tool_result` block (must be **paired**, in the right
   order, in the same turn group). The role map is the
   **structural skeleton** of the prompt.

2. **Provider translation** — the dialect-level conversion.
   Each provider has its own shape: Anthropic uses a
   separate top-level `system` array and `cache_control`
   markers; OpenAI folds system into a `developer` role
   (newer models) or a top-level `system` field (older
   models); tool-call argument schemas differ in strictness;
   role-alternation rules (`user → assistant → user → ...`)
   differ; some providers accept `tool` blocks inside
   `assistant`, others don't. `assemble` carries a `provider`
   argument and dispatches.

3. **Layout positioning** — *does not reorder.* `derive`
   hands `assemble` a `pinned` block and a `body` stream
   already organised by attention value. `assemble` just
   **places** them: system prompt first, then pinned block,
   then the body stream in `derive`'s order. Reordering is
   `derive`'s job; `assemble` only positions. See
   "Layout-aware assembly" below.

4. **Budget** — *where the planned budget meets the real
   one.* `derive` operates on a `budget.tokens` number; that
   number is **estimated** (tokenisers differ from provider
   counters). `assemble` re-counts the assembled prompt with
   the **provider's real tokeniser** (or the `model.usage`
   feedback from the previous round). If the real count
   exceeds the budget, `assemble` **drops the tail** of the
   `new` part (the last entries are the cheapest to lose — the
   model has already digested the head / middle from the
   cache). The output budget is **reserved upfront**
   (`max_tokens` on the request) so the model has room to
   reply. Budget vs actual divergence is handled **at this
   layer** because it's a serialisation concern (the wire
   size), not a content concern.

5. **Cache marker** — *the byte-stability guarantee, made
   real.* Once the prompt is laid out, `assemble` places
   the provider's cache-control marker on the **stable
   prefix** — the byte boundary the cache key depends on.
   For Anthropic, that's the end of the `system` block plus
   the pinned block plus the start of the `seen` part. For
   OpenAI, it's the implicit boundary the auto-cache infers
   (often the first 1024 tokens). `assemble` also **asserts
   the prefix bytes are stable** vs the previous round
   (e.g. the `system` block has not changed; the pinned
   block is byte-equal to the prior round's); a divergence
   is a **bug in `derive`** (it broke the cache rule) and
   `assemble` reports it before the call.

```python
def assemble(surface, projection, last_seen_seq=None,
            provider=PROVIDER_DEFAULT, round_id=None):
    # surface is already: masked, deduped, kinds-filtered,
    # position-clipped, pinned-injected, last_seen_seq split

    # 1. role mapping
    role_map = role_map_for(provider)
    messages = [role_map.to_message(e) for e in surface.body]

    # 2. provider translation
    payload = provider.render(messages, surface.pinned)

    # 3. layout positioning
    payload = position_sections(payload, projection.layout_mode)

    # 4. budget enforcement (real-token count + tail drop)
    real_tokens = provider.count_tokens(payload)
    if real_tokens > surface.budget.tokens:
        payload = drop_tail_to_fit(payload, surface.budget.tokens,
                                  real_tokens)

    # 5. cache marker (assert prefix stability + place marker)
    assert_prefix_stable(payload, last_seen_seq, provider)
    payload = provider.mark_cache(payload, where="after_pinned")

    return Assembled(payload=payload,
                     real_tokens=real_tokens,
                     surface=surface,
                     receipt=DeriveReceipt(...))
```

`assemble` reads only the `Surface` produced by `derive`. It
never walks the ledger, never decides which entries to keep,
never decides which kinds go on the surface — all of that is
`derive`'s job. `assemble`'s only job is to **say** what
`derive` decided, in the **provider's dialect**, under the
**real-token budget**, with the **cache marker** in the right
place.

#### Delta fragment and prompt-cache prefix

The provider prompt cache hits when the next iteration's
prefix is **byte-for-byte** the same as the previous
iteration's. `assemble`'s delta fragment is what makes
that work.

**The contract:** every iteration's prompt is structured
as

```
[pinned block]  [seen part = previous prompt's tail]  [new part = delta since last_seen_seq]
```

The **seen part is byte-for-byte the previous prompt's
tail**. The pinned block is byte-for-byte the previous
prompt's pinned block. The only thing that changes is the
**new part** at the end. The provider cache hits on
`pinned + seen`; the model only needs to ingest the
`new` part.

The **first iteration** of a round has no `last_seen_seq`,
so the entire body is `new`. The second iteration
truncates; the third and beyond are **append-only**
relative to the second. The `seen` part grows; the `new`
part shrinks. Eventually, **the `new` part is empty** and
the model only sees the prefix + a "no new entries"
marker — at that point the provider cache is *fully*
hitting.

The **delta boundary** (`last_seen_seq`) is recorded in
the round's `state` (see `loop.md` "The Turn State"). On
Suspend, the boundary is part of the resumable payload;
on Resume, the loop continues from the same boundary, and
the next `assemble` uses the same boundary — the
provider cache's stable prefix **survives a Suspend /
Resume cycle** as long as the suspended round is the one
being resumed.

Three properties of the delta fragment that
**constitutional** to its design:

- **Append-only within a round.** The `seen` part is
  always the same bytes; the `new` part is always appended
  at the end. The pinned block is always the same bytes.
  This is the same `Derive Stability` Rule 1 applied to
  the assembled prompt.
- **Boundary-anchored.** A `compaction.rewrite` resets
  the cache; the `seen` part is replaced with a
  `compaction.summary` row that the model has not seen
  before. The `new` boundary starts at the rewrite point.
- **Survives Suspend/Resume.** `last_seen_seq` is
  part of `state`, persisted to the ledger, and re-loaded
  on Resume. The provider-cache prefix is **stable across
  pause / resume cycles**.

#### Harness de-dup (same-feature supersedes)

Some `harness.*` kinds are **append-only by nature**: each
emission is a fresh snapshot of the same logical thing.

- `harness.feature{feature:"skills_catalog"}` — the runtime
  re-emits the current skills catalog whenever it changes.
  Over a long round, the ledger has N rows for the same
  feature; the model only needs the **latest one**.
- `harness.feature{feature:"env_snapshot"}` — environment
  state (PATH, working dir, tool availability) is re-emitted
  at meaningful changes. Same supersedes-older pattern.
- `harness.feature{feature:"agents_md"}` — the project's
  `AGENTS.md` content is re-emitted when it changes. Same
  pattern.

The default masking-instructions walk in `assemble` step 1
does **not** cover this — masking operates on entry `seq`
ranges, not on entry equivalence. So `assemble` adds a
**same-feature dedup** pass after the kinds filter:

```python
def dedup_harness(body):
    """Keep the latest entry per (kind, feature) pair.
    Older entries still exist in the ledger (audit) but
    are hidden from the surface."""
    latest = {}
    for e in body:
        if e.kind.startswith('harness.'):
            key = (e.kind, e.body.feature)  # feature is a top-level field
            latest[key] = e                   # later seq wins
    # split: harness rows go through 'latest' only; non-harness pass through
    out = [latest[(e.kind, e.body.feature)]
           if e.kind.startswith('harness.') else e
           for e in body]
    return out
```

Three properties:

- **Old entries are not deleted.** The ledger preserves the
  full history of `harness.feature` emissions. Audit sees
  every snapshot. The next `memory_watermark` walk may
  pick up the most recent of them as a long-term fact.
- **Old entries are masked from the surface.** The
  projection's view of the model is exactly one row per
  `(kind, feature)` — the latest. The model's context is
  not bloated by N copies of the same skills catalog.
- **Deduplication is per `(kind, feature)`, not per
  `body_hash`.** Two consecutive emissions of the
  `skills_catalog` *with the same content* still result in
  one row on the surface (one of them is shadowed). Two
  emissions *with different content* result in the **newer**
  one on the surface (the old is shadowed but kept).

The masking-timeline walk in step 1 stays as-is — it operates
on the masking-family entries (`compaction.rewrite`,
`privacy.redact`, `session.undo`, `session.redo`,
`branch.*`). The harness de-dup is a **separate pass**
because it operates on equivalence (same `feature`), not on
range (same `seq` span).

#### Layout-aware assembly (U-shaped attention)

The model has a known attention shape: **the head and tail
of the prompt are strong; the middle is weak**. The classic
"lost in the middle" finding. `assemble` exploits this by
positioning content by its attention value, not just by
its `seq` order.

The default layout has three zones, in order:

| Zone | Content | Why |
|------|---------|-----|
| **Head** (strong) | `pinned` block: `goal.*`, `system`, the latest `harness.feature` (skills catalog, env_snapshot, agents_md). The model's "frame" for the round. | Head of context is the most-attended; the frame belongs there. |
| **Middle** (weak) | `seen` part (history of prior iterations), `compaction.summary` rows. The "boring middle" — the model knows about it but doesn't anchor on it. | The model's middle attention is weakest; this is where the volume goes. |
| **Tail** (strong) | `new` part (delta since `last_seen_seq`), the most recent `tool.result`, the most recent `assistant.text`, anything the loop wants the model to *act on next*. | Tail of context is the second-most-attended; the actionable stuff belongs there. |

The **default** layout is `[head | middle | tail]`. Other
modes rearrange:

| Mode | Layout | Use case |
|------|--------|----------|
| `default` | `[head | middle | tail]` | Normal rounds. |
| `compact` | `[head | tail]` (no middle) | Long rounds where the middle is mostly summaries the model has already digested. Skips the attention valley. |
| `audit` | `[head | middle | tail]` + every row's metadata (`provenance`, `wall_ts`, `pair_ref`) appended | `AUDIT_REPLAY` projection; not for model consumption. |
| `governance` | `[head | relevant_intent]`, no middle, no tail | `GOVERNANCE_INPUT` projection; the judge sees only what it needs. |
| `speculative` | `[head | tail_only]` | Round is about to call the model with **only the deltas**; the seen part is implicit (provider cache). For rounds where the middle is guaranteed to be a cache hit. |

Layout mode is a parameter of `assemble`:

```python
def layout(body, mode='default'):
    head, middle, tail = split_zones(body)
    if mode == 'default':  return head + middle + tail
    if mode == 'compact':  return head + tail
    if mode == 'audit':    return head + middle + tail + audit_meta(body)
    if mode == 'governance': return head + governance_intent_only(body)
    if mode == 'speculative': return head + tail
```

The **head is always the same**: the pinned block + the
latest `harness.feature`. Head content does not change
between iterations (it's pinned / de-duped), so the
**head position is provider-cache stable**. The **tail is
appended to**, so the cache breaks at the tail (as the
delta fragment contract expects). The **middle is the
disposable zone** — if budget gets tight, that's where
`assemble` cuts first.

**Why this is constitutional:** the same
append-only-and-stability rules that govern `derive` govern
`assemble`'s layout. The head does not change bytes. The
middle appends at its own boundary. The tail is the only
truly-new bytes. The provider cache sees `[head]`
unchanged across iterations (full hit), `[middle]`
appended to (cache break at the new tail), `[tail]` freshly
emitted (always new). With `compact` mode, the middle is
omitted entirely — when the cache breaks at the new middle
boundary, the new boundary is *the same* as a
default-mode boundary (because the middle was already
digestible). Prompt cache locality is preserved across mode
switches within a round.

#### What assemble does **not** do

- It does **not** re-walk the ledger. `derive` does the
  ledger walk (incrementally); `assemble` works off the
  cache.
- It does **not** re-decide tier boundaries. Tier
  decisions are sticky (see `Derive Stability` Rule 2) and
  are decided once, when the entry is first projected.
- It does **not** call `governance.judge`. Governance
  happens **between** `assemble` and `model.invoke` —
  the model sees the assembled prompt, emits an intent,
  and the loop asks `governance.judge(intent)` before
  calling `executor.run(intent)`.
- It does **not** call `summarize`. `summarize` is a
  *write* op (it appends a `compaction.summary` row to the
  ledger), not a read op. `assemble` only reads.

`assemble` is **read-only over the cache**; all the writes
happen elsewhere (`summarize` writes summaries;
`governance.judge` writes verdicts; `executor.run` writes
`tool_result` and effects; the loop writes the
`model.usage` rows; **the loop also writes the per-round
receipt to `ops_log` with `op = 'loop.receipt'`**).

#### Where assemble fits in the loop

```
# in agent_turn, between derive and model.invoke:

surface = derive(kinds=ALL, budget=BUDGET)
assembled, receipt = assemble(
    surface,
    projection=PROMPT_FOR_MODEL,
    last_seen_seq=state.last_seen_seq,
    round_id=state.round_id,
)
state.last_seen_seq = max(state.last_seen_seq or 0, surface.latest_seq)

reply = model.invoke(
    pinned = assembled.pinned,
    seen   = assembled.seen,
    new    = assembled.new,
)
# ... judge, run, etc.

# Receipt flushed to ops_log at flush points (see below)
```

#### Derive / Assemble Receipt (log-only view description)

`derive` and `assemble` are not just **read** operations —
they are **read + describe**. Each call returns the data
the consumer asked for, **plus a receipt** that records
exactly what that call did. The receipt is **log-only**:
it does **not** enter the source-of-truth ledger as a
new kind; instead it is a **derivable view** over the
ledger. The runtime keeps the receipt in a small **in-memory
buffer attached to `state`**; the buffer is flushed to
`ops_log` at flush points (turn boundary / Suspend / close)
and queryable from there. (See "Receipt storage" below.)

**Why:** the ledger captures **what happened** — every
event, every intent, every result. The receipt captures
**what the model saw** — which is a function of the ledger
*and* the masking / projection / layout / dedup state at
the moment of the call. Without the receipt, an analyst
sees the ledger and asks "why did the model not see X?"; with
the receipt, they answer that question by reading one row.

> **Before the receipt, an analyst could only see *what the
> ledger has*; with the receipt, they can see *what the model
> actually saw*, and replay each round with the exact prompt
> the model got.**

**Receipt storage — `ops_log` with `op = 'loop.receipt'`.**
The receipt is **not** a new table. The ledger is
**append-only**; the receipt is **per-round metadata** that
the runtime needs to query for analysis. `ops_log` already
exists for exactly this kind of per-op audit trail (GC
runs, vacuum, sweep). The receipt is one more `op` value
in that table:

```sql
-- ops_log already has:
--   id, op, started_at, finished_at, items_removed, notes, wall_ts
--
-- For receipts:
INSERT INTO ops_log (op, started_at, finished_at, items_removed, notes)
VALUES ('loop.receipt', ?, ?, 0, ?);
-- notes = JSON-serialised receipt body
```

`ops_log`'s existing indexes (`started_at`, `(op,
started_at)`) are exactly what receipt queries need
("latest receipt", "all receipts for round X"). Adding a
new table for receipts would duplicate the indexing
infrastructure and break the "ledger + ops_log + audit"
three-table pattern; the right move is to **promote the
receipt to a first-class `op` value** in the existing
audit log.

**Why not a `loop.receipt` entry kind?** Because the
receipt is a **derivation**, not an event. The ledger is
"things that happened"; the receipt is "the runtime's
*description* of what the model saw in light of those
things." Mixing the two in one table conflates "what
happened" with "what was visible" — two different
concerns. Keeping the receipt in `ops_log` (the audit
trail) makes the **direction of dependence** explicit:
`ledger = truth`, `ops_log = derived view of how the runtime
used the truth`.

```python
class DeriveReceipt:
    round_id:        uuid                  # which round this is
    seq_from, seq_to: int                 # entries fed to derive
    seq_count:        int                  # entries considered
    projection:      str                  # PROMPT_FOR_MODEL | SUMMARIZE_INPUT | ...
    layout_mode:     str                  # default | compact | audit | governance | speculative
    token_pinned:    int                  # tokens in pinned block
    token_seen:      int                  # tokens in seen part
    token_new:       int                  # tokens in new part
    token_total:     int                  # pinned + seen + new
    last_seen_seq:   int | None          # the new boundary after this round
    notes:          dict                  # structured JSON

    @dataclass
    class MaskApplication:
        kind:        str                  # compaction.rewrite | privacy.redact | session.undo | session.redo | branch.verdict
        covers:     [int, int]            # [from, to] or [uid]
        reason:     str

    @dataclass
    class DedupAction:
        kind:        str                  # harness.feature | ...
        feature:    str                  # which feature was dedup'd
        shadowed_count: int              # how many older rows shadowed

    @dataclass
    class Skipped:
        seq:        int
        kind:       str
        reason:     str                  # branch.discard | privacy.redact | unknown_kind | ...

    @dataclass
    class Warning:
        kind:       str                  # unknown_kind_skip | partial_coverage | ...
        detail:     str
```

**`record_receipt(receipt)` writes to a separate
`receipt` table** in the same db — distinct from the
ledger's `entry` table because the receipt is a **view
description**, not an event:

```sql
-- ops_log already has:
--   id, op, started_at, finished_at, items_removed, notes, wall_ts
--
-- For receipts:
INSERT INTO ops_log (op, started_at, finished_at, items_removed, notes)
VALUES ('loop.receipt', ?, ?, 0, ?);
-- notes = JSON-serialised receipt body
```

The receipt's `op = 'loop.receipt'` reuses the existing
`ops_log` schema — its `(op, started_at)` index and
`started_at` index are exactly what receipt queries need
("latest receipt", "all receipts for round X"). No new
table, no new indexes, no schema drift.

**What the receipt enables:**

- **Per-round analysis** — which rounds used how much
  budget, which projections were applied, which rounds hit
  a compaction event. SQL aggregate over `receipt` answers
  "what was the model's *typical* surface look like over
  this round?" — useful for tuning tier policies, optimising
  `compaction.rewrite` triggers, etc.
- **Debug** — when the model makes a surprising call, the
  receipt shows *exactly what it saw* in that round. No more
  "but the tool_result was in the ledger, why didn't the
  model use it?" — the receipt says whether the row was
  masked (branch discard / privacy redact / compaction),
  dedup'd (harness de-dup), or simply not in `kinds`.
- **Replay** — given a receipt + the ledger at the moment
  of the call, the model can be re-invoked with the **exact
  same prompt bytes**. Useful for testing tier-policy
  changes against historical rounds ("what would the model
  have done on this round if the policy were X?").
- **Audit / privacy** — a user-facing tool can say "on
  round N, the model saw A, B, C, and D, but not E (which
  was redacted on day Y)". The receipt carries the
  redacted-by reference; the user sees what was hidden and
  why.

**The receipt does not see the body.** It records
*metadata* — what was masked, what was dedup'd, what was
skipped, how many tokens. It does **not** record the body
text itself (that's the ledger's job). The receipt is the
*index into the round*, not the *content of the round*.

#### Where assemble fits in the loop

The **prompt structure** the model sees is **three
segments** (`pinned`, `seen`, `new`), separated by
explicit tokens the runtime can recognise. The provider
sees a stable prefix (`pinned + seen`) and a small
appended delta (`new`); the cache key matches the prefix
across iterations.

`assemble` is the last transformation the loop applies
before the model call. Everything after `assemble` is
**interaction with the world** (`model.invoke`,
`governance.judge`, `executor.run`). Everything before
`assemble` is **interaction with the ledger** (`append`,
`derive`). `assemble` is the bridge.

### Derive in Detail (continued)
cache resets to the directive's start-point and is rebuilt
from there. A round that runs for a month reads **only the
delta since the last derive**, not the month's accumulated
ledger.

> **Compression is the steady state, not an edge case.**
> Any round that runs long enough crosses a token budget —
> summarisation lands, a `rewrite_directive` lands, the surface
> cache resets. The reset / replay path is the **hot path**
> of `derive`, not the cold one. Design and test accordingly.

```python
class Surface:
    """Cached view of the ledger that the loop maintains
    across calls to derive. Lives in `state` (alongside
    `phase`, `iteration_count`, etc.)."""
    start_seq: int                       # earliest seq in the cache
                                          #   (= 0 if no compression,
                                          #    or latest directive's
                                          #    covers.seq_range.end + 1)
    last_seq: int                       # latest seq in the cache
    entries: list[Entry]                # the cached entries
    always_loaded: dict[Kind, Entry]    # special kinds (e.g. goal)
                                          #   maintained independently
                                          #   of seq range

def derive(kinds, budget):
    # 1. 拉所有新条目
    #    rewrite_directive 也是 ledger 的一种 kind — 自然落在
    #    entries_since(last_seq) 里，无需单独的 last_directive_seq。
    new_entries = ledger.entries_since(surface.last_seq)

    # 2. 检查新条目里是否有 rewrite_directive
    #    如果有，最新的那条 directive 决定新的 start_seq
    directives = [e for e in new_entries
                 if e.kind == "rewrite_directive"]
    if directives:
        rw = max(directives, key=lambda e: e.seq)
        new_start = rw.covers.seq_range.end + 1
        # 重置到新起点；保留所有 >= new_start 的条目
        surface.entries = [
            e for e in (surface.entries + new_entries)
            if e.seq >= new_start
        ]
        surface.start_seq = new_start
    elif new_entries:
        # 3. 没有压缩 → 增量 append
        surface.entries.extend(new_entries)

    surface.last_seq = ledger.latest_seq()

    # 4. always-loaded kinds 独立维护（不受压缩影响，始终最新）
    for k in ALWAYS_LOAD_KINDS:        # {goal, system, ...}
        active = ledger.latest_active(kind=k)
        surface.always_loaded[k] = active   # 全量替换
                                            #   永远取最新版本

    # 5. 按 kinds + budget 装配 → surface
    return assemble(surface, kinds, budget)
```

#### Cost model — what actually runs per iteration

| Scenario | What `derive` does | Cost | Frequency |
|----------|-------------------|------|-----------|
| No compression, ledger grew by Δ entries since last derive | append Δ to cache | **O(Δ)** — typically a handful of entries | common (early round, short rounds) |
| **Compression event** (`rewrite_directive` lands) | clear cache, replay from `covers.seq_range.end + 1` | O(prefix-size after the cut) — bounded by what the summary captures | **steady state** — any long-running round hits this repeatedly |
| `always_loaded` kind updated | swap one entry in `surface.always_loaded` | O(1) | whenever `goal` etc. is rewritten |
| `assemble` for the budget shape | walk the cache, drop tail to fit budget | O(cache-size) — small per iteration | every iteration |

A round that has run for a month **does not re-scan** the
month's accumulated ledger on every iteration. The cache
holds the *current* view; derive only processes **the delta
since the previous call**. The compression path is exercised
frequently, not rarely — design and benchmark for it.

#### Why this is safe

- **Ledger is append-only** (`.agents/ddd.md` + `engine/AGENTS.md`).
  Once an entry is written, it's there forever; derive
  doesn't have to handle mid-stream edits.
- **`rewrite_directive` resets, not rewrites.** The cache
  clears and replays from a fresh start-point. Nothing is
  "merged" — the previous view is discarded.
- **`always_loaded` is replace, not merge.** Goal / system /
  frame entries are *singletons*; the model always sees the
  latest version, full stop. They're outside the seq-range
  compression scheme on purpose — `goal` is the *frame* and
  must survive any summarisation.
- **No "scan all" fallback.** The implementation never walks
  the whole ledger; if a corner case requires a fresh build
  (process restart, ledger corruption recovery), that is an
  explicit recovery path, not a per-iteration cost.
- **No redundant state.** `Surface` carries only the fields
  it actually reads back. `last_directive_seq` would be
  derivable from `entries_since(last_seq)` filtered by
  `kind == rewrite_directive` — so the algorithm inlines it
  rather than tracking a second cursor.

#### Summary prefix convention

Summarised entries are recognised by a **kind prefix** on
the ledger entry's content:

```
# 完整条目
reply.items[0]      →  kind = "assistant.message", seq = 42

# 摘要条目（替代 seq 30..41）
reply.items[1]      →  kind = "summary.v1",  seq = 43,
   content = "<compressed summary covering seq 30..41>"
```

Rules:

- A summary's `kind` **always** starts with `summary.`
  (e.g. `summary.v1`, `summary.short`, `summary.tool-only`).
- A summary's `covers.seq_range` names the seqs it replaces.
  Future derives skip those seqs.
- A summary's `seq` is **after** its covers (so a normal
  range walk reads it).
- A `rewrite_directive` references the summary's `seq` and
  the `covers.seq_range`; future derives find the latest
  directive and resume from `covers.seq_range.end + 1`.

#### Summary Entry Schema

A summary is **structured**, not a free-form blob. The model
that produces the summary is asked to fill in named fields —
not "summarise this prefix". Structure buys two things:
(1) the next surface is deterministic regardless of who
summarised; (2) the summary is queryable by field rather
than requiring the next LLM to re-parse prose.

| Field | What it captures | Source |
|-------|------------------|--------|
| `goal_progress` | Where are we in the user's task? What's done, what's left? | tracked across iterations |
| `confirmed_facts` | Tool calls that succeeded; their key results (e.g. "file X has 432 lines", "test fails on Y"). | every `tool_result` with `verdict = Approve` |
| `actions_taken` | The sequence of tools the round called, in order, with their key parameters. | the ledger's `intent` stream |
| `state_progress` | Where the iteration's `phase` machine is (Running / AwaitingApproval / AwaitingConfirm / Done / Suspended / Failed). | `state.phase` |
| `open_questions` | Things the model is still trying to figure out. | the model's own beliefs |

These fields are **always shown** in the summary entry, even
when the underlying detail has aged out of the surface. They
are dense, low-token, high-signal — exactly the "conclusion
of tool calls" the model needs to avoid re-doing work.

#### Boundary Invariants (Compression Boundaries)

The cover boundary of a summary must be **logically clean**:
it must not split a logical unit across the boundary.
Specifically:

- **Tool calls must appear as a pair** (`intent` + its
  `tool_result`). A summary must not cover an `intent`
  without its `tool_result`; the model needs the result to
  reason about whether the call succeeded.
- **A pending `approval_request`** (AskUser verdict) must be
  either fully inside the covered range — with its eventual
  verdict + effects back-filled — or fully outside it. A
  boundary that chops a pending approval in half leaves
  `Suspended` and `Resume` unable to reason about the round's
  pause state.
- **Respect iteration boundaries**: a summary should end at
  an iteration boundary, not in the middle of one. Mid-iteration
  summaries are recoverable only by replay (slow, defeats
  the point of compression).

The `summarize(prefix)` implementation is responsible for
**extending the prefix until the boundary is clean** — e.g. if
a tool call's `tool_result` lands on the edge, pull that
result in too. The cost is a slightly larger prefix; the
benefit is a derivable, recoverable surface.

### Derive Projections (purpose-specific views)

**`derive` is not a single function — it is a family of
projections**, each tuned to a different consumer. The
cached `Surface` is the *common path* every projection
starts from; the projection then **filters and reshapes**
according to its purpose. The same ledger, the same cache,
five different views.

```python
class Projection(Enum):
    PROMPT_FOR_MODEL   # the main loop's full surface
    SUMMARIZE_INPUT    # the prefix that summarise() will compress
    GOVERNANCE_INPUT   # intent + minimum context for governance.judge
    FORK_BRANCH         # per-branch result, side-by-side
    AUDIT_REPLAY        # append-only chronological, every entry visible
```

Each projection is a **filter + reshape** over the cached
surface — it does not re-walk the ledger, and it does not
build a separate cache.

#### `PROMPT_FOR_MODEL` — the main loop's surface

The full surface the model sees: full content for tier-0,
preview + seq pointer for tier-1, one-liner + seq pointer
for tier-2; always-loaded kinds first; `pinned` first;
budget-cut tail. The model is the primary consumer; this is
what the loop's `invoke` argument is.

#### `SUMMARIZE_INPUT` — the prefix for compaction

The `summarize(prefix)` op needs **only the entries inside
the prefix it is summarising** — not the whole surface, not
the always-loaded frames. The projection extracts the prefix
range from the cache:

```
def summarize_input(surface, prefix):
    return [e for e in surface.entries
            if prefix.from_seq <= e.seq <= prefix.to_seq]
```

No tier shrinking, no budget cut. `summarize` reads the
**un-shrunk** content of the prefix; if it can't see the
full text, it can't summarise faithfully. Tiering happens
**after** summarise produces the new `compaction.summary`
entry; until then, the prefix contents are full-fat.

#### `GOVERNANCE_INPUT` — intent + minimum context

`governance.judge(intent, state)` needs to evaluate **one
intent** at a time. The projection gives it:

- The **current intent** (the model emitted).
- The **minimum context** to evaluate it: the recent
  `assistant.message` (the model's stated intent + the tool
  it's about to call), the prior `tool_result` chain (what
  led up to this call), the current `goal.*`, and the latest
  one or two related entries.
- **No full surface.** The model can do the talking; the
  judge just decides. Excess context here is a **confound**:
  it lets the judge's verdict drift based on what happens to
  be in the cache.

```
def governance_input(surface, intent, state):
    return {
        'goal':     current_goal(surface),
        'system':   always_loaded(surface, kind='system'),
        'recent':   tail(surface, n=2),     # last 2 iterations' context
        'intent':   intent,
    }
```

#### `FORK_BRANCH` — per-branch result for comparison

When the user forks a session at `boundary_seq`, each branch
runs independently. Comparing branches needs:

- The branch's own append-only stream (no other branch's data).
- The **shared prefix** up to `boundary_seq` (so each branch
  can see what came before the fork).
- A **diff-friendly shape**: wall_ts, kind, summary fields,
  but **no body content** for entries that exist on both
  sides of the fork. Diff happens on the **what**, not the
  **how much detail**.

The projection returns a per-branch view; the comparison
tool is a separate op (`fork_diff`).

#### `AUDIT_REPLAY` — append-only chronological

The user or an auditor wants the **whole story**, in order,
with metadata:

- Every entry, in `seq`-ascending order.
- Each entry's `kind`, `wall_ts`, `provenance`, `seq`, `uid`.
- Bodies are shown **as they were written** — no tiering, no
  redaction (unless the row is `privacy.redact`-ed; the
  redaction placeholder shows instead).
- No budget cut. The auditor sees everything; the UI
  virtualises display.

This is the **only** projection that ignores tiering, surface
shrinking, and budget cuts. Audit is for *truth*, not for
*fitting a context window*.

#### Why projections, not a single `Surface` type

A single `Surface` type with optional fields is tempting
but wrong:
- It conflates the cache update path (common) with the
  consumer-specific reshape (varies).
- It makes audit a "really big surface", which collides with
  the loop's "small surface, please" pressure.
- It hides the per-consumer trade-off (e.g. governance
  doesn't need tiered previews; audit doesn't need pinned
  reordering).

Projections are a **family of view functions** over one
cache. They share the cache update cost; they differ in the
filter+reshape. The cache stays simple; the views stay
honest.

### Derive Stability (prompt-cache compatibility — constitutional)

LLM providers (Anthropic, OpenAI, others) ship **prompt
caches** keyed on a stable token prefix. The cache hits when
the next request's prefix **byte-for-byte matches** an earlier
one. If `derive` ever returns a surface whose *prefix* changes
between iterations — even semantically equivalent content
re-ordered or re-shrunk — the provider cache misses. A
sustained cache miss rate means paying full input cost every
iteration, which on long rounds is ruinous.

`derive` is therefore bound by **three stability rules**,
not by the optimisation pass's output shape alone. These are
**constitutional** — they govern the algorithm, not a
particular policy. Empirical validation against each
provider's cache behaviour is the final arbiter; the rules
below are the design's commitment to that empirical bar.

#### Rule 1 — Append-only projection

Across iterations within a session, `derive` returns a
surface whose **historical prefix is monotonic**. The
projection may *append* new entries (at the tail) and may
*evict* entries from the surface (replacing them with seq
pointers), but it must **never re-order** the surviving
historical entries, and must **never insert** new content
into a position earlier than the previous iteration's
tail.

```
# OK:     [tier-0]  [tier-0]  [tier-1]  [tier-1]  <-- new tail
# Bad:    [tier-0]  [tier-1]  [tier-0]  [tier-1]  <-- re-order
# OK:     [tier-0]  [redacted]  [tier-0]  <-- eviction collapses
# Bad:    [redacted]  [tier-0]  [tier-0]  <-- new entry in old slot
```

Append-only is the single most important property. It holds
even when the projection is `AUDIT_REPLAY` (where the
"prefix" is the whole session — re-ordering would also
defeat cache).

#### Rule 2 — Sticky tier decisions

Tier transitions are **one-way** within a session. A
`tool_result` that has been demoted from `tier-0` (full)
to `tier-1` (preview) **stays in `tier-1`** for the rest of
the session, even if a later iteration has budget to spare.
Re-promotion is **forbidden** in the same session.

```
# Allowed:
# tier-0 (full)  -> tier-1 (preview)  -> tier-2 (one-liner) -> ...
# Forbidden:
# tier-2 (one-liner) -> tier-1 (preview)  <-- re-promote
# tier-1 (preview)   -> tier-0 (full)     <-- re-promote
```

Why: oscillating between `tier-0` and `tier-1` would mean
the *same* surface bytes appearing in different forms on
adjacent iterations, which defeats the provider cache.
A demoted entry stays demoted; the model re-reads the
`seq_pointer` and re-attaches the full body **only if it
needs to** (via an explicit re-attach call, which the cache
key changes for, by design).

**Sticky applies within a session.** Across sessions (fork,
restart from backup), the surface is rebuilt from scratch —
no cache to break. The "sticky" rule is *intra-session*.

#### Rule 3 — Boundary-anchored reordering

Re-ordering of the surface is **only allowed at a
boundary**:

- `session.turn_start` — re-ordering the always-loaded
  block (e.g. `goal.*` rewriting its visible form) is OK
  because the cache is invalidated by the **turn boundary**
  anyway.
- `compaction.rewrite` — the cache breaks by definition; the
  new prefix starts at the rewrite point.
- A re-attach of a `seq_pointer` (the model asks for the
  full content of a previously summarised entry) is a
  *boundary* — the surface bytes change, and that's
  expected.

**Within a turn** (between two `compaction.rewrite`s), no
re-ordering of historical entries. The always-loaded block
may **append** new pinned entries (`goal.update` lands), but
the *order* of the already-loaded block does not change.

This is why `goal.*` is **append-only** (see `ledger.md`):
`goal.declare` appends, `goal.update` appends (with a `ref`
to the prior), `goal.close` appends. The current-goal
projection recomputes which is "current", but the visible
block's *byte order* is stable.

#### What this means for the algorithm

- **Tier decisions are sticky and append-only.** A `tier`
  column lives on each `tool_result` row in the cache (or
  on the row in the ledger); demotions are recorded, never
  reversed.
- **Order is by `seq` ascending.** The `derive` returns
  entries in `seq` order; no exceptions, not even for
  `pinned` rows. Pinned rows are prepended (lower `seq`),
  but among pinned rows, the original `seq` order is
  preserved.
- **Re-attach via `seq_pointer` is a write**, not a
  transformation. The model asks for a previously summarised
  entry's full body; the runtime appends a re-attach entry
  (or inlines it into the next surface) — the prior
  projection's prefix is not retroactively modified.
- **Boundary-anchored reordering** is the only escape
  hatch: at `compaction.rewrite` or `session.turn_start`,
  the cache resets and any reordering is fine.

#### Empirical validation (when this lands)

The three rules above are the design's **commitment**. The
actual provider behaviour (Anthropic cache TTL, OpenAI
auto-cache, Gemini implicit caching) determines how *strict*
the rules need to be in practice:

- If a provider's cache survives minor re-ordering, Rule 1
  can be relaxed for adjacent-tiers of the same tool
  (preserving prefix for the same `tool_result`).
- If a provider offers a "stability hint" (a hash, a
  fingerprint), `derive` may emit it for the model's debug.

`docs/bench/perf/cache-stability.md` (forthcoming) holds
the empirical measurements and the concrete policy
parameters (e.g. "Anthropic: 5-minute cache TTL; we treat the
surface as a 4-minute rolling window with re-anchored prefix
on every `compaction.rewrite`").

Until that file lands, the rules above are **the
constitution** — implementation is expected to honour them
even before benchmarks exist. The first regression test for
this feature should be a unit test that asserts
"`derive(N+1).prefix_bytes == derive(N).prefix_bytes` for
the no-reorder, no-promote case."

#### Why this shape

- **Start-point is single-valued.** Only the *latest*
  `rewrite_directive` matters. Older ones are reached
  transitively through the chain of summaries they describe.
- **Always-load kinds are exempt.** `goal`, `system`, and any
  future "frame" kinds are appended to every surface,
  regardless of what was summarised. The model always
  remembers the goal.
- **Budget cuts from the tail, not the middle.** If the
  range overflows budget, drop the oldest entries first
  (after the start-point has been re-established). Never
  excise a summary — a summary is a load-bearing record.
- **Reads are bounded by deadline.** A `wall_clock_ms`
  ceiling stops a runaway derive from blocking the loop.
  Partial surfaces are acceptable; the next iteration
  extends them.

#### Three-mechanism Recap (Tool-result Trajectory)

The three mechanisms — **structured summary**,
**clean cover boundary**, and **seq-pointer back-reference**
— together form a single trajectory for every tool result.
The surface is **always** a lossy projection; the ledger
never forgets.

| Phase | Surface form | Ledger form | Recovery |
|-------|--------------|-------------|----------|
| **Just called** | full content | full content | (n/a) |
| **Aged out** | preview + seq pointer | full content | re-resolve via seq |
| **Compressed away** | structured-summary fields (`goal_progress`, `confirmed_facts`, `actions_taken`, `state_progress`, `open_questions`) | full content + summary entry | re-resolve via seq, or read summary fields |

Three guarantees stack:

1. **No information lost that matters.** Summary fields
   capture the *conclusion* of tool calls — enough for the
   model to avoid re-doing work, but not enough for it to
   re-derive everything.
2. **No summary chops a split a logical unit.** Cover
   boundaries respect call/result pairs, pending approvals,
   and iteration boundaries.
3. **Detail is always recoverable.** A seq pointer in the
   surface lets the model re-attach the full content
   without the runtime round-tripping through the ledger
   on every reference.

The point: **the model never loses information that
matters, and never re-pays for information it once saw.**
Information loss is always **opt-in and recoverable**.

#### Why this shape

- **Start-point is single-valued.** Only the *latest*
  `rewrite_directive` matters. Older ones are reached
  transitively through the chain of summaries they describe.
- **Always-load kinds are exempt.** `goal`, `system`, and any
  future "frame" kinds are appended to every surface,
  regardless of what was summarised. The model always
  remembers the goal.
- **Budget cuts from the tail, not the middle.** If the
  range overflows budget, drop the oldest entries first
  (after the start-point has been re-established). Never
  excise a summary — a summary is a load-bearing record.
- **Reads are bounded by deadline.** A `wall_clock_ms`
  ceiling stops a runaway derive from blocking the loop.
  Partial surfaces are acceptable; the next iteration
  extends them.

### Surface Optimisation Passes (inside `assemble`)

The `assemble` step in `derive` does more than budget-fit.
It runs three optimisation passes before returning. **The
ledger is the source of truth; the surface is a tuned
projection.** Anything dropped from the surface is **still in
the ledger** and recoverable via a seq pointer — the surface
never *deletes* content, it only *compresses visibility*.

#### 1. Tier-based `tool_result` Visibility

`tool_result` entries shrink with age. The model's working
memory doesn't need the full text of a result from 50
iterations ago — it needs to know it exists, where to find
it, and a one-line hint.

| Iterations ago | What the surface shows |
|----------------|------------------------|
| **0–3** | full content |
| **4–10** | preview (head + tail, tool-specific) + seq pointer to ledger |
| **> 10** | one-liner (tool name + key result) + seq pointer to ledger |

The model is told `full content at <ledger-seq>` if it needs
to drill back. The original is **never deleted from the
ledger**; only the surface shrinks.

#### 2. Big-result Eviction

Some tool results are huge (`read_file` on a 50 MB log,
`bash` with megabytes of stderr). Even when fresh, they may
push the surface past `budget.tokens`. The eviction rule:

1. Identify the largest `tool_result` in tier 0 (full
   content) that's still in the surface.
2. **Evict** it — replace with a `seq_pointer` entry pointing
   back to the ledger. The model sees
   `tool_result evicted; full at <ledger-seq>`.
3. Repeat until the surface fits `budget.tokens`.

Eviction is **lossless** for the model (the ledger retains
the full content) and **transparent** (the model sees a
pointer, not a missing entry). When the model asks for the
full text, the runtime resolves the pointer and re-attaches
the content into the next iteration's surface.

Eviction never touches tier 1 (preview) or tier 2 (one-liner)
— those are already small enough.

#### 3. Per-tool Formatting

Different tools have different useful previews. A
`read_file` benefits from head + tail + size; a `bash`
benefits from exit code + last lines + line count. The
`preview_of` / `one_liner_of` functions are per-tool.

**Tier 1 preview shape (per tool):**

| Tool | Preview |
|------|---------|
| `read_file` | first 20 lines + last 5 lines + line count + size |
| `bash` | exit code + last 20 lines + total line count |
| `grep` | match count + first 5 matches |
| `web_fetch` | title + first paragraph + content-type |
| (other) | first 20 lines + last 5 lines + size |

**Tier 2 one-liner shape (per tool):**

| Tool | One-liner |
|------|-----------|
| `read_file` | `read_file(<path>): <N> lines, <size> bytes` |
| `bash` | `bash(<cmd>): exit <code>, <N> lines` |
| `grep` | `grep(<pattern>): <N> matches` |
| `web_fetch` | `web_fetch(<url>): <N> bytes, <status>` |
| (other) | `<tool_kind>(<key_arg>): <key_result>` |

Per-tool formatters are pluggable: adding a new tool = adding
a new formatter entry. The formatters live with the tool
definitions, not in the loop.

#### Per-tool Policy Profiles

Different tool *kinds* want different visibility rules.
Search results are consumable in compressed form (the model
just needs "this exists, here are top N matches"). Edit /
confirmation results must stay full — the model needs exact
content to confirm a re-edit without re-reading. Command
output sits in the middle. We give each tool class a
**policy profile** that ties together its tier boundaries,
eviction behaviour, and preview formatter.

| Class | Examples | Tier boundaries (full / preview / one-liner) | Eviction | Why |
|-------|----------|-----------------------------------------------|----------|-----|
| **Search** | `grep`, `glob`, `web_fetch`, `web_search`, `find` | 0–3 / 4–10 / >10 | **eager** | Result is a list; model needs match count + a few samples, not the whole list. Re-runnable. |
| **Edit** | `read_file`, `edit`, `write`, `apply_patch` | 0–10 / 11–30 / >30 | **never** | The model confirms edits against the exact file content it read. Compressing forces re-reads. |
| **Command** | `bash`, `python`, `node`, `cargo`, `go` | 0–5 / 6–15 / >15 | **late** | Exit code + tail diagnostics carry most of the value; full output is rarely re-needed. |
| **Generic** | (other tools) | 0–3 / 4–10 / >10 | **eager** | Unknown tools get the default cautious profile. |

```python
class ToolClass(Enum):
    SEARCH = "search"      # aggressive decay, eager eviction
    EDIT   = "edit"        # preserve, never evict
    COMMAND = "command"     # moderate decay, late eviction
    GENERIC = "generic"    # default cautious profile

def tier_for(tool_class: ToolClass, tiers_old: u32) -> Tier:
    return match tool_class:
        ToolClass.SEARCH   => match tiers_old: 0..3=>Full; 4..10=>Preview; _=>OneLiner
        ToolClass.EDIT     => match tiers_old: 0..10=>Full; 11..30=>Preview; _=>OneLiner
        ToolClass.COMMAND  => match tiers_old: 0..5=>Full; 6..15=>Preview; _=>OneLiner
        ToolClass.GENERIC  => match tiers_old: 0..3=>Full; 4..10=>Preview; _=>OneLiner

def eviction_for(tool_class: ToolClass) -> EvictionPolicy:
    return match tool_class:
        ToolClass.EDIT   => EvictionPolicy.NEVER
        ToolClass.SEARCH => EvictionPolicy.EAGER
        ToolClass.COMMAND => EvictionPolicy.LATE
        _                => EvictionPolicy.EAGER
```

The tool→class mapping lives with the **tool definition**,
not in the loop. Adding a new tool = declaring its class.
Default for an unknown tool is `GENERIC` (cautious profile).

#### Why `EDIT` is `never`-evict

The model will frequently re-reference a file it has read
once: to verify an edit, to re-edit a neighbour line, to
copy-paste a comment. Each reference requires the exact
content — a preview with a seq pointer would force the loop
to schedule a re-attach round-trip. The cost of *always
showing the full text* of an edit-class entry is cheaper
than the cost of *re-resolving it on every reference*.

`COMMAND` is `late`-evict because command output often
diagnoses a problem the model needs to circle back to (e.g.
"the test that failed at iteration 4 might be the same one
that fails again at iteration 7"). Eager eviction drops
that diagnostic context too early.

`SEARCH` is `eager`-evict because re-running a search is
cheap, and the result is structurally compressible (lists,
matches, top-N) — eviction saves more than it costs.

The per-class profiles are **policy**, not mechanism. They
fall out of the same three-pass `assemble`; only the
*thresholds* and *eviction triggers* differ. Swapping a
profile (eager ↔ late) does not require a mechanism change.

#### Combined Pseudocode

```python
def assemble(surface, kinds, budget):
    out = []

    for e in surface.entries:
        if e.kind in {"tool_result", "assistant.tool_call"}:
            tiers_old = state.iteration_count - e.approx_iteration
            if tiers_old <= 3:
                out.append(e)                       # tier 0: full
            elif tiers_old <= 10:
                out.append(preview_of(e))           # tier 1: head+tail
                out.append(seq_pointer(e.seq))      # + seq pointer
            else:
                out.append(one_liner_of(e))         # tier 2: one-liner
                out.append(seq_pointer(e.seq))
        else:
            out.append(e)                            # non-tool entries: full

    # Big-result eviction until surface fits budget
    while token_count(out) > budget.tokens and has_evictable(out):
        big = biggest_tool_result_in_tier0(out)
        out.remove(big)
        out.append(seq_pointer_evicted(big.seq))

    return out
```

The three passes interact in a fixed order: tiering first
(per-entry visibility), eviction second (budget pressure
releases big results). They never run in the opposite order —
eviction always sees the tier-shrunk surface.

### Mechanism vs Policy

The three passes above describe the **mechanism** — the
structural capability to:

- Compress a `tool_result`'s visibility (tiering).
- Evict a big result losslessly via seq pointer.
- Shape per-tool previews (pluggable formatters).

They are **not** a specific implementation. The mechanism
defines what the runtime can do; the **policy** chooses what
the runtime does. Two equally valid policies fit the same
mechanism:

| Policy | What it does | Trade-off |
|--------|--------------|-----------|
| **Eager (default candidate)** | Tier on every `derive`; evict big results as soon as the surface overflows budget. | Simple, predictable, slightly more compute per derive. |
| **Lazy / verify-first** | Keep `tool_result` at full content for a configurable grace window; only tier / evict when the model signals "I don't need this anymore" via an intent. | More compute-efficient; the model has to know it can hint eviction. |

A future variant — **adaptive** — switches between the two
based on observed budget pressure (token use × iteration
count). All three share the same **mechanism**: tiering,
eviction, per-tool preview, seq pointer to ledger. The
mechanism is what the codebase ships; the policy is what
runtime tuning chooses.

Future work: a `SurfacePolicy` config that picks one of the
above at startup, with **eager** as the sensible default
because it's the simplest to reason about and to test.

#### What is *not* up for debate

The **mechanism** is fixed:

- `Surface` is a cached projection of the ledger.
- `derive` is incremental + stateful.
- The ledger is the source of truth.
- `tool_result` can be shrunk / evicted losslessly via seq
  pointer.
- Per-tool preview is pluggable.

The **policy** is the choice:

- How many iterations count as "fresh" (tier 0 boundary).
- When to evict (eager / lazy / adaptive).
- What a specific tool's preview shape is.
- Whether the model can hint eviction.

Mixing the two has a cost: conflating them turns policy
choices into structural changes. Keep them separate — the
mechanism defines the surface area; the policy populates it.

### Governance Is a Port, Not an Actor

Every intent the model emits (tool call, file write, network
request) is **judged** by `governance.judge(intent)` before it
runs. The verdict is one of:

| Verdict | Action |
|---------|--------|
| `Approve` | executor runs the intent as-is |
| `Deny(reason)` | a `tool_result.rejected(reason)` is appended; the rejection is fed back to the model next iteration (no termination) |
| `ApproveWithRewrite(x)` | executor runs `intent.with(x)` — typically used to constrain a parameter |
| `AskUser(question)` | an `approval_request` is appended and the round **returns `Suspended`** — this is the **only** way the round pauses |

Governance is **queried** — the loop asks `governance.judge`,
not the other way around. This keeps the loop the single
sequencer and governance a side-effect-free function over
intent + state.

### Suspended / Resume

```
turn 2 (中途 Suspended)
  iteration 1..3  →  AskUser  →  return Suspended
  ... (3 天后用户应答) ...
  Resume  →  iteration 4..5  →  Done
```

The round that paused is **the same** `turn_id`. Resume is an
`entry` to `agent_turn`, not a new turn. On resume:

1. `derive_position(ledger)` finds the in-flight turn and
   locates where it paused (the pending `approval_request`).
2. The round re-enters the `while` loop with a synthetic
   `entry` carrying the human's answer.
3. Unpaired calls (from a previous iteration that didn't get
   to execute) get their results back-filled.

## Consequences

### Positive

- **Recoverable** by construction. The ledger is the
  snapshot; the driver just replays from there. A crash, a
  deploy, a network blip — none of them lose round state.
- **Bounded** by construction. `ledger.derive(kinds, budget)`
  enforces the budget *before* the call, not after. The model
  never has a chance to see a context window it can overflow.
- **Governed** by construction. Every effect flows through
  `governance.judge`. The model is a *proposer*; the runtime
  is the *actor*.
- **Audit-able.** Every decision is in the ledger. Every
  approval, every denial, every rewrite, every summary
  rewrite — all there with provenance.

### Costs

- **The ledger is a hot dependency.** The whole loop is
  bounded by `ledger.derive`'s latency. The derive path
  needs to be cheap; surface construction needs to be
  incremental; summarisation must be backgrounded when
  possible. (See Open Questions.)
- **Iteration is not free.** Each round trip is
  `derive → invoke → judge → run`. The driver loop is a
  critical-path latencher. Optimising derive and
  surface assembly is a high-leverage investment.
- **Governance policy lives outside the loop.** `governance`
  is a function over `(intent, state) → verdict`. Its policy
  is a separate design — `docs/architecture/governance.md`,
  TBD.

### Cross-language

This design applies uniformly to **Rust** (`engine/core/`) and
**Kotlin** (`engine-kt/core/`). Same loop, same ledger, same
governance port. The conformance suite
(`just conformance`) is the sync lock for the
ledger / governance / executor ports specifically.

## Open Questions

1. **Ledger shape**: append-only vs bitemporal; per-turn
   segments vs global log. (See forthcoming
   `docs/architecture/ledger.md`.)
2. **Summarisation cost**: the one impure step. Should
   summarisation run in a background task that pre-emptively
   summarises old prefixes, or strictly on-demand inside the
   loop?
3. **Approval-request lifetime**: do `approval_request`
   entries ever expire? If a user never replies, what does the
   round do — pause forever, timeout, escalate?
4. **Determinism**: does the loop need to be replay-deterministic
   for testing? If yes, the LLM summarise step is the only
   non-determinism and needs to be mocked.
5. **Multi-agent concurrency**: this design assumes one
   `agent_loop` per process. How does it interact with
   multi-agent runs (`engine/multiagent/`) that spawn multiple
   rounds in parallel? (Likely: each round still has its own
   ledger segment; the driver multiplexes.)
6. **Streaming vs request/response**: `model.invoke` is
   request/response here. If we adopt token streaming for
   low-latency UIs, the ledger-write granularity changes.

## Known Limitations

- **One LLM call per iteration.** Streaming tokens before the
  full response is final would let the UI render as the model
  thinks. This design assumes full-response-then-judge. If
  streaming is required, the loop splits into a streaming
  phase and a judging phase per intent.
- **`summarize` is synchronous**. The loop pauses on
  summarise. A 2-second model call to summarise an old
  prefix adds 2 seconds of round-trip latency. Background
  summarisation would help but adds the question "which
  summary is the model seeing right now?" — see Open
  Questions.
- **Governance is only as good as its policy.** A policy bug
  is a security bug. The policy itself is a separate design.
- **No optimistic concurrency.** Two `agent_loop`s writing the
  same ledger would race. Today this design assumes one
  process = one loop = one ledger writer. Multi-process
  would need a real ledger.

## See also

- `AGENTS.md` — repo-wide rules (the constitution).
- `.agents/ddd.md` — domain-driven design pipeline; this loop
  is the **mechanical heart** of a DDD aggregate.
- `.agents/multi-language.md` — Rust + Kotlin mirror rules;
  this loop applies to both.
- `.agents/testing.md` — the eight-layer test pyramid;
  iteration-level unit tests + ledger replay tests live here.
- `bench/AGENTS.md` — the runner-loop pattern (`load →
  drive → adapt → eval`) is the same shape, lifted one
  level up.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: **draft (possible mechanism)** — loop shape settled
  as a *candidate*; ledger shape, governance policy, tier
  thresholds, eviction triggers, and Open Questions still open.
  No final code; this is the deliberation, not the spec.