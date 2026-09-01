# TUI application architecture

> This Spec defines the resident Garive terminal application's ownership,
> state model, event/effect pipeline, terminal lifecycle, and module boundaries.
> It is the implementation and conformance contract for the resident TUI.

## Audience

Engineers implementing or reviewing `tui/`, the Rust Host client, and the
Runtime-backed end-to-end harness.

## Why

The shipping binary is now a resident application rather than the historical
blocking create/start/follow client. This contract keeps its application model,
I/O boundaries, terminal restoration, and asynchronous ownership reviewable as
the remaining physical-platform evidence is collected.

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

At master `5d1babef`, the side-effectful application path is no longer a
scaffold. Typed `AppAction`/`AppEffect` correlation owns bootstrap reads,
Session pages, exact snapshots, and create/start/cancel/continue mutations;
`PersistencePort` seals pending mutations before their Host effect is issued.
The runtime owns supervision, terminal scheduling, and H1/H4 subscription
cancellation, but cannot accept a stale generation, Session, cursor, or request
digest on the model's behalf. Executable ownership is pinned by
[`application_reducer.rs`](../../tui/tests/application_reducer.rs),
[`effect_runner.rs`](../../tui/tests/effect_runner.rs),
[`host_effect_runner.rs`](../../tui/tests/host_effect_runner.rs), and
[`architecture.rs`](../../tui/tests/architecture.rs), across commits
`1b6a4046`, `face1b02`, `b852a4d5`, `1619eeb4`, `cdc8a2f7`, `36f34f05`,
`83d2a341`, `b6eb2541`, and `2e533c44`. This evidence does not claim that
pure, synchronous editor operations are asynchronous effects.

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
    kill_buffer.rs       single-entry private kill/yank text component
    keymap.rs            typed default chord, intent, and help-label catalog
    logical_line.rs      newline-delimited cursor targets
    history.rs           transient bounded prompt-history browser
    mouse_gesture.rs     deterministic composer multi-click classification
    commands.rs          parser, typed registry, and shared availability contract
  persistence.rs         preference and pending-command ports/adapters
  runtime/
    mod.rs               event loop and task supervision
    controller.rs        terminal event and key-owner orchestration
    controller/overlay.rs modal key routing and filtered-list activation
    controller/mouse.rs  modal-safe pointer routing
    terminal.rs          idempotent terminal guard
    signals.rs           shutdown/resize signals
  view/
    mod.rs               root layout
    command_suggestions.rs pure anchored-menu rendering and pointer geometry
    conversation.rs      TurnBlock rendering and visual-cell scroll geometry
    inspector.rs         shared Inspector rendering and pointer geometry
    inspector/
      row_layout.rs      content-driven Inspector rows, window, and hit geometry
    session.rs           shared Session row presentation
    footer.rs            focus-derived contextual actions
    linear.rs            screen-reader presentation components
    composer.rs          shared Unicode row layout, selection, viewport, frame, and cursor rendering
    navigation.rs        Session/Agent navigation
    overlay.rs           help, command, Session, error, suspension overlays
    overlay/geometry.rs  popup, visible-window, and pointer geometry
    status.rs            connection/execution/status footer
    theme.rs             semantic theme tokens
    text.rs              wrapping, markdown subset, safe control filtering
    title.rs             content-free terminal title presentation
