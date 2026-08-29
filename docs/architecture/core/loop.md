# Agent Loop — Driver / Turn / Iteration

> **Three nested layers**: a long-running `agent_loop` (driver),
> each user message starts an `agent_turn` (round), and each
> round is one or more `iteration`s of the **derive → invoke
> → judge → run** loop. The ledger is the **single source of
> truth** for durable state; the LLM never sees raw ledger, only a
> **budget-shaped surface** derived from it. **Governance is
> queried, not invoked** — every model-intent is judged. Runtime owns
> suspension and recovery for approval, cancellation, provider backoff, and
> uncertain external effects.

This is **deliberative**, not a spec. The normative boundary and lifecycle are
defined by `spec/design/agent-architecture.md` and
`spec/design/agent-execution-contract.md`; those specifications win if an
example here drifts from them.

> **Heads-up: this document is a *possible mechanism*, not
> final code.** Every pseudo-code snippet, every field name,
> every threshold (the 0–3 / 4–10 / >10 tier boundaries; the
> 3 / 10 / 30 Edit-class numbers; the eager / late / never
> eviction triggers) is a **draft design choice** that may
> change as the slice lands. The *stable* part is the loop
> **shape**:
>
> - Runtime driver / durable Turn / disposable Kernel Execution / iteration
>   ownership
> - Runtime durable facts as the single source of resumable state
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

### C. Driver + turn + durable facts (CHOSEN)

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
    #   - 未完成的 calls       → 按 durable receipt 分类恢复；不盲目重放

while not termination.done(state):

    # ① 预算感知推导（预算是 derive 的输入）
    surface = ledger.derive(kinds_for(this_mode), budget)
    if surface.needs_summary:
        #   纯推理报告：需要新摘要
        ledger.append(summarize(prefix))          # LLM-backed impure step
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
        prepared = tools.prepare(intent)          # invalid never authorizes
        invocation = runtime.allocate_and_commit(prepared)
        verdict = governance.judge(invocation, prepared)
        ledger.append(verdict)

        match verdict.decision:
            Approve(grant):
                effects = executor.run(invocation, prepared, grant)
                ledger.append(effects.items)

            Deny(reason):
                ledger.append(tool_result.rejected(reason))
                #   拒绝原因回喂模型（不中断）

            ReplacementRequired(x):
                # replacement is not approval; prepare and authorize anew
                continue_with_new_preparation(new_intent_from(intent, x))

            AskUser(question):
                ledger.append(approval_request{
                    question,
                    引用: verdict.seq,
                })
                return Suspended                 # governance-triggered suspension
```

### Layered Semantics

| Layer | Identity | Lifetime | Concerns |
|-------|----------|----------|----------|
| `agent_loop` | one per process | forever | event source, dispatch, lifecycle |
| `agent_turn` | one per user message | until `Done` or `Suspended` | entry protocol, resume from ledger |
| `iteration` | one per `while` pass | sub-millisecond to minutes | derive, invoke, judge, run |

### The Ledger's Role

The Runtime-owned durable store (detailed in [`ledger.md`](ledger.md)) is the
**single source of truth** for resumable turn state. It
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

Summarization and external execution are impure operations. Their durable
inputs, outputs, and recovery classification are recorded by Runtime; a write
to the durable store is not itself a pure projection.

### Execution-local projection

Each Kernel Execution receives an immutable request and constructs a small,
typed projection for that invocation only. It may cache iteration count,
normalized usage, elapsed time and the reconstructed recovery cursor. Core is
the only writer of this disposable projection; ports return facts and never
mutate it.

Suspension closes the current Kernel Execution. Runtime commits the typed
`Suspended` outcome and later creates a new request with the same `turn_id`, a
new `execution_id`, and a cursor reconstructed from durable facts. There is no
in-memory phase transition back to `Running` and no `resume()` method.

Durable answers—model calls, usage, approvals, tool receipts, cancellation,
suspension and terminal outcome—come from Runtime storage. Execution-local
caches are projections only and can always be discarded. The exact request,
control and outcome types are specified in
`spec/design/agent-execution-contract.md`.

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
  through Runtime ports (durable request/response and usage receipts).
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

### Two protocols — event stream + ledger stream

> **Disclaimer — names vs implementation.** The names
> "event stream" and "ledger stream" describe the **two
> properties the design needs** — low-latency best-effort
> notifications vs lossless-durable records. They are
> **related in content** (both observe the same
> `model.invoke` boundary), **different in form** (event
> stream = ephemeral payload; ledger stream = durable
> rows), and **different in use case** (UI / monitor vs
> audit / recovery). An event-stream implementation could be
> in-process pub/sub, a per-loop `Channel`, an SSE stream, a
> WebSocket, or even poll-based. The ledger-stream is the
> existing `entry.append` — durable, atomic per transaction.
> **This doc names the properties; the runtime picks the
> transport.**

The `turn_loop` skeleton runs **two protocols in parallel**
during a single round. Both observe the same `model.invoke`
boundary; they differ in **what they carry** and **what they
promise**.

| Protocol | What it carries | Failure tolerance | Recovery |
|----------|-----------------|--------------------|----------|
| **Event stream** | Model streaming tokens, thinking deltas, tool-start/tool-complete signals, governance decisions in flight | **Lossy — OK** to drop a token | None — the event is ephemeral |
| **Durable facts** | model request/response receipts, normalized usage, tool results, approvals, effect receipts, and turn terminals | **Lossless** for committed facts | Required — Runtime rebuilds recovery and reconnect snapshots from them |

The split is load-bearing:

- **Event stream** is for **live observability** — the UI
  shows the model thinking token-by-token, the debug tool
  watches tool execution, the monitoring system tracks
  latency. A dropped token means the UI flickers for one
  frame; the user is fine.
- **Ledger stream** is for **durable consistency** — every
  call produces normalized usage and response receipts; every request has a
  durable pre-invoke receipt; every cancellation produces a
  `provider.partial` row. A missing row means the round's
  accounting is wrong, the audit trail is broken, or the
  recovery on resume fails.

The two protocols have separate delivery guarantees — the event stream is
best-effort pub/sub; durable facts commit through Runtime storage. A subscriber
of the event stream failing
costs the UI a few frames; a ledger append failing
**halts the loop** (the next round can't recover).

### Where the two protocols split inside a round

```
turn_loop round (annotated with protocol split):

  [event]  ──── token-by-token model output  ───►  event bus
              ▲
              │ streaming
              │
  derive  ──► assemble  ──► model.invoke  ──►  neutral outcome
   │           │                                │
   │           │                                ▼
   │           │                       [runtime] commit:
   │           │                            - model.request (before dispatch)
   │           │                            - model.response + usage (terminal)
   │           │                            - provider.partial  (Interrupted)
   │           │                            - limit evidence    (Rejected/ContextOverflow)
   │           │                            - rejection fact    (Rejected)
   │           │                            - availability fact (Unavailable)
   │
   ▼           ▼
  surface    payload
