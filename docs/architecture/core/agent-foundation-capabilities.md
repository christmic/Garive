# Agent foundation capabilities

> Accepted architecture for the capability ring around the Agent kernel:
> sandbox enforcement, safety governance, durable Goals, executable Plans and
> the first built-in tools. Normative field and transition details live under
> `spec/design/`.

## Problem

The Core loop, Ledger, governed-effect chain and recovery protocol already
exist. They deliberately do not answer five product questions:

1. which concrete operating-system boundary enforces an authorized effect;
2. which safety policy admitted it and what happens when proof is incomplete;
3. how work that spans several Turns is named and judged complete;
4. how a proposed sequence becomes a durable, revisable execution plan; and
5. which small, predictable tools every local Agent can actually use.

These are one capability ring, not five alternate runtimes.

## Layer map

```text
Client Goal/Plan views and interactions
                  |
                  v
Runtime durable coordinator
  - authenticated Goal commands and revisions
  - Plan adoption, step claims and recovery
  - safety policy and authorization decisions
  - sandbox binding and concrete tool executors
  - atomic Ledger facts and redacted projections
                  |
                  v
Core bounded execution
  - derives the frozen Goal/Plan context
  - proposes plan changes and tool intents
  - reduces governed observations
                  |
       +----------+----------+
       v                     v
Engine Goal/Plan values   Engine Tool contracts
and pure reducers         and exact resource access
```

Dependencies continue to point from Runtime to Engine. A portable Engine
module may describe a requirement, state transition or proposal. It cannot
open a path, spawn a process, inspect credentials, authorize itself or claim
that an operating-system control was enforced.

## Ubiquitous language

| Term | Meaning | Owner |
|---|---|---|
| Goal | Revisioned durable desired outcome with scope, success criteria and bounds. | Engine semantics; Runtime identity and persistence |
| Goal attempt | One bounded period of active work against one Goal revision. | Runtime |
| Plan revision | Immutable DAG of bounded steps for one exact Goal revision. | Engine semantics; Runtime adoption |
| Plan step | A declared unit of work; never authority to perform its effects. | Engine Plan |
| Step claim | Runtime lease allowing one worker to advance one ready step. | Runtime |
| Effect batch plan | C5b scheduling of prepared calls from one model reply. It is not a Goal Plan. | Tools/Runtime |
| Sandbox requirement | Portable declaration of controls an executor must prove. | Tools/config |
| Sandbox binding | Runtime-selected executor, workspace and policy revisions that promise those controls. | Runtime |
| Safety decision | Fail-closed policy result bound to exact subject and revisions. | Runtime policy port |
| Built-in tool | Versioned neutral Tool Definition plus an independently selected Runtime executor. | Tools + Runtime |

A Turn is one durable user/system objective sent to one Agent Instance. A Goal
may be created by a Turn, outlive it, receive later Turns and own multiple Plan
revisions. Neither a Goal nor a Plan is an invocation, grant or execution.

## Authority chain

```text
untrusted model proposal
  -> schema-valid Prepared Call
  -> exact resource resolution
  -> Goal/Plan scope check
  -> safety policy decision
  -> invocation grant
  -> sandbox preflight proof
  -> Started fact
  -> executor receipt
  -> durable result and projection
```

Every arrow narrows or preserves authority. A Goal, adopted Plan, tool
definition, resource declaration, safety label or sandbox profile grants
nothing by itself. The existing C5 invocation grant remains the sole authority
for one effect, and it binds the exact Prepared Call digest.

## Sandbox and safety split

Safety decides whether an exact operation is allowed under authenticated
product policy. Sandbox enforcement proves that the selected executor can
contain that allowed operation. Combining them would let policy declarations
masquerade as operating-system isolation.

Runtime performs four checks:

1. **catalogue admission** rejects definitions whose declared access cannot be
   resolved exactly;
2. **authorization** evaluates actor, Goal/Plan scope, resource set and limits;
3. **executor preflight** proves the selected sandbox implements every granted
   control before `effect.started`;
4. **terminal verification** validates receipt bindings, output bounds and
   redaction before publishing an observation.

