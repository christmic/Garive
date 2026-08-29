# Effect Layer — tool execution

> **Mechanism research, not the normative effect contract.** Accepted C4/C5/C6
> Specs own Prepared Calls, sequential v1 governance, receipts and recovery.
> Batch/concurrency, dispatcher names and pseudo-code below remain proposals
> unless a focused accepted Spec restates them.

> After `model.invoke` returns, the Agent prepares model intents and asks
> Runtime to **authorize**, **dispatch**, and **execute** them. This doc covers
> the boundary below
> the authorization port — preparation, dispatch, execution, and recovery.
>
> `loop.md` describes the effect-layer outline as one stage of
> the round; this doc is the **detail**. The boundary between
> provider-adapter (transport) and effect-layer (semantic
> output) is load-bearing — see the Boundary section below.

## TL;DR

```
model.usage.reply.intents
       ↓
Stage 1 — validate + prepare immutable calls
       ↓
Stage 2 — Runtime authorization  →  invocation grant
       ↓
Stage 3 — ToolDispatcher  (batch / concurrency / streaming / timeout)
       ↓
Stage 4 — Runtime execution adapter  →  bounded environment
       ↓
tool.result / tool.result.rejected → ledger
```

The four stages form one effect chain. Runtime durably binds prepared input,
authorization, execution identity, receipt, and model-visible result.

## Boundary — effect-layer vs provider-adapter

Provider-adapter recovers **transport failures** (5xx / 429 /
413 / truncation / cancel / model-down). Effect-layer
recovers **semantic output failures** (model returns invalid
arguments, schema mismatch, missing fields). The boundary:

| Failure | Where | Why |
|---------|-------|-----|
| Provider returns 413 | **provider-adapter** | It's a wire-level budget issue |
| Provider returns 5xx | **provider-adapter** | It's a transport issue |
| Provider cancels mid-stream | **provider-adapter** | Partial goes to ledger (`provider.partial`) |
| Model returns `{path: null}` to a `read_file` | **effect-layer** | The model produced bad output; re-issuing the same call would reproduce the bug. The effect layer must make the model **own its output**. |
| Model returns a tool name that doesn't exist | **effect-layer** | Schema violation by the model |

The boundary rule:

> **If the failure is about the wire, it's provider-adapter's.
> If the failure is about what the model said, it's effect-layer's.**

## Stage 1 — validate and prepare immutable calls

The model's reply carries one or more intents. Agent code validates the tool
name and schema, then produces an immutable prepared call. The model's
`call_id` is correlation data, not a recovery or idempotency identity:

```python
class PreparedToolCall:
    model_call_id: str       # untrusted correlation value
    tool_name: str           # exact admitted provider-neutral name
    tool_revision: str       # exact registered definition
    normalized_args: Value   # validated and immutable
    input_digest: sha256
    requirements: ExecutionRequirements
    replay_class: ReplayClass
```

Runtime allocates a `ToolInvocationId` only after preparation. Reusing that
Runtime identity with another digest or tool revision is a conflict.
Invalid model output becomes a model-visible preparation failure; it never
reaches authorization or an executor.

## Stage 2 — Runtime authorization

Each prepared call goes through an injected authorization port. Runtime derives
actor authority from authenticated product state and returns a grant bound to
the exact invocation, digest, tool revision, target, and limits.

| Verdict | What the dispatcher does |
|---------|---------------------------|
| `Approve` | Commit the grant, then dispatch the exact prepared call. |
| `Deny(reason)` | Write `tool.result.rejected{reason}`; feed reason back to model next iteration (no termination) |
| `ReplacementRequired(x)` | Reject the old preparation and create a new prepared call/digest/invocation; authorization never mutates an approved call in place. |
| `AskUser(question)` | Write `approval_request`; round returns `Suspended` (per `loop.md` "Stage ④") |

Authorization evaluates requested capabilities, but a declaration does not
enforce isolation. The selected Runtime executor must truthfully enforce the
filesystem, process, network, and resource limits carried by the grant. If it
cannot, execution fails closed before the operation starts.

## Durable execution lifecycle

External-effect recovery is part of the execution contract, not a later ledger
repair. Runtime advances a monotonic program counter:

```text
Prepared
  -> Authorized
  -> Started
  -> EffectCommitted(receipt)
  -> ResultRecorded
```