```

- **Event stream** runs **inside** `model.invoke` —
  streaming tokens, partial reads, thinking deltas. The
  transport adapter owns this.
- **Durable recording** runs **at the boundaries** of `model.invoke` — before
  the call starts and when it returns, is cancelled, or fails. Runtime owns
  durability; the Agent consumes the neutral model port.

### Cancellation: the bridge

A **mid-stream cancellation** is the one place the two
protocols *almost* touch. When the user / loop / harness
cancels:

1. The event stream **stops emitting** (the transport
   closes the connection / cancels the iterator). Tokens
   received so far are **lost to the UI** — that's fine, the
   UI is reactive.
2. The bytes already received **must go to the ledger** as
   a `provider.partial` entry. This is the only way to
   recover on resume.
3. The current Kernel Execution returns a typed `Suspended` or `Stopped`
   outcome according to Runtime policy. Any later continuation is a new
   execution reconstructed from the durable partial receipt.

The cancellation rule — **partial goes to the ledger, not
to in-memory** — is what bridges the two protocols. See
`provider-adapter.md` "Cancellation semantics" for the
full design.

### Why two protocols (not one)

A single protocol would have to choose between the two
properties — **realtime lossless** is impossible for streaming
tokens (you'd buffer the entire stream before publishing it,
defeating the point). Splitting them lets each protocol pick
the right property:

- **Event stream** picks **realtime + lossy**. A dropped
  frame is invisible.
- **Ledger stream** picks **durable + lossless**. A dropped
  entry is a corruption.

A single protocol that *also* tries to do both would be
neither realtime nor lossless — it would be **batch-oriented
+ best-effort**, which is the worst of both worlds.

### Skeleton implication

The split is **structural**, not behavioural. The
`turn_loop` skeleton continues to do exactly one thing per
protocol:

- For the **event stream** — the skeleton publishes a
  sequence of events; the subscribers (UI / monitor / debug)
  consume them. Failure of a subscriber is invisible to the
  loop.
- For the **ledger stream** — the skeleton calls `entry.append`
  at the boundaries. Failure of `append` halts the loop (the
  round can't recover without its accounting).

Adding a new **event** kind is a pub/sub topic; no skeleton
change. Adding a new **ledger** entry kind is a `ledger.md`
catalog update; no skeleton change. The skeleton's contract
with each protocol is **already complete**; new content
plugs in without touching the loop.

### Cross-references

- `provider-adapter.md` "Outcome kinds" — the provider-neutral outcomes
  `model.invoke` returns, including `Interrupted` with durable partial items.
- `ledger.md` "Entry kinds" — every fact that affects recovery has a durable
  representation; ephemeral UI deltas do not.
- `assemble-testing.md` "Dim 1c — Real API smoke" — verifies
  the outcome kinds land in the ledger on every test.

### Layer reversibility — what each tier means

The 4 layers of the agent's runtime stack have **different
reversibility profiles**. Knowing which layer is which
determines where retry is safe, where governance is
mandatory, and where the ledger is the source of truth.

| Layer | What it solves | Reversibility | Retry safe? | Governance | Truth source |
|-------|---------------|---------------|-------------|-------------|---------------|
| **derive / assemble** | What the **model sees** | **Information** | ✅ Re-run from scratch | None — pure function | Surface cache |
| **model.invoke** | What the **model says** | **Generation** | ✅ Retry (idempotency key) | Provider-adapter (transport) | `model.usage` |
| **Effect layer** (governance + tool exec) | **What the world experiences** | **Irreversible** — touched the world | ❌ Re-run is dangerous | **MANDATORY** | Ledger (must record) |
| **Ledger** | **What happened** | Append-only | N/A | None — it's the record | N/A |

The rule:

> **derive/assemble are information; mistakes there lose
> no real work.**
> **model.invoke is generation; mistakes there are usually
> recoverable with retry.**
> **Effect layer is real-world action; mistakes there are
> irreversible. The model can't tell from inside whether a
> tool actually succeeded — the ledger has to.**

Consequence:

- **Effect layer must be gated by `governance.judge` before
  execution.** No intent reaches the executor without a
  verdict. The verdict is the only place where an
  irreversible decision is made; that's where the audit
  chain starts.
- **Effect layer must record the truth to the ledger.** A
  tool that succeeds, fails, times out, or cancels all
  write a `tool.result` (or `tool.result.rejected`) row.
  The model **never** sees an "I think it succeeded" — it
  sees the actual outcome.
- **Effect layer's idempotency must be classified.** A
  `ReadOnly` tool can re-run without risk; a `Mutating`
  tool cannot — re-running is the **original failure mode**.

### Recovery is graded by effect class

The `effect_class` declaration on each tool is the boundary
that decides what the runtime can safely re-run on a
Suspend / crash recovery:

| Class | Recovery policy | Why |
|-------|------------------|-----|
| `ReadOnly` | **Auto re-run** | No side-effects; result is the same; re-run is free |
| `Idempotent` | **Auto re-run** | Overwrite semantics; result is the same; re-run converges |
| **`Mutating`** | **DO NOT auto re-run** | **Otherwise the recovery *causes* the failure it's trying to undo.** Mark `interrupted`, feed the model a hint ("this call was interrupted at seq N, here's the partial output"), model decides (or `AskUser`) |

> **Irreversible cannot re-do. Otherwise it's secondary
> damage.**

The discipline: **trust the `effect_class` declaration**.
The runtime never overrides it. A `Mutating` tool that was
mid-run when the process crashed is **not** re-issued
automatically; the user / model decides. A `ReadOnly`
tool that was mid-run is re-issued freely.

### Effect layer's two-side rule

The effect layer operates on **two sides** — both required:

- **Pre-execution** (`governance.judge`): stop the model from
  doing irreversible damage. A hallucinated `delete` or a
  wrong-path `write` is caught here. **Without this side,
  a model hallucination is a real failure operation.**
- **Post-execution** (ledger): record what actually happened.
  A `tool.result` row is the source of truth. **Without this
  side, the model thinks it succeeded when it didn't.**

Both sides are required. Pre-execution alone is not enough —
the executor itself can fail. Post-execution alone is not
enough — the model can do something wrong before the executor
sees it.

### Why "all-serial" is too slow, "all-parallel" is too unsafe

The dispatcher's **conflict-graph scheduling** is the
middle ground:

- **Two tools writing the same file** → conflict edge →
  serialised (no torn writes).
- **Two `read_file`s on the same file** → no conflict edge →
  parallel.
- **Two `bash` tools (same process resource)** → conflict
  edge → serialised.

The conflict graph catches **what** must serialise. The
runtime never assumes "all Mutating tools need serialising" —
that would be needlessly slow; nor "all parallel" — that
would be unsafe.

### Tool result must round-trip honestly

A tool's outcome goes to the ledger as **exactly** what
happened — not what the model wished happened:

- **Succeeded** → `tool.result{status: ok, output: blob}`
- **Failed** → `tool.result.rejected{reason}`
- **Timed out** → `tool.result{status: timeout, output_so_far}`
- **Cancelled** → `tool.result{status: cancelled, output_so_far}`
- **Exception** (caught at workspace boundary) →
  `tool.result.rejected{reason: "exception: ..."}`

The model sees the **actual outcome**, never a fabricated
"success". This is the discipline that lets the agent's
self-healing ability operate — the model can't fix what it
doesn't know is broken.

A **half-completed** tool (e.g. exception mid-run) returns
its partial output. The model sees "I got this far, then
it failed". It can decide to retry, change strategy, or ask
the user — but it can't pretend it succeeded.

### Cross-references

- `effect-layer.md` "Failure semantics — data, not exception" —
  the **specific** failure kinds and where they're caught.
- `effect-layer.md` "Effect class + recovery profile" —
  ReadOnly / Idempotent / Mutating; the **recovery table**.
- `effect-layer.md` "Conflict-graph scheduling" — the
  scheduler that catches what must serialise.
- `effect-layer.md` "BDI architecture" — Intention / Filter
  / Action; the **filter** is `governance.judge`.

### Budget Projection (anchored to `model.usage`)

The earlier budget design had `derive` estimating the
surface's token cost and asking the loop to truncate when
estimated > budget. That was **pure estimation** — no ground
truth. The candidate design anchors estimates to normalized provider usage.
This remains a hypothesis until provider-specific usage fields and tokenizer
evidence are verified; billed tokens are not assumed to equal context-window
occupancy.

#### The formula

```
budget_projection(this_round) =
    last_actual       # anchor: real cost from previous round's model.usage
    + est(this_round.current_surface)      # today's surface, estimated
    - est(this_round.prev_actual_surface)   # yesterday's surface, estimated