```

No production module exceeds 500 non-test lines. `lib.rs` exports only launch
configuration, launch outcome, and stable launch error types.

Command discovery has four explicit owners. `input/commands.rs` is the catalog,
parser, help, argument-completion flag, and availability source;
`application/model.rs` derives prefix matches and stores only selection plus
the dismissed draft; `view/command_suggestions.rs` owns bounded layout,
painting, visible-window math, and hit testing; `runtime/controller.rs` owns
key priority and completion edits. Mouse routing consumes view geometry and
cannot reproduce row coordinates. The modal command palette and visual-only
anchored menu share the catalog but remain separate components because their
search grammar, focus, accessibility, and backdrop contracts differ.

Conversation navigation has an equally explicit projection boundary.
`install_timeline` sorts the typed H2 `TurnTimelineItem` snapshot once and
projects one keyed `TurnBlock` per Turn plus a bounded
`ConversationLandmark` list. The block key is the exact internal
`(session_id, turn_id)` identity; its children are one User request, an
`ActivityStack`, an optional committed answer, and an optional terminal or
suspension outcome. H1 activity replaces the matching activity child and H2
snapshot install replaces the matching block children without deriving Turn
ownership from adjacent rendered rows. `LiveAnswer` remains a separately keyed
ephemeral child and only the matching durable answer replaces it.

Inspector has one pure application projection. `InspectorVariant`,
`InspectorState`, `InspectorEntry`, and typed activation derive only from the
validated application model; views cannot rediscover recovery policy or Turn
ownership from strings. The model persists only whether Inspector is open.
Current variant and selected entry key are transient. Wide column, narrow
overlay, pointer, and linear screen-reader paths consume the same entry order
and activation value. `InspectorRowLayout` is the single geometry authority for
variable-height visual entries: a label with no independent detail consumes
one row, while an entry with independent safe detail consumes two. Rendering,
selection visibility, pointer hit testing, and overlay height cannot separately
infer that height.

A landmark contains only its one-based ordinal, public `started_position`,
and sanitized public prompt preview. The navigator never reconstructs
identity by parsing rendered text or exposes a Turn ID. Activation resolves
the block whose User child owns `started_position`; absence is a safe no-op
with a local notice. Session or timeline replacement clears the landmark
filter, selection, and overlay before installing the new projection.

Composer multi-click also has separate owners. `input/mouse_gesture.rs`
classifies same-cell clicks within 500 ms as place, word, or logical-line
selection; `EditorState` resolves Unicode grapheme classes and newline ranges;
`view/composer.rs` remains the only screen-cell hit-test authority. Runtime
stores only transient click timing/count and cancels it on keyboard, paste,
resize, focus loss, or a click outside the composer.

Explicit selection copy keeps the same ownership split. `EditorState` exposes
only the currently selected admitted text; the typed command context derives
whether that value exists; the controller admits `Alt+C` or the catalog action;
and `runtime/clipboard.rs` alone encodes the bounded OSC 52 write. The footer
derives its selection-specific hint from model state. No view recomputes byte
ranges, no pointer gesture writes the clipboard, and copy does not clear,
persist, or submit the selection.

Kill/yank has an equally explicit private boundary. `input/kill_buffer.rs`
owns one in-memory text value no larger than the composer's admitted byte
bound. `EditorState` alone computes selection or logical-line ranges and
performs the corresponding one-transaction edit; the controller only maps
`Ctrl+U`, `Ctrl+K`, and `Ctrl+Y`. The buffer is independent of undo/redo and
survives ordinary draft replacement, but runtime clears it before loading a
different Session. It never enters `AppModel` serialization, preferences,
prompt history, diagnostics, OSC 52, Host requests, or rendered view state.

Keyboard defaults have one typed source. `input/keymap.rs` maps an exact
terminal chord to a semantic intent and owns its visual/spoken help labels.
The controller resolves intent and applies current focus, overlay, execution,
and frozen-composer policy; visual Help and linear Help consume the catalog's
presentation rows. Neither view may restate chords, and the catalog may not
mutate model state. `input/logical_line.rs` owns newline-delimited Ctrl+A/E
targets separately from the component-owned visual Home/End geometry.

The composer is similarly bounded. `EditorState` owns admitted text and
grapheme-indexed editing state; the private `EditorLayout` in
`view/composer.rs` derives sanitized display tokens and wrapped rows as a pure
width-dependent presentation model. Text spans, selection styles, cursor
placement, and scroll derive from those rows. Ratatui receives already-wrapped
lines and does not perform a second, divergent wrapping pass.
Root layout asks the composer component for desired visual height at the
available width; it never estimates height from `EditorState::line_count`.
The component also owns screen-cell-to-grapheme hit testing. The mouse
controller stores only the transient down/drag/up ownership bit and sends
grapheme placement intents to `EditorState`; it does not inspect text widths,
line breaks, theme spans, or responsive coordinates.
Keyboard Up/Down follows the same boundary: root view derives the composer's
actual inner width, `EditorLayout` returns the visual-row target and preferred
terminal-cell column, and the controller forwards that intent to
`EditorState`. The editor owns directional selection collapse and mutation but
does not duplicate responsive wrapping geometry.
Home/End follows the identical request path for a visual-row edge. Modified
`Ctrl+Home/End` bypasses presentation geometry and remains an editor-owned
document-boundary operation.

Prompt recall has three explicit owners. `persistence.rs` validates and bounds
durable `PromptHistoryEntryV1` records; `input/history.rs` owns only the
transient browse index plus the original draft text and grapheme cursor;
`EditorState` owns the recalled editable buffer. Root view remains the only
source of visual-row boundaries. The controller asks history for an older
entry only when unmodified Up has no selection and the shared visual layout
cannot move farther up; it asks for a newer entry only while browsing and the
layout cannot move farther down. History state never owns rendering, files,
Session transcript facts, or responsive coordinates.

The title presenter is pure and returns only bounded product labels derived
from typed connection/execution state plus a loaded Session ordinal. The
terminal guard owns OSC-title writes, suppresses unchanged titles, and resets
the title to neutral `Garive` during normal exit, setup failure, signal exit,
and unwinding. Screen-reader and full-screen runtimes consume the same title.

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
Non-list action overlays additionally consume the same typed bindings in the
terminal controller, fullscreen renderer, and linear renderer. The controller
dispatches semantic intents (`Close`, `ConfirmQuit`, `AcceptEphemeral`,
`ExactRetry`, or `AbandonPending`) rather than duplicating presenter labels.

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

## External-editor handoff

`runtime/external_editor.rs` owns bounded editor-command resolution, private
temporary-file lifetime, child outcome classification, and edited-text
normalization. `runtime/app.rs` remains the sole TTY owner: it drops the event
reader, restores every acquired mode, launches the child with inherited stdio
and no shell, waits, reacquires the configured modes, and forces a full redraw.
Controllers may request this handoff but may not spawn or manipulate raw mode.

The request snapshots a content digest and Session identity, never prompt text
in `Debug`, diagnostics, or effects. A successful result replaces the draft as
one undoable edit only when both identities still match. Host traffic remains
bounded; no second terminal reader may exist while the child owns stdin.

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
- Last reviewed: 2026-09-01
- Status: accepted