| Recovery position | Decision |
|---|---|
| Before `Started` | Retry with the same invocation after revalidating the frozen grant. |
| `Started`, no receipt | Retry only when the replay class and executor prove it safe; otherwise `OperatorRequired`. |
| Receipt exists, result absent | Recover from the receipt; do not execute again. |
| Result exists | Return the recorded result idempotently. |

Replay classes are explicit:

| Class | Example | Recovery |
|---|---|---|
| `ReadOnly` | bounded file read | Same-ID retry after input/target validation. |
| `Idempotent` | API supporting an idempotency key | Same-ID retry through the verified adapter. |
| `ReceiptRecoverable` | workspace mutation journal | Recover/finish from committed receipt. |
| `NeverReplay` | arbitrary command, payment, message send | Uncertain `Started` state requires operator reconciliation. |

The absence of `tool.result` never proves that the effect did not commit.

## Stage 3 — ToolDispatcher

The dispatcher owns the **policy** of how tools run:

### Batch policy

A single model reply can carry N intents. The dispatcher
decides whether to run them sequentially or in parallel:

| Default | All intents in a single turn run **sequentially** (deterministic) |
| **Parallel** | Model requests parallelism; preparation retains it as an untrusted scheduling hint. |
| **Mixed** | Read-only tools run in parallel; write tools run sequentially |

The model's claim of "parallel" is **advisory** —
Runtime may schedule sequentially regardless of the hint; authorization and
dependency analysis take precedence.

### Concurrency policy

Per-tool **concurrency limit** (e.g. `bash: 2`,
`read_file: 16`, `web_fetch: 4`). AIMD per-tool:

```
on tool.success:       tool.concurrency += 1      # additive
on tool.timeout:       tool.concurrency /= 2      # multiplicative
on tool.policy_violation: tool.concurrency -= 1    # permanent cut
clamp(tool.concurrency, 1, tool.max_concurrency)
```

The AIMD values are a provisional policy, not an accepted default. Concurrency
must also respect target and global limits. Failure meanings differ:

- `bash` timeout → reduce concurrency (provider likely busy)
- `read_file` timeout → unusual (filesystem I/O); reduce less
- `web_fetch` 4xx → reduce (provider limit hit)

### Streaming policy

For long-running tools (`bash`, `web_fetch`, custom tools):

- **Stream stdout/stderr to the event stream** (live UI shows
  the output as it lands). The bytes are lossy-OK — dropped
  frames are fine.
- **Buffer to a `tool.result` entry** at completion. The bytes
  in the entry are **lossless** — the full output is there
  for audit / replay.

The two-protocol split (see `loop.md` "Two protocols") is
exactly this: live bytes go to the event stream; durable
bytes go to the ledger via `tool.result`.

### Timeout policy

Per-tool `timeout_ms` (default 30 s):

| Tool | Default timeout |
|------|-----------------|
| `read_file` | 5 s |
| `bash` | 30 s |
| `web_fetch` | 20 s |
| `grep` | 30 s |
| (custom) | configurable |

When a tool times out:

- The partial output so far is buffered to `tool.result{status: timeout, output: blob}` — **lossless** for audit
- The model gets a typed `tool.result{status: timeout}`; rejection is reserved
  for authorization/preparation denial
- The runtime's tool.concurrency is **multiplicatively** reduced (AIMD)

## Stage 4 — tool implementation + workspace

Every Agent-visible tool definition provides preparation metadata. Concrete
execution is a Runtime adapter selected before the turn:

```python
class ToolDefinition:
    name:           str
    description:    str
    schema:         dict               # JSON Schema for args
    requires:       Capabilities       # filesystem | process | network
    timeout_ms:     int
    concurrency:    int                # AIMD-adjusted
    max_concurrency: int                # clamp upper

class ExecutionPort:
    async def execute(call: PreparedToolCall, grant: InvocationGrant) -> ToolReceipt
```

`Workspace` carries the bounded environment:

```python
class Workspace:
    session_id:     uuid
    fs_root:        Path               # /workspace/<session_id>/
    process_runner: ProcessRunner      # subprocess with restricted env
    network:        NetworkPolicy      # deny by default
    env_overrides:  dict               # allow-list, not arbitrary
```

### Workspace is the boundary