```

- `last_actual` is the previous request's normalized
  `context_input_tokens`, not a billed-cost total.
- `est(...)` is `derive`'s budget-aware estimate of the
  surface's token cost, in the same units the provider
  would count.
- The **difference** is the signal — if the new surface has
  more entries than the old, the budget grows; if it has
  fewer (compaction), the budget shrinks. The estimate's
  absolute accuracy doesn't matter, only its *change*.

#### Context budget vs billing

- **Context budget** uses normalized input occupancy plus reserved output.
- **Billing** uses provider-reported categories and the price schedule pinned
  to the exact provider/model revision.
- **Cache accounting** remains separate because providers include/exclude
  cached input differently. Each adapter documents its normalization.

```python
class Tokens:
    context_input:     u32   # normalized context-window occupancy
    output:            u32
    cache_read_input:  u32 | None
    cache_write_input: u32 | None
    provider_raw:      Value # verified provider usage for audit
```

#### Boundary conditions (fallback to pure estimation)

The anchored projection has **three fallback paths** to
pure estimation, when the anchor is unavailable or
invalid:

| Condition | Behaviour |
|-----------|-----------|
| **First round** of a session (no `last_actual`) | Pure `est(surface)` — the model.usage anchor doesn't exist yet. |
| **`model_id` changes mid-session** | The previous anchor was for a different tokenizer; the diff is meaningless. Reset to pure `est(surface)`. The next round will have a fresh anchor. |
| **Provider emits malformed usage** | Runtime retains the raw receipt, marks normalized usage invalid, and falls back to pure estimation without inventing fields. |

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

Normalized usage and the durable request receipt share an exact `request_id`.
The request receipt describes the surface and points to the request artifact:

```sql
-- "Show me what the model saw and what it cost."
SELECT r.request_digest, r.artifact_ref, u.normalized_usage, u.provider_raw
  FROM model_request r
  JOIN model_usage u USING (request_id)
 WHERE r.turn_id == ?
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
- **D6. Usage terminal atomicity.** The request receipt is committed before
  dispatch and both records share `request_id`. Whether response and usage
  commit in one transaction remains a Runtime storage decision.
