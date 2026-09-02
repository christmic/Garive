# Headless Multi-Agent collaboration — real API evidence

Date: 2026-09-03  
Runtime: compiled `garive-headless` on `127.0.0.1:18790`  
Provider profile: `anthropic.messages.v1`  
Model target: `deepseek-v4-flash` through the configured loopback deployment

## Claim boundary

This acceptance used the real H1 HTTP surface, SQLite ledger, Runtime worker,
provider transport, and configured model. It proves the currently admitted
MA1 `Notify` slice: ten equal named peers in one Session, durable directed and
broadcast messages, recipient-side model consumption, and real named,
anonymous, and fork-self assignee executions.

It does not claim the unfinished `AwaitBeforeFinal`, `SuspendExecution`,
restart-safe delivery supervisor, authority/budget/fanout admission, or
Agent-callable collaboration tool catalogue. Peer messages in the ten-Agent
run were submitted through H1 with explicit roster identities; delegation
results were emitted by Runtime itself after observing the assignee terminal.

No API credential is included in this record.

## One Session, ten equal named peers

Session:

```text
session-5f4b6f3a8f5e68a9ede8188da0132d0c3f5e2d31961d0a431886a5851d809f4c
```

`GET /v1/sessions/{session}/agents` returned exactly ten distinct members:

```text
Atlas (founding), Birch, Cinder, Delta, Ember,
Fjord, Grove, Harbor, Iris, Juniper
```

All ten carried different `agent_instance_id` values. Only the first member
had `founding_member=true`; that marker grants no control role.

## Directed and broadcast communication

Atlas wrote a durable directed message to Birch at position 11:

```text
TEAM_SEED=CRAB-10. Acknowledge this exact seed.
```

Birch's real Turn consumed that message from Runtime context and completed at
position 20 with:

```text
BIRCH_ACK=CRAB-10
```

Birch then broadcast the seed at position 21. Eight other peer Turns consumed
the broadcast and completed with their own exact acknowledgement:

```text
Cinder_ACK=CRAB-10
Delta_ACK=CRAB-10
Ember_ACK=CRAB-10
Fjord_ACK=CRAB-10
Grove_ACK=CRAB-10
Harbor_ACK=CRAB-10
Iris_ACK=CRAB-10
Juniper_ACK=CRAB-10
```

The nine peers' result messages were addressed back to Atlas at positions
271–279. Atlas's real synthesis Turn consumed them and completed at position
297 with:

```text
ATLAS_SUMMARY=9_PEERS;SEED=CRAB-10
```

Some initial peer attempts failed because the model selected a Workspace tool
despite the literal text-only task; successful retry Turns remain distinct in
the ledger. No failed attempt was rewritten as success.

## Real Notify delegation targets

The corrected dispatch transaction durably orders
`collaboration.assignee_started` before the assignee `turn.started`,
`turn.input`, and terminal `execution.started`. This lets recovery bind both
roster members and task-scoped anonymous/fork instances before execution.

Three real selector types completed:

| Selector | Assignee | Terminal result |
|---|---|---|
| `named` | Birch peer `agent-8477…304d8` | `TEAM_SEED=CRAB-10` plus `NAMED_PEER_DELEGATION_OK_2` |
| `anonymous` | task-scoped `agent-anonymous-8a30…8ae2` | `ANONYMOUS_DELEGATION_OK_2` |
| `fork_self` | task-scoped `agent-fork-2240…0315` | `FORK_DELEGATION_OK` |

For the anonymous and fork cases, Runtime—not the API caller—observed
`turn.completed`, committed `collaboration.assignee_terminal` and
`collaboration.result_delivered`, then appended an addressed
`session.agent_message` back to the dispatcher.

One first anonymous provider attempt stopped after durable `model.started` and
produced no terminal. A fresh idempotency key produced the successful run
above. This is retained as failure evidence and exposes the still-open need for
terminalizing provider failures and restart recovery.

## Dispatcher does not suspend

A clean two-peer Session used AtlasClean as dispatcher and BirchClean as named
assignee:

```text
Session     session-1723c2f0117c3f19763fe605e0b85651cdafd74ffdef23be2a86fe641407f6a7
Delegation delegation-43c1038f8e0ab418461eb48e971a77e7b623f5cacc8cfd9b577cb90ec34bff74
Child Turn turn-dd15704805c13a99ed0834155f948af75e1ed51cbd2e355bfdc4f86113021b7c
Parent Turn turn-9883480b702efad17b5243ae38ea722d82e94545ec03728a7b0aa62faf78e564
```

Immediately after the delegation response, H1 accepted a new AtlasClean Turn.
It completed with `CLEAN_PARENT_CONTINUED_OK`; no parent `turn.suspended` fact
was emitted. The first child provider attempt did not terminalize. A second
named delegation in the same clean Session completed with
`CLEAN_CHILD_RETRY_OK`, and Runtime delivered it to AtlasClean at position 33.

This proves `Notify` dispatch does not gate dispatcher progress. It does not
prove simultaneous model transport: the current headless worker consumes its
queue serially.

## Automated regression gates

`runtime/replica/tests/live_host.rs` now proves:

- ten unique named members are admitted and an eleventh is rejected;
- every one of the ten can send a directed message to another peer;
- one peer can broadcast, while foreign senders and recipients fail closed;
- two peer Turns can be open independently;
- named, anonymous, and fork-self Notify requests allocate real Turn and
  Execution coordinates;
- identical dispatch replay returns the original coordinates without a second
  assignee, while changed content conflicts;
- the assignee binding precedes `execution.started`, which is the dispatched
  committed boundary;
- unimplemented `await_before_final` fails closed instead of behaving like
  Notify.

Portable Rust and Kotlin MA1 domain tests additionally enforce the ten-member
bound, unique names/identities, selector validation, and delivery-policy
definitions.