Unsupported enforcement is a typed pre-start result. Lost proof after start is
uncertain effect state and follows C5 recovery; it is never rewritten as a
safe failure.

## Goal lifecycle

```text
Draft -> Active -> Succeeded
          |  |----> Failed
          |  |----> Cancelled
          `-------> Suspended -> Active
```

Each transition uses optimistic revision matching and a stable command
identity. Success requires evidence satisfying the frozen success criteria;
model prose alone cannot close a Goal. Editing objective, criteria, scope or
bounds creates a new revision. Historical revisions and their evidence remain
addressable for audit.

Child Goals form an acyclic ownership graph with explicit budget and authority
narrowing. Delegation may create work for another Agent, but MA0 child-agent
lifecycle and Goal parentage remain distinct identities.

## Plan lifecycle

```text
Proposed -> Adopted -> Running -> Completed
              |          |------> Suspended
              |          |------> Failed
              `---------> Superseded
```

One adopted Plan revision binds one exact Goal revision, definition snapshot,
tool catalogue revision, policy revision and hard bounds. Steps form a finite
DAG. Readiness is a pure projection of adopted topology plus durable step
terminals. Runtime alone leases ready steps and commits results.

Replanning creates a new immutable revision. It never edits a running plan in
place, reuses a completed step without evidence, or discards a started
uncertain effect. Runtime may carry forward only terminal step evidence whose
declared input and dependency digests are equal.

## Built-in tool baseline

The first production set is intentionally small:

| Tool | Access | Replay | Output |
|---|---|---|---|
| `workspace.read_text` | one exact relative file read | read-only | bounded UTF-8 text |
| `workspace.list` | one exact relative directory read | read-only | bounded sorted entries |
| `workspace.search_text` | one exact rooted search | read-only | bounded ordered matches |
| `workspace.apply_patch` | exact declared file writes | receipt-recoverable | changed-file receipt |
| `process.run` | one admitted argv/process lane | never-replay unless executor proves otherwise | bounded exit/output record |

The catalogue contains no implicit shell, home-directory expansion,
environment lookup, ambient network or absolute workspace path. `process.run`
uses structured argv. A separately named shell tool would require its own
definition, authority and sandbox evidence.

Read-only tools land before mutation and process tools. This is delivery order,
not permission inheritance: installing all five still grants none of them to
an Agent Definition.

## Durable truth

Goal, Plan and safety facts enter the same Runtime Ledger transaction model as
Turns and effects. Public projections are derived, redacted and restart-safe.
At minimum the audit chain preserves:

- command identity, authenticated actor reference and expected revision;
- exact Goal/Plan/snapshot/catalogue/policy digests;
- safety decision and sandbox binding revisions;
- step claim, invocation, grant, receipt and result bindings;
- suspension or uncertainty reason without secret policy internals.

Ephemeral progress may be dropped. Goal terminals, adopted/superseded Plans,
step terminals, safety decisions and sandbox receipts may not.

## Cross-language boundary

Rust owns the production Runtime and concrete sandbox. Kotlin independently
implements portable Goal/Plan validation, canonical digests and pure
transitions from shared fixtures. Kotlin does not claim operating-system
sandbox, production worker or storage parity.

## Delivery order

1. freeze Sandbox/Safety portable values, decisions and fixtures;
2. implement Runtime sandbox preflight and read-only built-in executors;
3. freeze Goal lifecycle, commands, facts and projection;
4. freeze Plan validation, revision and step reduction;
5. add write/process tools with journals, fault injection and recovery;
6. integrate Goal/Plan context, Host projections and clients.

Each increment must keep existing C4/C5/C5b/C6 behavior green and land in a
small reversible batch.

## Rejected shortcuts

- treating a prompt checklist or Markdown plan as executable authority;
- storing Goal/Plan truth only in client state or Memory;
- putting filesystem paths, subprocesses or sandbox technology in Core;
- interpreting a tool's `ReadOnly` declaration as enforcement proof;
- using process environment as hidden configuration or tool input;
- replaying a started write/process because a result fact is absent;
- creating a second scheduler or ledger for Plans.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