- **D7. Usage projection semantics.** Durable normalized usage is authoritative.
  A Kernel Execution may cache the total admitted by its request, but it must
  preserve unknown counts and must not infer cross-Turn billing totals.

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
| **Adjust a tier policy** (e.g. raise `tool.result` from tier-1 to tier-0 for the most recent 5 iterations) | `derive` | The tier-policy table in this document updates. `assemble` reads the tier from the cache and renders accordingly. |
| **Change the kinds of a projection** (e.g. `GOVERNANCE_INPUT` now needs `text.user` too) | `derive` | The projection's `kinds` set updates. `assemble` simply doesn't filter them out. |
| **Add a new model-visible kind** (a new `compaction.*` flavour, etc.) | `derive` | Add the internal kind and projection rule; add a wire schema only if it crosses an admitted boundary. |
| **Change the pinned block** (which kinds are always-loaded) | `derive` | The pinned set updates. `assemble` still renders the pinned block as the head; only the *contents* of the head change. |
| **Change the layout mode set** (add a new mode like `striped` for A/B testing) | `assemble` | The layout function updates. The pinning and tier decisions in `derive` don't change. |
| **Add a new projection** (a new view like `DIFF_VIEW` for round-vs-round comparison) | `derive` + `assemble` | A new branch in the dispatch; but the *content decisions* are the existing rules, only the *serialisation* is new. |
| **Change the delta-fragment policy** (e.g. seen part is the *last 2* iterations instead of the *last iteration*) | `derive` | `last_seen_seq` definition updates. `assemble` reads it; the new boundary is the new cache key. |
| **Change the execution boundary** (suspension / continuation rules) | Runtime request reconstruction | Runtime changes the durable cursor projected into the next request. `derive` and `assemble` still operate on the admitted cursor. |

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

The **delta boundary** (`last_seen_seq`) is a durable Runtime fact referenced by
the request cursor. Suspension closes the execution. If Runtime later
continues the Turn, it reconstructs that boundary into a new request, so the
next `assemble` can preserve the provider cache's stable prefix without
depending on in-memory state.

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
  happens **after** `model.invoke` emits an intent and C4 has validated it.
  The loop asks Runtime to authorize the immutable Prepared Call before
  calling the execution port with its exact invocation and grant.
- It does **not** call `summarize`. `summarize` is a
  *write* op (it appends a `compaction.summary` row to the
  ledger), not a read op. `assemble` only reads.

`assemble` is **read-only over the cache**; all the writes
happen elsewhere (`summarize` writes summaries;
`governance.judge` writes verdicts; `executor.run` writes
`tool_result` and effects; the loop writes the
`model.usage` rows; Runtime durably records the exact request receipt before
the provider call starts).

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

