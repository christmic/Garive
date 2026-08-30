# TUI application architecture

> This Spec defines the resident Garive terminal application's ownership,
> state model, event/effect pipeline, terminal lifecycle, and module boundaries.
> It is the implementation contract for replacing the one-shot TUI binary.

## Audience

Engineers implementing or reviewing `tui/`, the Rust Host client, and the
Runtime-backed end-to-end harness.

## Why

The current binary performs one blocking create/start/follow sequence and
prints lines. It cannot remain responsive while Host work runs, reopen a
Session, preserve an exact retry, render overlays, or restore the terminal
after every exit path. A mature TUI needs one testable application model whose
I/O boundaries remain explicit.

## Ownership

```text
OS terminal/input/signals
        |
        v
TerminalRuntime ----> AppEvent ----> AppModel::reduce
        ^                              | state + effects
        |                              v
   render(View) <---- immutable view  EffectRunner
                                           |
                 HostClient / PreferenceStore / Clock / IdGenerator
                                           |
                                      EffectResult
```

| Layer | Owns | Forbids |
|---|---|---|
| `main` | argument parsing, concrete construction, exit mapping | workflow and rendering policy |
| `terminal` | raw mode, screen, paste/focus/mouse modes, cursor, restore | Host or application state |
| `application` | immutable state, reducer, intent/effect correlation | HTTP, files, clocks, terminal calls |
| `host` | adapter from `garive-host-client` to typed application results | UI policy and retries |
| `persistence` | bounded preference and pending-command ports | Session transcript or Runtime facts |
| `input` | key/mouse/paste normalization and editor state | commands or async work |
| `view` | layout, widgets, semantic styles, cursor placement | state mutation and I/O |
| `runtime` | event multiplexing, effect tasks, redraw cadence, shutdown | domain decisions hidden from reducer |

The TUI depends on the public Host client. It must not import Engine, Runtime
implementation, Proto-generated transport values, SQLite, Provider, adapter,
credential, or configuration-loader modules.

## Target module layout

```text
tui/src/
  lib.rs                 public launch/config/result contract
  main.rs                process entry and safe errors
  args.rs                explicit CLI grammar
  application/
    mod.rs               AppModel and reducer facade
    action.rs            semantic AppAction values
    effect.rs            AppEffect and correlated AppEffectResult
    model.rs             immutable product state
    update.rs            pure transition logic
  host.rs                HostPort and live adapter
  input/
    mod.rs               key/paste/mouse routing
    editor.rs            grapheme-aware multiline editor
    command.rs           slash-command parser and registry
  persistence.rs         preference and pending-command ports/adapters
  runtime/
    mod.rs               event loop and task supervision
    terminal.rs          idempotent terminal guard
    signals.rs           shutdown/resize signals
  view/
    mod.rs               root layout
    conversation.rs      timeline and scroll model
    session.rs           shared Session row presentation
    composer.rs          editor and validation
    navigation.rs        Session/Agent navigation
    overlay.rs           help, command, Session, error, suspension overlays
    status.rs            connection/execution/status footer
    theme.rs             semantic theme tokens
    text.rs              wrapping, markdown subset, safe control filtering
```

No production module exceeds 500 non-test lines. `lib.rs` exports only launch
configuration, launch outcome, and stable launch error types.

## Application values

```text
AppModel {
  generation,
  route,
  definitions,
  sessions,
  selected_session,
  conversations: bounded map<SessionId, ConversationState>,
  composer,
  overlay,
  notice,
  preferences,
  pending_command,
  connection,
  terminal_size,
  focus,
  dirty,
}

ConversationState {
  session,
  items,
  observed_max_position,
  follow_cursor,
  follow_state,
  execution,
  viewport,
  has_older_items,
}

ExecutionState =
  Idle
  | Submitting {effect_id, command_id, request_digest}
  | Following {turn_id}
  | Cancelling {turn_id, command_id, request_digest}
  | Suspended {turn_id, suspension}
  | Continuing {turn_id, command_id, request_digest}
  | CommandUnknown {pending_command}

ConnectionState =
  Connecting
  | Online
  | Disconnected {attempt, next_retry_at?}
  | Reconnecting {attempt}
  | Unavailable {safe_code}
```

