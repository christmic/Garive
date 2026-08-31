# H1 — Live durable Host API v1

## Status

Accepted implementation contract. H0 freezes the cross-platform wire values;
H1 composes those values with C6 Runtime commands, SQLite durability and a
loopback HTTP/SSE server.

## Scope and trust boundary

H1 is the first Rust product Host. It provides durable Session/Turn commands
and replayable events to CLI, TUI, Web, Desktop and mobile clients. It does not
own Agent decisions, provider policy, credentials, public Internet ingress,
tenant authentication or TLS termination.

The H1 server must bind an explicitly supplied loopback socket. Non-loopback
binding fails construction. Remote deployment requires the separately admitted
Gateway; H1 must not grow an implicit unauthenticated remote mode.

Every path, database location, installed Agent value, clock value, limit and
poll interval enters through construction. H1 reads no environment variables
or configuration files.

## Installed Agent binding

One immutable `InstalledAgent` supplied by Runtime composition contains:

```text
definition_id
definition_revision
snapshot_digest
agent_instance_namespace
effective Runtime/Core limits and model target policy
```

`POST /v1/sessions` accepts only an admitted `agent_definition_id`. Session
creation derives stable opaque `session_id` and `agent_instance_id` values from
the idempotency key plus the installed binding, then atomically commits one
`session.opened` fact. Its canonical payload binds the command ID, definition,
revision, snapshot digest and Agent instance. An HTTP success before this commit
is forbidden.

The Host reconstructs this binding from position one before accepting a Turn;
caller fields can never replace the installed revision, snapshot or instance.

## HTTP commands

| Method and path | Body | Required condition | Durable effect |
|---|---|---|---|
| `POST /v1/sessions` | `CreateSessionRequestV1` | known installed definition | `session.opened` |
| `POST /v1/sessions/{session_id}/turns` | `StartTurnRequestV1` | open owned Session | C6 start transaction |
| `POST /v1/turns/{turn_id}:cancel` | `CancelTurnRequestV1` | non-terminal owned Turn | `turn.cancel_requested` |
| `POST /v1/turns/{turn_id}:continue` | `ContinueTurnRequestV1` | exact current suspension and Session version | C6 continuation transaction |
| `GET /v1/sessions/{session_id}/events?after_position=N` | none | existing Session; `N` may be zero | replay then follow |

Every mutation requires exactly one `Idempotency-Key` header containing 1–128
visible ASCII characters. The key becomes the C6 Runtime command identity.
Identical replay returns the original identities and positions; reusing a key
with different semantics returns `command_conflict` and commits nothing.
The continuation `turn.started` fact binds `expected_session_version` so an
identical command remains distinguishable from reuse against another durable
version after restart.

Start and continuation responses are emitted after their transaction commits
and before later execution success is assumed. A configured `TurnDispatcher`
is notified only after commit. Dispatch rejection or process loss cannot roll
back the command; C6 recovery owns the resulting open Execution.

Cancellation is only a durable request. HTTP success does not claim that a
running model or effect has stopped.

### Typed continuation value amendment

The shipped Proto `ContinueTurnRequestV1.input = 4` remains a raw UTF-8 string.
The coordinated H2 wire slice additively assigns
`optional string input_json = 5`. Exactly one field is present for an
approval/external-input interaction:

- `input` is allowed only when the durable portable response schema admits a
  JSON string; Runtime canonical-JSON encodes that string before validation;
- `input_json` contains exact RFC 8785 JSON text for any admitted root value.

Runtime reconstructs the exact response schema/digest from the verified
suspension continuation, validates the normalized value before commit, and
stores those canonical JSON bytes in `interaction.resolved.response`. Unknown,
non-canonical, schema-invalid, dual, or absent interaction input is
`invalid_request` and commits nothing. Idempotency binds the selected field and
its exact normalized bytes; a retry cannot change representation.

Non-interaction suspension kinds do not gain authority from field 5.
Operator reconciliation remains outside the ordinary H1 continuation route
until a focused public authority contract is accepted.

## Successful responses