# Runtime has already committed the request receipt before invoke
```

#### Derive / Assemble Receipt (durable request description)

`derive` and `assemble` are not just **read** operations —
they are **read + describe**. Each call returns the data
the consumer asked for, **plus a receipt** that records exactly what that call
did. Before `model.invoke` starts, Runtime commits the receipt, exact
`request_id`, model coordinate, request digest, and an opaque request-artifact
reference. An in-memory buffer is insufficient because a crash after provider
dispatch would otherwise lose the evidence needed to classify recovery.

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

**Receipt storage.** The physical table is a Runtime storage decision. The
logical contract is a durable `model.request` fact keyed by `request_id`, not
an eventually flushed analytics row. It must commit before network dispatch
and link to the later response/usage receipt:

```sql
BEGIN;
INSERT INTO model_request
  (request_id, turn_id, model_coordinate, request_digest, artifact_ref, state)
VALUES (?, ?, ?, ?, ?, 'prepared');
COMMIT;
# provider dispatch may begin only after this commit
```

The full request bytes may remain in encrypted/artifact storage, but the
identity, digest, coordinate, state, and artifact reference are durable facts.
Whether they share a physical table with other entries is not decided here.

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

`record_request(receipt)` is an idempotent Runtime operation keyed by
`request_id`. Reusing the ID with a different digest is a conflict:

```sql
INSERT INTO model_request (...) VALUES (...)
ON CONFLICT(request_id) DO UPDATE SET request_id = request_id
WHERE model_request.request_digest = excluded.request_digest;
```

Response, usage, cancellation, and terminal facts link to the same
`request_id`. Recovery after dispatch is receipt-based; it never assumes that
absence of a response means the provider was not invoked.

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
| `state_progress` | Current durable Turn outcome and the active execution cursor, if any. | Runtime facts + request cursor |
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
- **A pending `approval_request`** must stay outside the covered range. A
  resolved request/response pair may be fully inside. A
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

`governance.judge(invocation, prepared, state)` needs to evaluate **one
Prepared Call** at a time. The projection gives it:

- The immutable **Prepared Call** plus its Runtime invocation identity.
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
            tiers_old = surface.cursor.iteration_count - e.approx_iteration
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

Every intent the model emits (tool call, file write, network request) is first
prepared, then **judged** by `governance.judge(invocation, prepared)` before it
runs. The verdict is one of:

| Verdict | Action |
|---------|--------|
| `Approve(grant)` | execution port runs the exact invocation and Prepared Call bound by the grant |
| `Deny(reason)` | a `tool_result.rejected(reason)` is appended; the rejection is fed back to the model next iteration (no termination) |
| `ReplacementRequired(x)` | reject the old preparation; create a new intent, digest, invocation identity, and authorization decision |
| `AskUser(question)` | an approval request is committed and the execution returns `Suspended(ApprovalRequired)` |

Governance is **queried** — the loop asks `governance.judge`,
not the other way around. This keeps the loop the single sequencer while
Runtime owns authority inputs and durable decision facts.

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
3. Incomplete model/tool invocations are classified from durable receipts as
   retryable, receipt-recoverable, or operator-required. An unpaired call is
   never blindly executed or given a synthetic success result.

## Consequences

### Positive

- **Recoverable under an explicit contract.** Runtime rebuilds state from
  committed facts and exact receipts. Uncertain external effects fail closed
  to operator reconciliation instead of being replayed.
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

Rust under `engine/core/` is production-first. Kotlin experimentally checks only
the slices admitted by `cross-language-agent-contract.md`; C0-C3 are supported,
while the entire loop is not implicitly in lockstep.

## Open Questions

1. **Ledger shape**: append-only vs bitemporal; per-turn
   segments vs global log. (See forthcoming
   [`ledger.md`](ledger.md).)
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
- `.agents/multi-language.md` — optional implementation admission and
  conformance levels.
- `.agents/testing.md` — test categories and evidence maturity.
- `bench/AGENTS.md` — the runner-loop pattern (`load →
  drive → adapt → eval`) is the same shape, lifted one
  level up.

## TurnBudget — earlier approach (superseded)

> **Earlier approach — preserved for evolution history.**
> This section documents the first iteration of the guardrail
> concept. The current approach (ProgressGuardian, below)
> supersedes it: the four axes stay as detection inputs,
> but the **hard caps** are removed in favour of useless-work
> detection. Read this as **"how the design got here"**, not
> the current contract.

### Original TurnBudget design

A four-axis budget per turn, with hard caps that emit a
summary entry when breached:

```
max_tool_calls_per_round:    32    # default
max_tool_calls_per_session: 1024   # default
max_tool_time_ms:           60_000 # per-tool, per-round cumulative
```

When a cap is hit, the runtime emits a graceful-exit summary
entry rather than an error — the round continues with the
partial result and the model sees "you've used your budget
for this round; here's where we are".

### What TurnBudget did well

- **Explicit numeric guardrails** — the four axes are
  observable, the caps are deterministic, the runtime can
  assert "you're within budget" with no analysis.
- **Graceful exit** — a summary entry on breach gives the
  model context for what to do next, rather than a hard stop.
- **Cost-visibility** — token and wall-clock axes are real
  cost dimensions; capping them is a real cost control.

### What TurnBudget missed

- **Doesn't catch useless-work** — the caps trigger **on
  count, time, cost**, not on **whether the work made
  progress**. An agent can:
  - Call `pytest` 100× with the same `ImportError: X` (count
    cap trips at 32, but the work was already useless
    earlier).
  - Oscillate file edits A → B → A (count caps don't catch
    regression).
  - Open new directions without finishing any (count caps
    don't catch divergence).
- **Hard caps feel arbitrary** — when the cap trips, the
  agent is left wondering "but I was making progress?". The
  cap doesn't distinguish progress from grind.
- **No escalation path** — the breach summary is the only
  signal; there's no path to AskUser or Suspend.

ProgressGuardian (below) replaces TurnBudget's cap-trip
recovery with **detection + intervention**. The four axes
remain useful as **inputs to the detection** (token
consumption rate → efficiency-ratio signal) — they're no
longer the cap, they're the data.

## Interruption semantics — queue vs redirect

> **Interruption ≠ pause.** Pause = "wait for a human"
> (approval / governance suspension). Interruption = "user
> changed their mind — redirect". The two paths have
> different defaults, different costs, and different
> mechanisms.

The driver layer's default policy is **queue, not interrupt**.
Interruption is an **explicit** action the user must take.

### Default path — queue (new messages during a running turn)

The turn is running (potentially for several minutes). The
user sends a new message. The driver layer:

1. **Immediately writes the message to the ledger**
   (`kind = user.queued`). The truth lands **first**; the
   user's message is durable even if everything else fails.
2. Enqueues the message in the per-session **pending queue**.
3. **Acknowledges to the UI** — "received; current task
   continues; will process after".

The current turn finishes naturally (Done or Suspended).
The driver layer dequeues the first pending message and
**opens a new `agent_turn(Fresh)`** with the queued message
as the new user entry. The new turn's `derive` projects the
queued message onto the surface naturally — the model sees
it and decides what to do.

> **Direction change is the model's job, not the mechanism's
> job.** The runtime does not interpret the new message;
> the model reads it and adapts. The queue is just delivery.

Multiple messages pile up — all enter the ledger, all queue,
the driver processes them in order. **The queue loses
nothing** — it just delays.

### Explicit path — interrupt (cancel and redirect)

Trigger: **cancel word** (TUI Ctrl+C, explicit directive
"cancel current task"). The driver layer sends a
**collaborative cancel signal**. The signal lands at **four
sites** — all already designed.

| Site | Behaviour on cancel |
|------|----------------------|
| **① Skeleton** — top of each iteration | One-line check: `if cancel_requested: transition = interrupted_by_user; break`. |
| **② `model.invoke`** — streaming consumer | Stop the streaming iterator. **Already-emitted tokens land in `assistant.partial`** — the user's cancel is preserved in the ledger. |
| **③ ToolDispatcher** — in-flight tools | Cooperative cancel yielded to in-flight tools. Each tool returns `tool.result{status: cancelled, output_so_far}`. |
| **④ Iteration boundary** — clean shutdown | The current iteration completes cleanly (no torn writes), turn ends with `transition = interrupted_by_user`. |

After step ④, the driver layer dequeues the redirect
message and opens a new `agent_turn(Fresh)`. The new turn's
`state.phase` starts fresh; the cancellation is recorded
in the ledger for audit.

### Why default is queue, not interrupt

> **Interruption's cost is real.** Tokens already spent don't
> come back. Side effects already taken still apply. The
> current iteration's half-finished work needs cleanup.
> Queueing only **delays**; interruption **destroys** the
> work in flight.
>
> That's why interruption is an **explicit** action — the
> user has to mean it. The default interpretation of a new
> message is **"the user is fine, they're just typing"**,
> not **"the user changed their mind"**.

The cost asymmetry:

| | Queue | Interrupt |
|---|-------|-----------|
| Token spend | preserved (counts toward budget) | preserved (can't un-spend) |
| Side effects | preserved (don't double-do) | preserved (might roll back) |
| Iteration work | preserved (no re-do needed) | discarded (must redo) |
| User's new message | delivered when current turn ends | delivered now (current turn torn down) |
| Cost | zero | non-zero (waste + cleanup) |

### Implementation — no new mechanism

This is **zero new code**. Every site already has its design:

- `model.queued` — new `kind` in the **user/model** category
  (per `ledger.md` "Entry Kinds — ten categories").
- Pending queue — `state.pending_messages: list<Entry>`
  (per `loop.md` "The Turn State" — append to the state struct).
- Skeleton cancel check — **+1 line** at the top of the
  iteration loop (consistent with `loop.md` "convergence"
  rule: "new capabilities land in the seam, not the skeleton").
- `provider-adapter.md` "Cancellation semantics" — full
  streaming cancel already designed.
- `effect-layer.md` "Cancellation: collaborative" — cooperative
  cancel for in-flight tools already designed.
- `loop.md` "Compression scope split" — transition reason
  `interrupted_by_user` is a new value in the existing enum.

The driver layer **wires** these. No new mechanism.

### Cross-references

- `loop.md` "The Turn State" — `state.pending_messages` is the
  field the queue lives in.
- `ledger.md` "Entry Kinds" — `user.queued` is a new kind in
  the user/model category.
- `provider-adapter.md` "Cancellation semantics" — already
  designed for `model.invoke` (site ②).
- `effect-layer.md` "Cancellation: collaborative" — already
  designed for tools (site ③).
- `loop.md` "Convergence audit" — Interaction (queue / redirect)
  was previously listed as a tail-end item; **resolved** by
  this section.

## ProgressGuardian — useless-work detection (supersedes TurnBudget)

> **护栏从"限额"换成"识功"。** ProgressGuardian doesn't
> limit how many calls you can make; it ensures you don't
> spin in place. The only "upper limit" preserved is the
> **human judgment** at escalation tiers 3 / 4.

### Why — useless-work is the #1 agent failure mode

Recent agent error-mode analysis ranks **stuck-in-loop** as
the single most common failure mode. The four shapes:

| Pattern | Characteristic | Example |
|---------|-----------------|---------|
| **重复** | Same / similar action repeated | Same command run 3× in a row |
| **撞墙** | Same error keeps recurring | Every iteration hits the same `ImportError: X` |
| **振荡** | A → B → A back-and-forth | File edited, then reverted, then re-edited |
| **发散** | Many actions, no convergence | New directions opened, nothing finished |

A **digital cap** (`max N calls per round`) doesn't catch any
of these — the agent can do unlimited useless work. A
**detection + intervention** design catches all four.

### 5 detection signals — derived from a ledger window

Every iteration boundary, the runtime runs a **window
analysis** over the last K entries of the ledger:

| Signal | What it checks |
|--------|-----------------|
| **① 调用相似度** | `tool.call` (name + args) hash repeat rate |
| **② 错误模式** | `tool.result{error}` message similarity clustering |
| **③ 进展产物** | Window contains "forward motion" — successful writes, tests failing → passing, diff growth |
| **④ 振荡检测** | Edit sequence on the same file goes back to a prior hash |
| **⑤ 效率比** | token_spent / useful_output ratio, trend over time |

> **关键性质**：检测是**账本的纯分析** — 进展本身变成可推导的东西，和 surface 同构。**零额外数据源**。

### 4-tier response ladder — escalating, never auto-killing

Detection triggers a **ladder**, not a hard stop. Each tier
brings **evidence** to the model — never just "stop doing
that".

| Tier | Trigger | Response |
|------|---------|-----------|
| **1 — 提醒** | First useless-work signal | Inject a reminder entry into the surface — **with evidence**: "you've run `pytest` 3 times in a row with the same `ImportError: X`; repeating the same action won't yield a different result" |
| **2 — 反思** | Signal persists after the reminder | Stronger reflection prompt + show the full detected pattern (which file, which error, how many times) |
| **3 — AskUser** | Still no progress after tier 2 | `governance.approval_request` — **show the evidence to the user**, ask for direction |
| **4 — Suspend** | Headless (no human available) | `state.phase = Suspended` — wait for a human |

**No tier is "auto-kill".** Termination decisions always
have evidence behind them. The **only** "upper limit"
preserved is the **human judgment** at tiers 3 / 4.

### Architecture — one-line wiring

| Aspect | Decision |
|--------|----------|
| Component | `ProgressPolicy` (L1 endpoint); default impl = ledger-window analyser |
| When | Skeleton iteration boundary — same place as `usage` recording, **one query** |
| Result | `progress.alert` row in the ledger — fully auditable ("when was this detected, what was the reminder") |
| Inject | Reminders go through the **reminder-injection channel** (`loop.md` "Two protocols" — has its own `kind`, subject to surface eviction policy) |
| Skeleton change | **+1 query** — continues the "new capabilities land in the seam, not the skeleton" rule |

### Theoretical grounding

- **Patience / plateau detection** — from ML training's
  `early stopping with patience`: monitor a metric, tolerate
  a window, change strategy when no improvement. Same idea —
  the metric is "progress signal" instead of training loss.
- **Agent failure-mode taxonomy** — recent research ranks
  stuck-in-loop as the **#1 agent failure mode**. The detection
  here targets exactly that enemy.
- **Evidence-based intervention** — saying "stop repeating"
  is useless. Feeding the model the **specific detected
  pattern** lets it actually switch strategy. **Evidence-based
  reminders are the most valuable part of this design**.

### Cross-references

- `loop.md` "Two protocols" — reminder injection channel
  (lossy event + durable ledger rows).
- `ledger.md` "3. Instruction family" — `progress.alert` is a
  new kind in the **notification** family.
- `loop.md` "TurnBudget — earlier approach" — the design
  that ProgressGuardian supersedes (above); four axes remain
  as detection inputs.

## Convergence audit — `turn.loop` design closed

The `turn_loop` skeleton has reached **architectural
convergence**. Multiple rounds of deep-dive (model.invoke
refinement, effect-layer safety, budget anchoring,
contract correctness, projection determinism) added
mechanism without changing control flow — exactly the
design criterion we set up front.

### Status of `turn.loop`'s 9 modules

| Module | Status | Where it lives |
|--------|--------|------------------|
| 0 — Recovery entry (suspend / resume) | settled | `loop.md` + `ledger.md` |
| 1 — Derive (6-step pipeline) | settled | `loop.md` + `derive-testing.md` |
| 2 — Assemble (5 responsibilities) | settled | `loop.md` + `assemble-testing.md` |
| 3 — Effect layer (contracts / dispatch / security) | settled | `effect-layer.md` |
| Surface / ledger schema | settled | `ledger.md` |
| Instruction family (10 categories) | settled | `ledger.md` |
| Navigation ops (undo / redo / branch / compaction / redact) | settled | `loop.md` + `ledger.md` |
| Driver loop (long-lived / Fresh / Resume) | settled | `loop.md` |
| Suspend / Resume (unpaired detection + position derivation) | settled | `loop.md` |

### What landed this turn

| Mechanism | Module |
|-----------|--------|
| Two protocols (event + ledger) | `loop.md` |
| 8 outcome kinds for `model.invoke` | `provider-adapter.md` |
| Cancellation semantics (partial ledger-挂账) | `provider-adapter.md` |
| AIMD + circuit breaker per-pool | `provider-adapter.md` |
| Multi-model dispatch + `request_id` | `provider-adapter.md` |
| Adaptive compression (5-step + 4-layer) | `compression.md` |
| Budget projection anchored to `model.usage` | `loop.md` |
| Layer reversibility principle (4 tiers + 2-side) | `loop.md` |
| 5-effect-layer contract (intake / governor / dispatcher / workspace / security) | `effect-layer.md` |
| Failure semantics (data, not exception) | `effect-layer.md` |
| Tool discipline (few / well-designed / composable) | `effect-layer.md` |
| Effect class + recovery profile (ReadOnly / Idempotent / Mutating) | `effect-layer.md` |
| Conflict-graph scheduling (max parallelism under correctness) | `effect-layer.md` |
| Branch × snapshot workspace (ledger branch → physical isolation) | `effect-layer.md` |
| Read-cache (same-args free, with staleness handling) | `effect-layer.md` |
| Tool-call event contract (start/terminal pairing) | `effect-layer.md` |
| L2 dispatchers (Gated/Passthrough/Sequential) | `effect-layer.md` |
| Tool registry + DynamicToolSource (kimi-style late binding) | `effect-layer.md` |
| Background tasks (`start_background_task`) | `effect-layer.md` |
| BDI architecture framing (Belief/Desire/Intention/Filter/Action) | `effect-layer.md` |
| Deterministic simulation tests (E1–E5 + pairing invariant) | `effect-layer.md` |
| Tool double-declaration (`effect_class` + `accesses`) | `effect-layer.md` |
| Tool result security (provenance + taint) | `effect-layer.md` |
| Tool health + budget + schema versioning + leases | `effect-layer.md` |

### Tail-end — 4 contracts to fill

The remaining items are **contracts to fill**, not new
architecture — a single round of discussion lands them:

| Item | Depth | Where |
|------|-------|-------|
| **StopPolicy** — termination set semantics + transition-reason enum + `max_iterations` | 浅 | `loop.md` |
| ~~**TurnBudget**~~ — four-axis + graceful exit (summary, not hard-stop) | done — **superseded** by ProgressGuardian (preserved above as "earlier approach") | `loop.md` |
| **Interaction** — user sends a new message mid-turn: queue vs cancel-and-redirect (one rule) | 浅 | `loop.md` |
| **EventCatalog** — `AgentEvent` full enum + subscribe / back-pressure + `EventOrderingChecker` | 浅-中 | `loop.md` |

### Documentation debt — 2 recap docs to land

Two design records **only live in conversation**, not yet
filed in the repo:

| Doc | Contents |
|-----|---------|
| `docs/architecture/core/loop-invoke-and-effect.md` | `model.invoke` full design + effect-layer upgrade (contracts + conflict dispatch + security two-side) |
| `docs/architecture/core/loop-context-pipeline.md` | derive / assemble upgrades + anchor accounting + bench scheme |

These are **recap commits**, not new design — the substance is
already in `loop.md` / `provider-adapter.md` / `effect-layer.md` /
`compression.md`. Filing them moves the **conversation memory**
into the **repo's documentation ledger** (where future
contributors can find it).

### The next continent

`turn_loop` closed. By dependency order, the remaining
**continents** are:

1. **Memory layer** — `dream` (long-term memory extract),
   recall, cross-session `goal` ownership. Personal agents
   live here.
2. **Composition root + configuration** — pattern assembly,
   kind registry's concrete form, `RecoveryPolicy` table's
   runtime shape.
3. **Channel layer** — TUI / DingTalk / ACP event subscription
   + rendering.
4. **First implementation slice** — `Step 1`: ToolDispatcher
   extraction (per the user's plan).

`turn_loop` is no longer the bottleneck — implementation can
start.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: **draft (possible mechanism)** — loop shape settled
  as a *candidate*; ledger shape, governance policy, tier
  thresholds, eviction triggers, and Open Questions still open.
  No final code; this is the deliberation, not the spec.