Identity and content-bearing values are validated newtypes. `SessionId`,
`TurnId`, `SuspensionId`, `CommandId`, and `EffectId` are never interchangeable
strings. Positions and generations are checked monotonic non-zero integers
where their contract requires it.

`AppModel` is the only mutable application aggregate. Views borrow it
immutably. Async tasks receive owned request values and return results; they do
not retain mutable access to the model.

## Actions, effects, and results

```text
AppAction =
  Boot | QuitRequested | QuitConfirmed
  | TerminalResized | TerminalFocusChanged | Tick
  | InputEdited | SubmitRequested | CancelRequested
  | CommandInvoked | SessionSelected | SessionCreateRequested
  | RetryExactCommand | AbandonUnknownCommand
  | ReconnectRequested | OlderTimelineRequested
  | OverlayOpened | OverlayClosed | SuspensionAnswered
  | EffectFinished(AppEffectResult)

AppEffect =
  LoadPreferences | SavePreferences
  | LoadPendingCommand | SavePendingCommand | RemovePendingCommand
  | LoadDefinitions | LoadSessionPage | LoadTimeline
  | CreateSession | StartTurn | CancelTurn | ContinueTurn
  | FollowEvents | DelayReconnect
  | RingBell | SetTerminalTitle | CopyVisibleText
  | PersistDiagnostic | Exit

AppEffectResult {
  effect_id,
  issued_generation,
  session_id?,
  request_digest?,
  result,
}
```

The reducer returns an ordered `Vec<AppEffect>`. Effects execute outside the
reducer and re-enter through `EffectFinished`. A result mutates state only when
all correlation fields expected by the pending operation match. Stale or
foreign results are ignored and counted by a content-free diagnostic.

Each Session permits at most one mutation effect. Reads, a single event follow,
preference persistence, ticks, and rendering may coexist. Selecting another
Session does not cancel its running Turn or redirect its results.

## Reducer invariants

1. Host snapshots and events are the only source of durable conversation state.
2. User text enters the timeline only after a committed start response or an
   H2 timeline snapshot contains it.
3. EOF, timeout, disconnect, process signal, and spinner completion never
   create a Turn terminal.
4. A terminal transition requires a validated committed Host event or H2 view.
5. Cursor movement is monotonic; gaps are valid; conflicting duplicates fail
   the follow operation.
6. A command identity and semantic request digest are allocated once and remain
   stable through exact retries.
7. A lost mutation response becomes `CommandUnknown`; no new command may target
   that Session until exact retry or explicit abandonment.
8. Composer draft clearing follows durable start acknowledgement, not keypress.
9. Switching route, overlay, theme, or viewport cannot change Host authority.
10. Bounds are enforced before allocation, persistence, Host request, and
    rendering.

## Boot state machine

```text
Cold
  -> terminal acquired
  -> preferences loaded or safely reset
  -> pending command loaded or safely rejected
  -> definitions requested
  -> sessions requested
  -> Ready | NotConfigured | Unavailable
```

The first frame appears before network reads complete and carries a truthful
loading state. A preference failure does not block Host navigation. An invalid
pending-command file blocks mutation for its named Session only after its
validated identity is known; malformed content is quarantined without being
rendered or logged.

When no installed definition exists, the TUI renders `NotConfigured` with the
stable Host error and a quit/help path. The TUI does not ask for model,
credential, endpoint, or database configuration.

## Conversation state machine

```text
Idle -> Submitting -> Following -> Idle
Idle -> Submitting -> CommandUnknown -> Submitting
Following -> Cancelling -> Following -> Idle
Following -> Disconnected -> Reconnecting -> Following
Following -> Suspended -> Continuing -> Following
Suspended -> Continuing -> CommandUnknown -> Continuing
```

Opening a Session loads H2 timeline through a frozen watermark, then follows
H1 after that watermark. Events arriving during the read are replayed by H1;
duplicates at or below the cursor are ignored. A continuation updates the same
Turn item.

The follow task is long-lived but replaceable. It emits each accepted Host
event to the application and emits a distinct stream-ended result. It never
holds the render loop, terminal input, or reducer lock.

## Event-loop scheduling

The runtime multiplexes these sources in priority classes:

| Priority | Sources | Rule |
|---|---|---|
| 1 | termination signal, panic restore request, terminal writer failure | cannot starve |
| 2 | cancel key, blocking suspension answer, Host terminal event | processed before ordinary redraw traffic |
| 3 | other Host results, terminal input, preference result | fair bounded batches |
| 4 | resize/focus, reconnect deadline, animation tick | coalesced |
| 5 | redraw | at most one pending request |

