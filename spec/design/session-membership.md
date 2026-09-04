# S0 — Session membership metadata

> Public Runtime contract for maintaining the set of registered Agent
> identities associated with a durable Session. Membership is metadata;
> execution admission is evaluated when a Turn starts.

## Audience

Engineers implementing the Runtime Host, durable Session projection, and
clients that manage Session participants.

## Why

A Session may contain multiple equal named Agents. Adding an Agent to a
Session must not instantiate it, load its directory, freeze an execution
snapshot, or require it to be active. Those checks belong to the Turn that
attempts to execute the Agent.

The Session roster is an allowlist for Turn delivery. It is not an execution
catalogue and does not define a primary Agent or a parent-child hierarchy.

## Reference

### Membership value

One materialized member contains only:

```text
SessionMember {
  agent_id
  joined_position
}
```

`agent_id` uses the Agent registry identity grammar. Runtime-internal
execution identities and snapshots do not belong to the public membership
value.

### HTTP API

Every mutation requires `Idempotency-Key`. Bodies reject unknown fields.

| Method and path | Request | Result |
|---|---|---|
| `GET /v1/sessions/{session_id}/agents` | none | current members in join order |
| `POST /v1/sessions/{session_id}/agents` | `{"agent_id":"reviewer"}` | add metadata |
| `POST /v1/sessions/{session_id}/agents/{agent_id}/remove` | empty JSON object | remove metadata |

Add and remove validate Session existence, Session lifecycle, Agent identity
syntax, the configured roster bound, and command idempotency. They do not
read the Agent registry or Agent directories and do not resolve an installed
execution definition.

Adding an existing member or removing a missing member returns
`precondition_failed`. Exact command replay returns the original resulting
roster. Reusing a command identity with different membership semantics returns
`command_conflict`.

Removal is allowed while the Agent has running or recoverable work. It changes
admission for later Turns and does not cancel, redirect, or invalidate work
that was already admitted.

### Turn delivery

Every new user Turn declares one closed delivery selector:

```text
direct(agent_id)
broadcast
```

`direct` requires the target `agent_id` to be a current Session member.
`broadcast` resolves the current complete member set at Turn admission. An
absent selector, an empty direct identity, or mixed direct/broadcast fields is
invalid; Runtime never infers a target from roster size or join order.

For each resolved recipient, Turn admission then requires the registered Agent
to exist, be active, and pass current directory/resource validation. A removed,
missing, inactive, archived, or invalid Agent fails before its execution
starts. The later Turn contract will define broadcast atomicity and its
multi-recipient response shape before broadcast execution is implemented.

### Durable projection

Membership changes use `session.agent_joined` and `session.agent_left` facts.
Each fact binds the command identity and `agent_id`. The current roster is the
ordered reduction of those facts. Rejoining after removal appends a new join
position; it does not restore the old position.

Legacy membership facts may contain display and execution fields. Runtime may
decode them for existing databases, but new metadata commands do not write
those fields and public responses do not expose them.

## Acceptance

1. An inactive, archived, missing, or currently invalid Agent identity can be
   added to and removed from Session metadata.
2. A direct Turn for that identity fails until it is a current member and an
   active valid registered Agent.
3. Removing a member does not cancel already admitted work.
4. Exact retries survive Runtime restart; semantic command reuse conflicts.
5. Roster reads reconstruct joins, removals, and rejoins in stable order.
6. New membership responses contain no definition revision, snapshot digest,
   display alias, directory content, or execution authority.

## See also

- [Agent registry](agent-registry.md) defines Agent identity and activation.
- [Multi-Agent collaboration](multi-agent-collaboration.md) defines autonomous
  peer messaging and delegation after Turn admission.
- [Host API v1](host-api-v1.md) defines common command and error behavior.

## Meta

- Owner: Garive Runtime
- Last reviewed: 2026-09-04
- Status: accepted
