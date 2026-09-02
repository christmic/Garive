# MA1 — Session collaboration and task delegation

## Status

Accepted successor to the topology assumptions in MA0. MA0 intent, authority,
budget and result bindings remain reusable primitives; this document owns who
may collaborate, what “parent/child” means, and whether a dispatcher waits.

## One Session, equal named collaborators

A durable Session admits a roster of zero to ten **named Agent Instances**.
Named members are peers: roster order, creation order and UI placement grant no
authority and establish no permanent parent, coordinator or leader. Each member
has a unique stable `AgentInstanceId`, unique non-empty display name within the
Session, exact Agent Definition revision and its own capability grant.

All members participate in the same conversation and address the same Session
ledger. Facts and messages always carry an author/actor Agent Instance ID so the
UI, context projection, audit and recovery never infer authorship from text.
Context visibility is policy-controlled; “same Session” does not implicitly
share credentials, private memory namespaces or tool authority.

Ten is a capacity bound, not a required team shape. A ten-member acceptance run
must prove ten distinct named identities collaborating in one Session; ten
ordinary Sessions or ten renamed model calls do not satisfy it.

## Delegation is an edge, not a hierarchy

“Parent” and “child” describe one `DelegationId` edge only:

```text
dispatcher Agent Instance --delegation--> assignee Agent Instance / fork
```

The relation ends when that delegation reaches a terminal result. It does not
change roster equality and does not prevent the assignee from independently
delegating other work within policy.

An assignee selector is exactly one of:

```text
Anonymous {
  definition_id, definition_revision
}
ForkSelf {
  source_agent_instance_id, through_position, branch_name?
}
Named {
  agent_instance_id
}
```

- `Anonymous` allocates a task-scoped Agent Instance that is not added to the
  named roster and is addressed by identity, never by a fabricated name.
- `ForkSelf` allocates a task-scoped Agent Instance with the dispatcher's exact
  definition/snapshot and a frozen lineage prefix for parallel exploration.
- `Named` addresses an existing member of the same Session roster. Runtime
  rejects missing, removed, cross-Session or revision-conflicting targets.

Anonymous/fork instances count against separate bounded active-task capacity;
they do not consume one of the ten named-member slots.

## Dispatch and join policy

Dispatch is non-blocking by default. The dispatching Agent's current Execution
may keep deriving context, call tools, send messages, complete, or dispatch more
work. Creating a child must never imply suspension.

Each delegation freezes one result-delivery policy:

```text
DeliveryPolicy =
  Notify             // append a durable result message; dispatcher continues
  AwaitBeforeFinal   // dispatcher may work, but its Turn cannot commit final
  SuspendExecution   // explicitly end this Execution and resume with result
```

`Notify` is the normal collaboration mode. `AwaitBeforeFinal` is a join/barrier,
not an immediate suspension. `SuspendExecution` preserves the existing MA0
continuation behavior for cases where no useful parent work can proceed.

Results enter the shared Session as durable addressed messages. A result can be
observed by the dispatcher in a later iteration or Turn, and may be collected by
an explicit join over exact delegation IDs. Arrival order never changes the
declared result order. Duplicate equal delivery is idempotent; conflicting
delivery fails closed.

## Fork exploration

`ForkSelf` is a real Agent Instance, not merely a provider/model role. It reads
an immutable Session prefix and receives independent Turn/Execution identities,
budgets and ports. Several forks may explore concurrently within explicit fanout
and aggregate-budget bounds. Adoption/discard is a separate governed verdict;
discarded work remains auditable and is excluded from the normal model surface.

The existing lightweight `branch.*` ledger mechanism remains appropriate for a
short single-Agent alternative. `ForkSelf` is used when exploration must execute
independently or concurrently. Runtime records the escalation and lineage.

## Portable request

MA1 evolves the MA0 request without changing provider adapters:

```text
DelegationIntentV2 {
  delegation_id
  session_id
  dispatcher_agent_instance_id, dispatcher_turn_id, dispatcher_execution_id
  assignee: Anonymous | ForkSelf | Named
  delivery_policy: Notify | AwaitBeforeFinal | SuspendExecution
  objective, input_evidence, result_schema
  budget, cancellation_policy, through_position
}
```

Authority is evaluated for the dispatcher, assignee and requested capability
set. Budget reservation precedes allocation/dispatch. Concurrent delegations are
allowed only within frozen per-Execution, per-Agent and per-Session fanout and
aggregate limits; unknown usage consumes its reservation.

## Durable facts

The admitted lifecycle is:

```text
session.agent_joined | session.agent_left
collaboration.delegation_requested
collaboration.delegation_authorized | collaboration.delegation_denied
collaboration.assignee_started
collaboration.assignee_terminal
collaboration.result_delivered
collaboration.joined?
```

Roster mutation, delegation authorization, assignee start and terminal result
are durable before publication. Facts bind Session, actor, assignee selector,
resolved assignee identity, lineage for forks, delivery policy, exact snapshots,
budget and content digests. `SuspendExecution` additionally uses the existing
`turn.suspended` and `delegation_result` continuation facts; the other policies
must not emit a parent suspension.

## API and tool surface

H1 exposes roster read/mutation, addressed Session messages, delegation query,
result collection and join. The Agent capability catalogue exposes neutral
`delegate`, `message_agent`, `collect_delegations` and `fork_self` tools. Tools
carry no provider/vendor semantics and all configuration is constructor-supplied.

An API client may initiate the same governed command for testing/operations, but
it cannot submit a fabricated child result. Runtime owns allocation, dispatch,
terminal binding and delivery.

## Acceptance

Real-provider acceptance must record:

1. one Session with ten distinct named peer Agent Instances;
2. addressed collaboration in which multiple peers contribute and consume each
   other's durable messages;
3. `Named`, `Anonymous` and `ForkSelf` delegation targets;
4. a `Notify` dispatcher continuing while the assignee runs;
5. an `AwaitBeforeFinal` join and explicit `SuspendExecution` continuation;
6. concurrent fork results arriving out of order but collected deterministically;
7. restart/replay without duplicate assignees, results or messages;
8. authority, roster-size, cross-Session target, fanout and budget rejection.

Rust and Kotlin share canonical V2 intent/result fixtures and reducers. Runtime,
worker, H1 and SQLite recovery are Rust composition responsibilities; Kotlin
does not claim those layers from portable fixture parity.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-03
- Status: accepted