Session creation returns `CreateSessionResponseV1`. Turn mutations return
`TurnCommandResponseV1`. `committed_position` is the last position committed by
that command, never an in-memory publication sequence. Responses contain no
credential, adapter header, raw provider value or unredacted recovery evidence.

## Stable failures

Every non-success response is `HostErrorV1` with no implementation exception
text.

| HTTP | Code | Meaning |
|---|---|---|
| 400 | `invalid_request` | malformed path/body/header or invalid continuation value |
| 404 | `not_found` | Session or Turn is absent under the requested owner |
| 409 | `command_conflict` | idempotency identity was reused with different semantics |
| 409 | `concurrent_modification` | supplied/current Session version lost an optimistic race |
| 412 | `precondition_failed` | Session closed, Turn not suspended, or suspension mismatch |
| 503 | `durability_unavailable` | the durable store could not complete the operation |
| 500 | `corrupt_state` | persisted state failed integrity or schema validation |

Failures must not disclose whether an identity exists in a different Session.

## Durable event projection

The SSE endpoint replays a fixed SQLite prefix, then follows later committed
facts. Each record is:

```text
id: {durable Session position}
event: host
data: {HostEventV1 JSON}
```

`HostEventV1.position` is the source fact position. Positions strictly increase
but may have gaps because internal facts are not all client events. Reconnect
uses `after_position`; duplicate delivery is permitted, reordering is not.
Heartbeat comments carry no position and no semantics. Stream EOF is never a
Turn terminal.

Every H1 event carries the exact string `api_version = "v1"`. This value is
already frozen by the shared client fixture; package-qualified Proto names are
not runtime version values. An emitter/consumer mismatch is a protocol failure,
not a value clients may normalize.

The v1 projection is deliberately small:

| Durable fact | Host event | Public text |
|---|---|---|
| `session.opened` | `session.created` | absent |
| first `turn.started(kind=start)` | `turn.started` | absent |
| `turn.completed` | `turn.completed` | redacted display text derived from committed response items |
| `turn.suspended` | `turn.suspended` | absent |
| `turn.stopped` | `turn.stopped` | absent |
| `turn.failed` | `turn.failed` | absent |

Continuation `turn.started` facts do not create a second UI Turn. Unknown and
internal facts remain durable audit truth but are omitted from this public
projection.

H1 does **not** claim replayable token deltas. The retired fake-shell
`output.delta` events were never part of H1. A future live delta slice requires
an accepted persistence/backpressure/redaction contract; it cannot reuse
ephemeral callbacks while claiming durable positions.

That separate slice is now specified by
[`host-live-output-v1.md`](host-live-output-v1.md). H4 uses a distinct endpoint,
no SSE ID, and no durable cursor. H1 remains unchanged and authoritative for
reconnect and terminal convergence.

## Execution and recovery boundary

The Host command layer never invokes a model before the start transaction
commits. `TurnDispatcher` receives only committed identities and opens its own
Runtime composition. C6 execution leases, model/effect lifecycle facts,
cancellation observation, terminal commit and restart recovery remain
authoritative.

H1-T supplies one Runtime-owned model HTTP transport. H1 neither performs
provider retry nor reads provider configuration in route handlers.

## Client compatibility

The retired pre-network scenario and its `output.delta` event are not part of
H1. App conformance uses `live-host-client-v1.json`.

`spec/fixtures/host/live-host-v1.json` freezes command replay/conflict, event
projection, gaps, cancellation wording and every stable error code. Native
HTTP tests must use a real loopback listener and a file-backed SQLite database.

## Acceptance

- generated Rust/Kotlin/KMP bindings compile from the Proto SSOT;
- every mutation proves commit-before-response and commit-before-dispatch;
- identical command replay is stable and conflicting replay commits nothing;
- a process restart replays the same event JSON and positions from SQLite;
- SSE resumes after gaps and never invents a terminal on EOF;
- cancellation and continuation enforce exact owner/version/suspension values;
- typed interaction continuation validates canonical JSON against the durable
  response schema before commit and preserves representation-bound replay;
- route errors contain only stable codes and redacted messages;
- the server refuses non-loopback binding and reads no environment/config file;
- H1-T passes its local real-HTTP matrices;
- strict Rust and full Kotlin gates pass.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
