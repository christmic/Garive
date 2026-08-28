# Effect Layer — tool execution

> After `model.invoke` returns, the loop hands the model's
> intents to the effect layer: **governance**, **dispatch**,
> **execute**. This doc covers everything below
> `governance.judge` — the `ToolDispatcher` and the tool
> implementations themselves.
>
> `loop.md` describes the effect-layer outline as one stage of
> the round; this doc is the **detail**. The boundary between
> provider-adapter (transport) and effect-layer (semantic
> output) is load-bearing — see the Boundary section below.

## TL;DR

```
model.usage.reply.intents
       ↓
Stage 1 — extract intents
       ↓
Stage 2 — governance.judge  →  verdict
       ↓
Stage 3 — ToolDispatcher  (batch / concurrency / streaming / timeout)
       ↓
Stage 4 — tool implementation  →  workspace
       ↓
tool.result / tool.result.rejected → ledger
```

The four stages are **a single round's effect chain**;
each stage writes its own ledger row so the chain is fully
auditable.

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

## Stage 1 — extract intents

The model's reply carries one or more `intent`s in
`reply.items`. An intent is a 4-tuple:

```python
class Intent:
    name:     str            # tool name, e.g. "read_file"
    args:     dict          # tool-specific arguments
    call_id:  str           # model-side identifier
    parallel: bool = False  # explicit parallel flag (rare)
```

Multiple intents in one reply → **batch dispatch** by the
ToolDispatcher. The batch policy decides whether to run them
sequentially or in parallel — see Stage 3.

## Stage 2 — governance.judge

Each intent goes through `governance.judge(intent)` →
verdict (Approve / Deny / ApproveWithRewrite / AskUser). The
verdict is itself a `governance.verdict` ledger row.

| Verdict | What the dispatcher does |
|---------|---------------------------|
| `Approve` | Dispatch as-is |
| `Deny(reason)` | Write `tool.result.rejected{reason}`; feed reason back to model next iteration (no termination) |
| `ApproveWithRewrite(x)` | Dispatch with rewritten args |
| `AskUser(question)` | Write `approval_request`; round returns `Suspended` (per `loop.md` "Stage ④") |

The verdict is **the only place** that consumes the
workspace capability declaration. A tool that declares
`network: false` cannot be coerced into a network call by
the model — `governance.judge` rejects it on principle.

## Stage 3 — ToolDispatcher

The dispatcher owns the **policy** of how tools run:

### Batch policy

A single model reply can carry N intents. The dispatcher
decides whether to run them sequentially or in parallel:

| Default | All intents in a single turn run **sequentially** (deterministic) |
| **Parallel** | Explicit `intent.parallel: true` flag on the intent |
| **Mixed** | Read-only tools run in parallel; write tools run sequentially |

The model's claim of "parallel" is **advisory** —
`governance.judge` may rewrite it to sequential if the
policy says so.

### Concurrency policy

Per-tool **concurrency limit** (e.g. `bash: 2`,
`read_file: 16`, `web_fetch: 4`). AIMD per-tool:

```
on tool.success:       tool.concurrency += 1      # additive
on tool.timeout:       tool.concurrency /= 2      # multiplicative
on tool.policy_violation: tool.concurrency -= 1    # permanent cut
clamp(tool.concurrency, 1, tool.max_concurrency)
```

The default is per-tool because the failure modes differ:

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
- The model gets `tool.result.rejected{reason: "timeout"}` to retry
- The runtime's tool.concurrency is **multiplicatively** reduced (AIMD)

## Stage 4 — tool implementation + workspace

Every tool implements:

```python
class Tool:
    name:           str
    description:    str
    schema:         dict               # JSON Schema for args
    requires:       Capabilities       # filesystem | process | network
    timeout_ms:     int
    concurrency:    int                # AIMD-adjusted
    max_concurrency: int                # clamp upper

    async def run(self, args: dict, ws: Workspace) -> ToolResult:
        ...
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

Workspace = the boundary between **tool power** and
**governance policy**. `governance.judge` checks intent
*against* the workspace capability declaration. A tool
that declares `network: false` cannot make a network call,
**full stop** — even if the model tries.

| Tool class | `requires` | Workspace behavior |
|------------|------------|---------------------|
| `read_file` | `filesystem` | Path is normalised and **rooted at `/workspace/<session>/`**. Path traversal blocked. |
| `bash` | `filesystem + process` | Args are **arrays** (not shell strings) → no shell injection. Env is an allow-list. Network disabled by default. |
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

Each row has **`pair_ref`** linking the judge → the result,
and the verdict → the result.rejected. An audit query joins
on `pair_ref` to reconstruct the **full effect chain** for a
given turn.

## Tool result lifecycle

```
intake → judge → run → result
                    ↓
   ┌──────────────┬──────────────┐
   ↓              ↓              ↓
ok result    rejected      exception
   ↓              ↓              ↓
tool.result   tool.result.   tool.result.
              rejected        rejected{reason:
(cached in    (feedback       "exception:
 surface      to model)        ..."}
 via tier-1)   
```

- **OK result** → `tool.result` — visible on surface (via
  tier-1 preview, per `loop.md` "Per-tool Policy Profiles")
- **Rejected** → `tool.result.rejected` — reason fed back to
  the model next iteration (no termination; the model owns
  its output)
- **Exception** (caught at the workspace boundary) →
  `tool.result.rejected{reason: "exception: ..."}` — same
  path as rejected

All three paths land in the ledger; the **model.usage**
records the cost; the **loop.receipt** records what was in
the surface. Audit is end-to-end.

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
- Last reviewed: 2026-08-27
- Status: **draft (possible mechanism)** — the 4-stage
  shape, the failure-semantics discipline, the L2 dispatcher
  family, the tool registry with `effect_class`, and the E1–E5
  contract tests are settled. Specific tool implementations,
  per-tool timeouts, and workspace capability flags land with
  the slice. No final code.