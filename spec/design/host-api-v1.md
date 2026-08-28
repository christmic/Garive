# H1 — Host API v1

## Status

Accepted executable-shell contract for CLI, TUI, Web, Desktop, Android and
iOS. Local clients may call it in-process; remote clients use the same JSON
envelopes over HTTP/SSE. Transport does not change event semantics.

## Commands

- `POST /v1/sessions` with `{agent_definition_id}` creates a Session;
- `POST /v1/sessions/{session_id}/turns` with `{text}` creates a Turn and its
  first Execution;
- `GET /v1/sessions/{session_id}/events?after_position=N` returns SSE events
  in durable-position order;
- `POST /v1/turns/{turn_id}:cancel` records cancellation idempotently;
- `POST /v1/turns/{turn_id}:continue` creates a new Execution only when the
  Turn is suspended.

Every mutation accepts `Idempotency-Key`. The response repeats stable Session,
Turn and Execution identities and the committed ledger position. A client must
never invent a terminal from HTTP success or stream EOF.

## Event envelope

```text
HostEventV1 {
  api_version: "garive.host.v1",
  session_id,
  position: u64,
  event: session.created | turn.started | output.delta | turn.completed |
         turn.suspended | turn.stopped | turn.failed,
  turn_id?, execution_id?, text?
}
```

Positions begin at one and strictly increase. Unknown events are retained for
audit and ignored for presentation only when they cannot change terminal
meaning. `turn.completed`, `turn.suspended`, `turn.stopped` and `turn.failed`
are the only admitted UI terminals.

## Fake-host acceptance

`spec/fixtures/host/fake-session.json` is the deterministic pre-network host.
Every executable shell consumes or reproduces its command/event sequence:
create Session, submit one Turn, display two deltas, and display exactly one
completion. The fake is a development adapter, not a second API model.
