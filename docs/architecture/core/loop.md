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

### The LLM Never Sees the Ledger

`ledger.derive(kinds, budget)` projects the ledger into a
**budget-shaped surface** — a token-bounded slice of round
history. The LLM is given only this surface. This guarantees:

- The model cannot be poisoned by raw ledger entries.
- The model cannot ask the runtime to do something the
  runtime didn't already know.
- The context window is enforced before the call, not after.

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
- Status: draft — loop skeleton settled; ledger shape,
  governance policy, and Open Questions still open.