The Runtime execution adapter is the boundary between **tool power** and
**authorization policy**. Policy approves requirements; the adapter enforces
them. Unsupported or unverifiable enforcement fails closed.

| Tool class | `requires` | Workspace behavior |
|------------|------------|---------------------|
| `read_file` | `filesystem` | Path is normalised and **rooted at `/workspace/<session>/`**. Path traversal blocked. |
| `bash` | `filesystem + process` | Structured argv avoids an implicit shell; an explicit shell command remains untrusted input. Env is an allow-list. Network denial must be enforced by the selected executor. |
| `web_fetch` | `filesystem + network` | URL is allow-listed by host. Output is captured as a blob. |
| `grep` | `filesystem` | Recursive glob is rooted. |

## Effect round — example

Model reply:

```
1. intent(name="read_file", args={"path": "/workspace/foo.txt"}, call_id="c1")
2. intent(name="bash",     args={"cmd": ["ls", "/workspace"]},    call_id="c2")
```

ToolDispatcher:

1. `c1`: `read_file` — sequential (default), concurrency 16, timeout 5 s
2. `c2`: `bash` — sequential after `c1`, concurrency 2, timeout 30 s, **network disabled**

Results → ledger:

- `governance.verdict{decision: approve, evidence_ref: c1}` + `tool.result{call_id: c1, status: ok, output: blob}`
- `governance.verdict{decision: approve, evidence_ref: c2}` + `tool.result{call_id: c2, status: ok, output: blob}`

If `bash` is detected to attempt `network`, `governance.verdict{decision: deny, reason: "network not in workspace"}` + `tool.result.rejected{reason: "..."}` — feedback to the model.

## Effect ↔ Ledger — the audit chain

| Stage | Ledger row(s) |
|-------|---------------|
| Intake (intent) | (already in `model.usage` — `reply.items`) |
| Judge | `governance.verdict{decision, rule_id, evidence_ref}` |
| Deny | `tool.result.rejected{reason}` — feedback to model next iter |
| Approve (run) | `tool.result{call_id, status, output: blob}` |
| Effects | `executor.effects{...}` — file changes, subprocess results, etc. |

Every fact carries the stable `invocation_id`; authorization additionally binds
the prepared input digest and tool/target revisions. Audit reconstructs the
chain by that identity, not by a model-provided `call_id` or ambiguous pair.

## Tool result lifecycle

```
intake → judge → run → result
                    ↓
   ┌──────────────┬──────────────┐
   ↓              ↓              ↓
ok result      rejected       failed/timeout
   ↓              ↓              ↓
tool.result   tool.result.    tool.result
              rejected        {status:error|timeout}
```

- **OK result** → `tool.result` — visible on surface (via
  tier-1 preview, per `loop.md` "Per-tool Policy Profiles")
- **Rejected** → `tool.result.rejected` — reason fed back to
  the model next iteration (no termination; the model owns
  its output)
- **Execution failure** → typed `tool.result{status: error}` linked to the
  durable execution terminal; it is distinct from authorization rejection

All three paths become Runtime-owned durable facts. Model request/response
receipts record what the model saw and what it cost; the shared invocation
identity makes effect audit end-to-end.

## Execution semantics — determinism + speculative dispatch

The dispatcher gives a **deterministic order** to events
that cross the event / ledger boundary, even when the
underlying tools run in parallel.

### Event ordering rule

For a given turn, the dispatcher emits events in this
order:

1. **Per intent**: `toolStart → (toolDelta × N) → terminal`
   (one of `toolEnd`, `toolTimeout`, `toolCalled`,
   `toolRejected`).
2. **Across intents in a parallel group**: by `intent.call_id`
   ascending, **not** by completion time. If intent `c2`
   finishes before `c1`, the dispatcher waits for `c1` before
   publishing `c2.toolEnd`. The user sees ordered results.
3. **Across parallel groups**: ordered by the model's emit
   order in the original reply.

The **determinism** invariant: replaying a turn produces
the same event sequence, byte-for-byte, regardless of which
intent finishes first. (See `provider-adapter.md` "Outcome
kinds" for the 8 kinds `model.invoke` returns — the
determinism applies to the events downstream of those.)

### Speculative dispatch during streaming

The dispatcher can **start a tool before the model finishes
streaming its reply**. The protocol:

