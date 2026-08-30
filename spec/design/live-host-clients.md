# A1 — Live Host v1 client contract

## Status

Accepted implementation contract for replacing fixture-only App behavior after
H1 and R1 exist. H0 protobuf values and H1 HTTP/SSE semantics remain the wire
source of truth.

## Client boundary

Every App uses one `HostClient` abstraction:

```text
create_session(command_id, definition_id)
start_turn(command_id, session_id, text)
cancel_turn(command_id, session_id, turn_id, through_position)
continue_turn(command_id, session_id, turn_id, suspension, version, input)
events(session_id, after_position)
```

Browser, CLI, TUI and mobile receive an explicit loopback Host base URL. They
do not discover ports from environment, read Provider credentials or access
Engine/SQLite. Desktop uses typed Tauri IPC to embedded R1; its frontend still
implements the same semantics and never receives Provider configuration.

“Mobile receives a loopback URL” proves KMP client semantics only when the Host
runs on that same device/test process. A physical device cannot reach a Desktop
loopback listener. Remote mobile product connectivity requires an authenticated
Gateway or a separately admitted on-device Runtime and is not an A1 claim.

## Command identity and retry

Command IDs are stable client-generated identities. Retrying the same mutation
reuses the same ID. A timeout/lost response is unknown; only the byte-equivalent
command may be replayed. Clients never silently generate a new ID or mutate
content. `command_conflict` is terminal for that local command.

Required response identities and non-zero committed positions are validated.
Known Host errors remain typed. Unknown future codes become
`unknown_host_error` with HTTP status but without raw response text.

## Event reduction

Consumers require `api_version == "v1"`, the requested Session, strictly
increasing non-zero positions and a non-decreasing reconnect cursor. Gaps are
valid. Duplicate positions at or below the saved cursor are ignored; a new
backward/conflicting position fails the stream.

Unknown event names are preserved but do not mutate known UI Turn state. EOF,
disconnect and heartbeat are never terminal. Only `turn.completed`,
`turn.suspended`, `turn.stopped` and `turn.failed` create typed terminal or
suspension views. V1 uses committed completion text and does not concatenate
the fake `output.delta` presentation events.

## Surface responsibilities

| Surface | Required behavior |
|---|---|
| CLI | Create/reuse Session, submit one Turn, follow events, print committed text and map typed terminal/failure to documented exits. |
| TUI | Keep Session/Turn/cursor state and render ordered events without blocking execution. |
| Web | Strict TypeScript client/reducer with explicit base URL and injectable transport. |
| Desktop | Typed IPC starts/reuses embedded R1; React consumes typed command/event results. |
| Android/iOS | KMP owns Host request/event reduction; native UIs render shared state only. |

Fixture transports may remain in test support but cannot be imported by a
shipping entry point. `FakeHost`, its protobuf messages and scenario fixture
were removed after the final consumer migrated.

## Bounds and privacy

Constructors receive non-zero command bytes, event bytes, reconnect attempts
and follow deadline bounds. HTTP redirects are rejected. Browser CORS is an
explicit embedded-Host policy, never `*`. Logs, Debug and UI errors contain no
command input, event text, credentials, headers or raw bodies.

## Acceptance evidence

- shared `live-host-client-v1.json` scenarios for gaps, duplicate replay,
  disconnect-before-terminal, unknown event and every stable Host error;
- CLI/TUI tests against real loopback H1;
- Web/Desktop strict TypeScript tests over injectable transport/reducer;
- Desktop Rust IPC tests against temporary embedded R1;
- KMP fixture and wire round-trip tests used by Android/iOS bridges;
- source scans proving migrated entry points import no Fake Host support;
- native build gates for every claimed surface.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
