# Effect Layer — tool execution

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
