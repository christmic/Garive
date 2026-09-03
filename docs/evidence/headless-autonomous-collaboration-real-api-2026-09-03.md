# Headless autonomous collaboration — real-provider evidence

Date: 2026-09-03  
Runtime: compiled `garive-headless` on loopback  
Provider profile: `anthropic.messages.v1`  
Model target: configured `deepseek-v4-flash` deployment

## Claim boundary

H1 was used only to create Sessions, join named Agents, and submit user Turns.
The API caller never posted `/agent-messages` or `/delegations` in the
autonomous runs. Each collaboration action originated as a real model Tool
intent inside the Agent loop. Runtime derived the actor from `turn.started`,
applied Prepared-v3/F0 governance, committed the effect receipt, and published
the command from the durable outbox.

No credential or provider endpoint is included in this record.

## Real-provider acceptance matrix

| Scenario | Durable evidence | Result |
|---|---|---|
| addressed peer message | model chose `message_agent`; F0 actor matched active Agent; one `session.agent_message` | pass |
| named delegation | parent completed before publication; Birch child completed; result delivered to Atlas | pass |
| anonymous delegation | Runtime-created `agent-anonymous-*` completed and delivered | pass |
| self fork | distinct `agent-fork-*` and child Turn completed and delivered | pass |
| collect results | tool observation read all three delivered results before final synthesis | pass |
| ten equal named Agents | ten model Tool intents produced a complete addressed ring in one Session | pass |
| crash after `model.started` | restart classified `model.uncertain(runtime_lost)` and suspended without unsafe replay | pass, safe convergence |

### Named, anonymous, fork and collect

Primary Session:

```text
session-fd12eb26cc9621ac46a3bd51a9d0c183941982b42ec3cf2c552d98c75025bb5c
```

The named parent returned `PARENT_NAMED_120_OK` before
`collaboration.delegation_requested`; the child returned
`CHILD_NAMED_120_OK`. The anonymous and fork parents likewise returned
`PARENT_ANONYMOUS_120_OK` and `PARENT_FORK_120_OK` without waiting, followed by
delivered `CHILD_ANONYMOUS_120_OK` and `CHILD_FORK_120_OK` results.

The later `collect_delegations(max_results=10)` observation contained exactly
those three delegation records with `state = delivered`. Only after observing
that tool result did the model return `COLLECT_THREE_120_OK`.

### Ten-Agent autonomous ring

Session:

```text
session-c56ca0725ca8760f81963ad1ac853c450ff0ee5ef4110a121a8fe7416ed7cd56
```

The roster contained Atlas10, Birch10, Cinder10, Delta10, Ember10, Fjord10,
Grove10, Harbor10, Iris10, and Juniper10 as equal named members. API calls only
started their Turns. The model-selected messages formed this ring:

```text
Atlas10 -> Birch10 -> Cinder10 -> Delta10 -> Ember10 -> Fjord10
        -> Grove10 -> Harbor10 -> Iris10 -> Juniper10 -> Atlas10
```

An independent SQLite audit counted 10 matching `model.completed` Tool intents,
10 completed parent Turns, and 10 `session.agent_message` facts. Every message
command identity was `invocation-*`; zero ring messages had an API command
identity. Sender and recipient Agent Instance IDs matched the roster edge at
all ten positions.

## Failure found and fixed

The initial real delegation attempts stopped after 30 seconds with a body
decode timeout while the streamed provider response was still active. The
headless model request and Agent execution budgets are now the same explicit
120,000 ms constant. The regression test asserts that the HTTP request timeout
covers the Agent deadline. Named, anonymous, fork, collect, and the ten-Agent
ring all passed after this change.

Startup previously retained delegation delivery supervision only in process
memory. The Host now reconstructs safe pre-dispatch assignee work, defers any
request that crossed an external dispatch boundary, sweeps terminal undelivered
results, and runs the existing C6/F0 startup recovery before listening. A real
process kill after the child `model.started` proved the fail-closed branch:
restart committed `model.uncertain` and `turn.suspended` instead of replaying an
ambiguous provider request.

## Deterministic regression evidence

`runtime/replica/tests/autonomous_collaboration.rs` covers authenticated actor
binding, addressed publication, missing-target rejection, receipt-committed
outbox reconstruction, non-blocking named delegation, safe pre-dispatch child
redispatch, and terminal result recovery. `engine/multiagent/tests` covers the
exact four-tool Prepared-v3 schemas and rejects model-supplied actor forgery.

Still outside this acceptance claim: AwaitBeforeFinal, explicit
SuspendExecution composition, authority/budget/fanout admission, and restoration
of an unavailable historical immutable Agent snapshot.