1. `model.invoke` streams tokens.
2. As soon as the dispatcher sees a **complete tool call
   block** in the partial stream — a structured `<tool_call>`
   with `name`, `args`, `call_id` fully emitted — it can
   submit the call to the work pool.
3. The model's reply keeps streaming in parallel; new
   `tool_call` blocks are dispatched the same way.
4. At `toolEnd`, the `tool.result` is appended to the
   ledger. The `model.usage` row records the `prefetch_lag`
   metric — how early the dispatcher started the call relative
   to the model's stream-end.

**Benefits**:
- The user sees **tool execution begin before the model has
  finished its prose** — better latency for long, expensive
  tools.
- The dispatcher hides network / disk latency behind the
  model's stream completion.

**Risks**:
- The model might **abort** the tool call mid-stream (e.g.
  the model changes its mind). Speculative dispatch handles
  this by **cancelling** the in-flight tool via cooperative
  cancel (`Provider-Adapter` "Cancellation semantics"). The
  partial result goes to `provider.partial` on the ledger
  for audit.
- The dispatcher's `prefetch_lag` budget is bounded: a tool
  may not be dispatched more than **N ms ahead** of the
  model's stream completion. `N` is configurable per tool.

**Status of priority**: speculative dispatch is a
**layered enhancement**. The base dispatcher works
correctly without it; speculative is on top. **Lower-priority
item — lands after the deterministic core ships.**

### Resource conflict detection

The dispatcher detects **resource conflicts** between
in-flight tools:

```python
class ToolConflict:
    tool_a:      str           # name
    tool_b:      str
    resource:    str           # e.g. "/workspace/foo.txt"
    conflict:    str           # "read-write" | "write-write" | "same-process"
    resolution:  str           # "serialise" | "reject-second" | "abort-both"
```

Tools declare their **resource claims** in the schema:

```python
class Tool:
    ...
    claims: list[ResourceClaim]    # what resources the tool touches
    conflicts_with: Callable[[ResourceClaim, ResourceClaim], bool]
```

The dispatcher runs `conflicts_with(claim_a, claim_b)` before
starting a tool. **Read-write / write-write on the same path
serialises**; **same-process via `bash`** schedules onto
different process slots; **network on the same host
rate-limits**.

Two `read_file`s on the same path → **parallel** (no conflict).
Two `write_file`s on the same path → **second is rejected**
with `governance.verdict{decision: deny, reason: "concurrent
write on path"}`. The model sees the rejection and picks
another approach.

### Tool discipline — few, well-designed, composable

> **编排组合的工具 should beat many, scattered tools.**

A small, well-modelled tool set is better than a sprawling
catalog. Adding a new tool is **expensive**:

- It needs an `effect_class` declaration (mandatory).
- It needs a `Workspace` capability declaration (mandatory).
- It needs a `claims` declaration for resource conflict
  detection.
- The contract test must cover it (E1–E5).
- The **kimi-style `ToolAccesses`** (read / write / all) is a
  weaker ancestor; we require the full declaration.

Composing tools (e.g. `find . -name '*.rs' | xargs grep`)
is **preferable** to having `grep_rust_files` as a separate
tool. The dispatcher already supports composition; the
`bash` tool's effect_class is `Mutating`, and the conflict
detector handles `bash`-vs-`bash` correctly.

The default **tool set** is small:

| Tool | Effect class | Capabilities | Default |
|------|--------------|-------------|---------|
| `read_file` | `ReadOnly` | filesystem | yes |
| `write_file` | `Idempotent` | filesystem | yes |
| `edit_file` | `Idempotent` (overwrite semantics) | filesystem | yes |
| `bash` | `Mutating` | filesystem + process | yes |
| `grep` | `ReadOnly` | filesystem | yes |
| `glob` | `ReadOnly` | filesystem | yes |
| `web_fetch` | `ReadOnly` | network | optional |
| `mcp_*` | declared by MCP | declared by MCP | optional |

Adding a new tool is a config + a contract test; no
core-runtime change.

### Workspace — git-worktree isolation (deferred)

For **git-repository workspaces**, the runtime uses
**git-worktree** to isolate tool execution:

```
session workspace
└── <repo>/
    ├── main worktree (default)
    └── .worktrees/
        └── <session-id>/
            └── (tool runs here, isolated from main)
```