One loop iteration drains at most the configured number of events per source.
A ready stream cannot indefinitely suppress input or cancellation. Redraw is
dirty-state driven and capped at 60 frames per second; idle state performs no
continuous drawing. Resize bursts coalesce for one 16 ms window.

Async effects run in supervised tasks. Task panic or unexpected channel close
becomes a typed internal failure result and initiates controlled shutdown only
when the failed task owns terminal or event-loop integrity.

## Terminal lifecycle

`TerminalGuard::acquire` performs this ordered setup:

1. verify stdin and stderr are terminals;
2. snapshot terminal capabilities needed for exact restore;
3. enable raw mode;
4. enter alternate screen unless `--screen-reader` selects linear mode;
5. enable bracketed paste and focus events;
6. hide the cursor only while a view supplies an explicit cursor position;
7. clear and draw the first frame.

Setup failure rolls back completed steps in reverse order. `restore` is
idempotent and runs for normal return, launch error, signal shutdown, and panic
hook. It drains pending terminal writes, disables mouse/focus/paste features,
shows and resets the cursor, leaves alternate screen, disables raw mode, and
flushes. No async task may write terminal bytes after restore begins.

Mouse capture is off by default. It activates only after explicit user action
or configuration because some terminal/remote combinations leak mouse reports.
The same action can disable it without restarting.

`--screen-reader` uses a linear, non-alternate presentation: completed message
blocks are printed once, status changes use concise lines, interactive overlays
become numbered prompts, mouse is disabled, and animations are disabled. It
consumes the same application model and Host contract.

## Rendering contract

Rendering is a pure function of `AppModel`, theme, and terminal area. It must
not read clocks, files, environment, Git, Host, clipboard, or mutable globals.
Every frame fully determines visible cells and cursor placement.

All Host/user strings pass through control-character filtering before they
become terminal cells. Tabs and newlines have explicit layout meaning; C0/C1
controls, bidi overrides, OSC, CSI, and untrusted ANSI escapes are rendered as
safe visible replacements or removed according to the text Spec. OSC 8 links
are emitted only for locally constructed validated URLs.

## Failure model

| Failure | Application result |
|---|---|
| Invalid launch config or non-TTY | `LaunchError` before state creation |
| Terminal setup/write failure | restore attempt, safe stderr error, exit `2` |
| Host protocol/order/bound error | Session follow stops, blocking protocol notice |
| Transport EOF/timeout | disconnected state with explicit reconnect |
| Known Host command error | stable code notice; no raw body |
| Lost mutation response | `CommandUnknown` and exact-retry UI |
| Preference corruption | quarantine/reset local UI state; Host truth unaffected |
| Effect task panic | safe internal code; no panic payload in UI |

Errors and `Debug` implementations exclude user text, completion text, prompt
JSON, headers, URLs, credential references, and raw response bodies.

## Acceptance

- reducer fixtures cover every state and invariant above, including stale and
  cross-Session effect results;
- property tests cover cursor monotonicity, exact retry identity, bounded
  collections, editor Unicode operations, and reducer panic freedom;
- render snapshots cover terminal widths `20`, `40`, `80`, `120`, and `200`,
  empty/loading/running/suspended/failed/disconnected states, and both themes;
- terminal tests inject failure after every setup step and assert exact
  idempotent restoration;
- a PTY test launches the shipping binary twice, drives input, resizes, exits,
  and proves the shell echo/cursor/mode are restored;
- source boundaries prove `tui/` imports no Runtime implementation, Engine,
  database, Provider, credential, or environment configuration loader;
- focused tests, strict Clippy, warning-denied rustdoc, workspace tests, and the
  architecture gate pass.

## See also

- [`../../docs/tui-source-audit.md`](../../docs/tui-source-audit.md) — exact source evidence and rejected transfers.
- [`live-host-clients.md`](live-host-clients.md) — command identity and event reduction.
- [`host-api-v1.md`](host-api-v1.md) — durable command/event authority.
- [`host-read-model-v1.md`](host-read-model-v1.md) — navigation and timeline snapshots.
- [`client-product-experience.md`](client-product-experience.md) — shared application-state precedent.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