A **default-isolated** tool runs in a per-session worktree
under `.worktrees/<session-id>/`. The worktree is created
on the first git-tool call and torn down at session end
(if the policy is "scratch worktree"). Persistent sessions
keep the worktree across turns.

This is **deferred priority**:

- The basic workspace (filesystem rooted at
  `/workspace/<session>/`) is in scope now.
- The git-worktree-aware variant is a **later layer** — the
  architecture is ready (the dispatcher hands a
  `Workspace` object to each tool), but the implementation
  lands with the slice that uses git repos.
- **Overlay filesystems** (e.g. Copilot / Jujutsu-style)
  are an even later optimisation — they trade disk for
  speed. We don't depend on them; if they land later, they
  sit behind the same `Workspace` interface.

## BDI architecture — Belief / Desire / Intention

The agent's runtime matches the **BDI (Belief-Desire-Intention)
architecture** — a classical agent theory (Bratman 1987) — at
the **effect layer**. Naming the pipeline after BDI makes it
legible to anyone who knows the theory and makes failures
categorisable.

```
Belief     = the ledger (everything the agent knows)
Desire     = `goal.*` (current + subtree of future goals)
Intention   = `intent` in `model.usage.reply.items` (the model's next-action proposal)
Filter     = `governance.judge(intent)` → verdict
Action     = tool call (after dispatch, after filter)
```

The effect-layer pipeline is **literally BDI's intention
filtering**:

1. **Intention formation** — model emits `intent`s in its
   reply. These are *what the model wants to do*.
2. **Intention filtering** — `governance.judge(intent)` →
   verdict (Approve / Deny / ApproveWithRewrite / AskUser).
   This is BDI's "filter" step: not every intention is
   executable.
3. **Action execution** —
   `ToolDispatcher.dispatch(approved_intents)`. The filtered
   intentions become tool calls.

| BDI term | Garive term | Lives in |
|----------|-------------|----------|
| **Belief** | ledger (append-only) | `ledger.md` (whole stack) |
| **Desire** | `goal.declare / update / close` | `ledger.md` "goal.* family" |
| **Intention** | `intent` (model reply) | `model.usage.reply.items` |
| **Filter** | `governance.judge(intent)` | this doc "Stage 2" |
| **Action** | tool call (after dispatch) | this doc "Stage 3–4" |

### BDI failure modes

Naming the pipeline after BDI gives a **categorical failure
taxonomy** — each failure maps to a BDI term, and the
recovery is well-defined:

| Failure | BDI term | Recovery |
|---------|----------|-----------|
| Ledger lost / corrupted | **Belief** failure | Replay / reconstruct from backup |
| `goal.*` is stale / contradictory | **Desire** failure | Model re-derives the goal from current context |
| Model emits an intent not matching desire (e.g. edits a file not in goal) | **Intention** failure | `governance.judge` → Deny → feedback |
| `governance.judge` has a wrong policy | **Filter** failure | Edit the policy table; user can override |
| Tool itself fails | **Action** failure | Per the failure-semantics discipline (data, not exception) |

The **filter** step is the only BDI step that has explicit
runtime code; the others are emergent from the data
structures (ledger / `goal.*` / `intent`). Naming makes the
**policy** the only thing that has to be coded; the rest is
typed.

### Speculative dispatch — the cost is real

Speculative dispatch (above) has **a real cost** that must
be acknowledged:

- The model can **abort** the tool call mid-stream. The
  dispatcher must cooperatively cancel the in-flight tool
  and write `provider.partial` to the ledger. **Some work
  the tool did is wasted** — a write_file that was rolled
  back, a network call that was made. The user's "save
  latency" is paid in "tool does work that gets undone".
- A **wrong dispatch** is also wasted — the model emits an
  intent that the dispatcher starts, then changes its mind
  before the model finishes streaming. Same cost.
- **For `Mutating` tools** (per the effect-class discipline)
  the cost is higher — a partially-written file may need
  cleanup.

**Why it's worth doing anyway** (deferred, not now):

- The latency win is **measurable** — first-token-to-first-
  tool-execution drops from `(model-stream-time + tool-
  start-latency)` to `tool-start-latency` alone.
- The cleanup cost is bounded — partial results in
  `provider.partial` + the `Mutating` recovery rule (no auto
  re-run) keeps the blast radius small.
- The **alternative** (wait for model stream to finish before
  dispatching) is the conservative default — speculative
  is the **layered enhancement** that lands later.

**Layering**:
- Base dispatcher works correctly without speculative —
  the deterministic ordering rule still holds.
- Speculative is added as a **layer on top** that respects
  the ordering rule and adds a `prefetch_lag` bound.
- The recovery semantics (cooperative cancel + `provider.partial`)
  are already designed for both modes.

**The cost is the justification for deferral** — but the
cost is also bounded, so deferral is a priority choice, not a
correctness concern.

## Deterministic simulation — exhaustive interleavings

The E1–E5 contract tests verify **specific scenarios**
(static). The dispatcher needs **stronger** guarantees:
**byte-determinism across all possible scheduling
interleavings**, given the same input.

The deterministic simulation is `dispatcher_contract_tests`!
test suite — distinct from E1–E5:

| Property | Test |
|----------|------|
| **Replay determinism** | Run the same batch 1000× with random delays (each tool 0–50 ms); the outcome (events emitted + ledger rows written) is **byte-equal** every time. Seed is fixed; same seed → same outcome. |
| **Exhaustive scheduling** | For a parallel group of N tools, enumerate **all N! orderings**. Each ordering must produce the same final state. |
| **Race-condition injection** | Random delay on each tool (0–50 ms); assert the outcome is deterministic regardless of timing. |
| **State-machine coverage** | Every state in the `ToolDispatcher` (READY / INVOKING / BACKOFF / RESPONDING / DONE / TRUNCATED / ERROR_*) is visited at least once. |
| **Pairing invariant** | Every `toolStart` has exactly one terminal (`toolEnd` / `toolTimeout` / `toolCalled` / `toolRejected`); orphans are bugs. |

The **two suites together**:

- **E1–E5 (property + fixed)** — "the dispatcher behaves
  correctly on the canonical scenarios."
- **Deterministic simulation** — "the dispatcher is
  byte-deterministic across all interleavings, given the same
  input."

Both must pass. The deterministic simulation catches what
E1–E5 cannot:

- Race conditions (concurrent access to shared state).
- Ordering ambiguity (events emitted in the wrong order).
- Partial state (interrupted runs leave dangling rows).
- Cross-pool races (GatedBatch + Passthrough + Sequential
  dispatcher running concurrently).

The deterministic simulation is **CI-gated** — a
non-determinism regression fails the build. Failures are
**P0** because they violate the load-bearing contract that
the dispatcher is byte-deterministic.

### Pairing invariant (concrete)

Every `toolStart` event **must** be paired with exactly one
terminal. The contract test asserts this:

```python
def test_pairing_invariant(intents, simulated_delays):
    events = run_dispatcher(intents, delays=simulated_delays)

    starts = [e for e in events if e.kind == 'toolStart']
    terminals = [
        e for e in events
        if e.kind in ('toolEnd', 'toolTimeout', 'toolCalled', 'toolRejected')
    ]

    assert len(starts) == len(intents)
    assert len(terminals) == len(intents)
    assert all(s.call_id == t.call_id
               for s, t in zip(starts, terminals))
    assert all(start.seq < terminal.seq  # ordering rule
               for start, terminal in zip(starts, terminals))
```

A failure is a **P0**: an orphan start means the dispatcher
lost track of a tool. The recovery is to find which
scheduler path missed the terminal and add a coverage test.

## Cross-references

- `loop.md` "Stage ④ — invoke / judge / run" — the loop's
  effect-layer outline (this doc is the detail).
- `loop.md` "Two protocols" — the event/ledger split
  between live bytes (event stream) and durable bytes
  (`tool.result` in ledger).
- `provider-adapter.md` "Boundary — effect-layer vs
  provider-adapter" — what's recovered where.
- `ledger.md` "tool.* family" — the entry kinds this layer
  produces.
- `compression.md` "Derive in Detail" — tool.results are
  surface-visible; large results trigger tier-1 (preview
  + seq pointer) per the policy.
- `.agents/testing.md` "Tool / Integration" — integration
  tests exercise the dispatcher's batch / concurrency
  policies end-to-end.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: **draft (possible mechanism)** — the 4-stage
  shape, the failure-semantics discipline, the L2 dispatcher
  family, the tool registry with `effect_class`, and the E1–E5
  contract tests are settled. Specific tool implementations,
  per-tool timeouts, and workspace capability flags land with
  the slice. No final code.
