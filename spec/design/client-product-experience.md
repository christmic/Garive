# A-UX1 — Product client experience

> This Spec turns the buildable client shells into one coherent Agent product:
> durable Session navigation, conversation execution/recovery, typed activity,
> responsive native presentation, accessibility, and strict Host-only layering.

## Audience

Desktop React/Tauri, Web, KMP, Android Compose, SwiftUI, shared Host-client, and
test engineers implementing user-visible Garive behavior.

## Why

The current surfaces prove H1 connectivity but expose developer fields, create
a new Session for each action, collapse all failures into text, and cannot
reopen durable work. A usable Agent client must make durable state legible,
preserve exact retry/cancel/continue semantics, recover from disconnects, and
keep configuration and authority in Runtime.

## Product scope

Desktop is the reference product surface because it embeds the configured local
Runtime. Web is a strict Host client for development/deployment compositions.
Android and iOS consume the same KMP presentation contract with native UI.
CLI/TUI retain their focused terminal contracts and do not block A-UX1.

H1/H2 bind only to loopback. Therefore Desktop and a browser running on the
same machine can close live product E2E in A-UX1. A physical mobile device
cannot reach that Host merely by accepting a URL: live remote mobile transport
is gated on an authenticated Gateway or a separately admitted on-device
Runtime. UX-C proves the KMP controller and native UI against injected contract
transport and does not claim deployable remote connectivity.

A-UX1 includes:

- configured/not-configured startup;
- installed-Agent selection without hard-coded IDs;
- durable Session list, creation, reopening, and timeline;
- multi-turn composer with send, cancel, exact retry, and continuation;
- running, disconnected, suspended, completed, stopped, and failed states;
- a typed activity summary derived only from H3 public Host views/events;
- keyboard, screen-reader, reduced-motion, contrast, and responsive behavior.

Memory control, attachments, media generation, voice, notifications, search,
Session rename/delete, organization accounts, and remote authentication require
their own Host/product slices. M2-D later adds Desktop Memory review without
changing the conversation architecture.

## Layering

```text
native composition root
  -> application controller / state machine
      -> HostClient + local preference ports
          -> versioned Host API
  -> platform UI renders immutable view state and sends intents
```

| Layer | Owns | Must not own |
|---|---|---|
| Composition | concrete Host transport, IPC bridge, clocks, ID generator, limits | view state or Agent policy |
| Application | navigation, command identity, reducer, reconnect/retry state | HTTP/IPC details, credentials, Engine facts |
| Domain view | immutable Session/Turn/activity/error presentation values | durable truth or side effects |
| UI | layout, focus, input, native accessibility and rendering | Host calls, command IDs, retries, configuration parsing |

Desktop and Web mount one React Work UI and the same TypeScript product
controller. Their composition roots inject different effect ports: Tauri IPC
for Desktop and bounded HTTP/SSE plus browser preferences for Web. Platform-only
setup, Workspace, Artifact export, updater, menu, picker, and zoom effects stay
capability-gated and never execute in the Web composition. KMP `commonMain`
continues to own the mobile application controller and view values; Compose and
SwiftUI contain no Host workflow.

The shared React implementation is the SSOT for information architecture,
copy, responsive behavior, keyboard commands, focus, theme, density, locale,
Session navigation, conversation, Activity, and capability gates. A browser
fork that merely resembles Desktop is not admitted. Native macOS chrome and
authority surfaces remain Desktop-owned because browser equivalents would
weaken the security boundary.

The target product directories are:

```text
desktop/frontend/src/
  app/                         composition, routes, error boundary
  features/setup/              C2 write-only first-run/reconfigure UI
  features/sessions/           navigation and selection
  features/conversation/       timeline, composer, suspension
  features/activity/           H3 inspector
  features/memory/             M2-D review/control
  state/                       pure controller/reducer and view values
  ipc/                         generated typed Tauri adapters only
  ui/                          tokens and reusable accessible primitives

mobile/shared/src/commonMain/kotlin/com/garive/mobile/
  application/                 pure controller/effect correlation
  host/                        generated Host mapping and transport port
  model/                       immutable product view values
  preferences/                 bounded local preference port
```

Android/iOS mirror product features in their native UI source trees but do not
copy controller logic. A directory name is an ownership boundary, not permission
for circular feature imports: features depend on `state`/`ui` contracts; `state`
never imports a feature or concrete IPC/HTTP adapter.

## Information architecture

| Region | Desktop/tablet | Phone |
|---|---|---|
| Sessions | Persistent collapsible rail | Separate navigation destination/sheet |
| Conversation | Primary center column | Primary full-width destination |
| Activity | Optional right inspector | Drill-in sheet/destination |
| Composer | Sticky bottom of conversation | Safe-area-aware sticky bottom |
| System state | Composer-bound execution state and inline notices | Composer-bound execution state and inline notices |

The conversation column is content-first and has a readable maximum width.
Internal IDs appear only in a copyable details view. Endpoint, credential,
model deployment, and database fields never appear in the normal conversation
surface. A developer Web composition may show its explicit Host URL in a
separate connection screen.

### Desktop composition contract

Desktop and Web share the work DOM, tokens, keyboard model and responsive
geometry. Desktop additionally owns native-window composition:

- the primary title row is 34 points high and preserves the macOS traffic-light
  safe zone in expanded, collapsed and narrow navigation states;
- drag regions never cover an interactive control, and the restore/navigation
  action remains reachable without colliding with native window controls;
- the row presents one quiet route/task glyph and one-line document identity;
  global search stays in Command-K and the rail instead of a permanent capsule;
- execution state never duplicates into the title row: admitted Activity,
  authority and stop remain attached to the bottom Composer;
- Environment uses a compact overlay, while a committed file opens a real
  resizable, tabbed, independently scrolling split workbench;
- a file workbench exposes one close action for the current layer: preview
  close returns to Deliverables, and only the Deliverables layer exposes
  inspector close; tab and toolbar chrome never duplicate identical close
  actions;
- no logo-led browser header, invented avatar, fake sharing action or decorative
  model selector may be added to imitate unavailable product authority.

The Web shell uses the same hierarchy without the traffic-light inset or native
drag behavior. Responsive reflow must not create a second Web-specific visual
language.

### Task control and command center

Desktop and Web derive task labels only from the H2 lifecycle projection. The
presentation groups suspended work as Needs input, running work as Active,
failed work as Failed, completed/stopped work as Completed, and every other
admitted state as Ready. Attention sorts before active, failed, ready, and
completed work; recency breaks ties. Presentation priority never invents
Runtime scheduling, background concurrency, or completion.

`Command-K` opens one modal command center over the current route. It combines
new work, full search, inspector, and settings actions with a bounded list of
priority durable Sessions. Typing filters only H2 public titles and definition
labels. Arrow keys and Tab reach actions, focus remains inside the dialog,
Escape closes it, and focus returns to the invoking control. Search offers All,
Needs input, Active, and Completed lifecycle filters without a separate index.
At narrow widths the same actions and state vocabulary reflow rather than
disappearing.

## Application state machine

```text
AppShell: Booting -> NotConfigured | LoadingNavigation | Unavailable
          LoadingNavigation -> Ready | Unavailable

ConversationExecution:
  Idle -> Submitting -> Following -> Idle
  Idle/Following -> Cancelling -> Following | Idle
  Following -> Disconnected -> Reconnecting -> Following | Unavailable
  Following -> Suspended -> Continuing -> Following
```

`Idle` means no mutation/follow operation is currently controlling the selected
Turn; the timeline still retains its latest terminal view. A committed
completion/stop/failure returns execution state to `Idle`, so the same durable
Session can submit its next Turn. Changing navigation never changes the durable
state of a running Turn.

One immutable `AppViewState` contains configuration state, definitions, Session
page, selected Session, timeline watermark, composer draft, pending command,
connection state, and bounded activity items. Only application intents mutate
it. Stale async completions carry an operation generation and are ignored.

```text
AppIntent =
  Boot | RefreshNavigation | SelectSession | CreateSession
  | EditDraft | SubmitDraft | RetryPending | CancelTurn
  | ContinueSuspension | Reconnect | DismissNotice

AppEffect =
  LoadDefinitions | LoadSessionPage | LoadTimeline | FollowEvents
  | CreateSessionCommand | StartTurnCommand | CancelTurnCommand
  | ContinueTurnCommand | LoadPreferences | SavePreferences

AppEffectResult {
  effect_id, issued_generation, session_id?, request_digest?,
  result: DefinitionsLoaded | SessionPageLoaded | TimelineLoaded |
          HostEventReceived | EventStreamEnded | CommandSucceeded |
          PreferencesLoaded | PreferencesSaved | Failed {AppError}
}

PendingCommand {
  kind, command_id, semantic_request_digest,
  session_id?, turn_id?, issued_generation, status
}
```

The reducer is pure: it emits ordered effects, and only `AppEffectResult`
values re-enter as result intents. At most one mutation is pending per Session;
navigation reads and event following may coexist. Effect-result correlation
requires exact effect identity, Session, generation, and request digest.

Command identity is allocated once when an intent becomes pending and remains
stable across byte-equivalent retries. The draft is cleared only after durable
start success. A lost mutation response remains `unknown`; the UI offers exact
retry and never silently creates a new command. Switching Sessions never drops
a pending identity or redirects its result into the new selection.

## Conversation behavior

- Enter sends; Shift+Enter inserts a line break. IME composition never submits.
- Empty/whitespace-only input and input over the Host byte bound are disabled
  with an accessible explanation.
- User input appears after durable start acknowledgement, not optimistic click.
- While a Turn is following, the textarea remains editable as the selected
  Session's next-instruction draft. It cannot submit until the Turn is terminal,
  is never presented as queued work, and must survive the same preference and
  restart boundary as an idle draft.
- Following begins at the command's committed position. Reopening loads H2
  timeline then follows H1 from its watermark.
- Disconnect preserves durable content and cursor, shows a non-terminal banner,
  and uses bounded explicit reconnect. Exhaustion offers retry; it does not fail
  the Turn.
- Cancel states that it is a request. Controls remain truthful until a committed
  terminal arrives.
- Approval/external-input suspension renders the H2 public prompt and a
  schema-appropriate value action; continue binds the exact suspension identity,
  Session version, response-schema digest, and H1 continuation variant.
  Other suspension kinds render status only unless their own public authority
  contract is accepted.
- Failed/stopped states show stable localized copy and a copyable error code.
  Raw response bodies and exception messages are never rendered.

## Activity presentation

H3 snapshots/events reduce to ordered, bounded activity items with semantic
kind, status, Turn, and durable position. Unknown event names appear as neutral
“Activity updated” entries in details and never alter known Turn state.
Tool arguments, hidden reasoning, credentials, paths, raw provider values, and
internal Ledger facts are absent. Streaming token/tool output is not simulated;
the UI shows a running state until an admitted public event exists.

## Local versus durable state

| State | Owner |
|---|---|
| Sessions, Turns, timeline, suspension, terminal, cursor | Host/Ledger |
| Composer draft, selected Session, rail/inspector state, theme | bounded local preference store |
| Provider/model/credential/database configuration | Desktop backend Runtime configuration |
| Command ID while pending | application controller, optionally crash-safe local pending-command record |

Local preferences are versioned, non-secret, bounded, and disposable. A corrupt
preference file resets UI preferences only; it never changes or hides durable
Host state. Browser storage is not trusted as a Session database.

```text
ClientPreferencesV1 {
  schema_version: 1
  selected_session_id?
  session_rail: expanded | collapsed
  activity_inspector: open | closed
  theme: system | light | dark
  composer_drafts: [{session_id, text}] // bounded LRU, local only
}
```

Unknown versions reset after preserving no fields. Duplicate Session drafts,
unknown keys, oversized text, and invalid enums reject the whole preference
document. A pending command record is stored separately and contains only the
typed fields above; it is removed only after a known durable response or an
explicit user abandonment acknowledgement.

## Visual and accessibility contract

- Shared Desktop/Web surfaces conform to `shared-client-visual-system.md`; new
  components consume its semantic tokens and state grammar rather than local
  palette, radius, typography or motion values.
- Use platform system fonts, semantic spacing/color tokens, light/dark themes,
  and a restrained content-first surface. Status is never encoded by color alone.
- Body text meets WCAG AA contrast; controls retain visible keyboard focus and
  at least 44×44 platform points on touch surfaces.
- Every control has an accessible name, state, and error association. Timeline
  updates use a polite live region; token-by-token announcements are forbidden.
- Focus moves to a blocking error/suspension prompt, returns to the composer
  after acknowledged send, and never jumps on background event arrival.
- Reduced-motion disables non-essential transitions. Layout remains usable at
  320 CSS pixels, 200% text scaling, and desktop keyboard-only navigation.
- Loading uses truthful progress labels or skeleton structure; no infinite
  animation is the sole indication of work.

## Failure and privacy rules

Application errors are typed as `configuration`, `validation`, `command_unknown`,
`host`, `transport`, `protocol`, or `local_preference`. Logs and analytics may
contain stable code, route family, duration bucket, retry count, and view name;
they must not contain user text, Agent output, IDs beyond approved opaque trace
tokens, Session titles, Host URL, headers, credential refs, or raw bodies.

Clipboard/export actions are explicit user gestures. The UI never copies hidden
content with visible content and clears no OS clipboard automatically.

## Delivery and acceptance

| Slice | Required evidence |
|---|---|
| UX-A controller | Shared scenarios for boot, navigation, multi-turn, exact retry, stale completion, disconnect/reconnect, cancel, suspension, unknown event, and every error family. |
| UX-B Desktop | Configured embedded Runtime E2E: create, complete, restart app, reopen Session, second Turn; plus not-configured, keyboard, focus, contrast, 200% text, and responsive tests. |
| UX-C Web/mobile | Web production build and same-machine loopback E2E; KMP controller conformance; Android API 37 Compose device UI flow; iOS SwiftUI/XCFramework build and native state tests. Physical-device live Host connectivity remains Gateway/on-device-Runtime gated. |
| UX-D release gate | No hard-coded definition/input in shipping entry points, no fixture transport, no Engine/database import, no secret/content logging, and dependency/toolchain policy green. |

UX-A begins after coordinated H2/H3 wire fixtures are accepted. UX-B begins
after H2/H3 Runtime projections and A-DESKTOP-C2 configured startup are
verified. UX-C reuses the accepted controller semantics but retains
platform-native UI. Screenshot-only, compile-only, and fake-Host evidence
cannot close a product slice.

The shared `client-product-experience-v1` fixture contains `bootstrap_cases`,
`navigation_cases`, `command_cases`, `follow_cases`, `suspension_cases`,
`activity_cases`, `preference_cases`, and `failure_cases`. Each case supplies
initial state,
ordered intents/effect results, and the complete expected state/effect list.
TypeScript Desktop/Web and KMP consume every semantic case; platform UI tests
cover rendering/focus/navigation and do not reimplement controller semantics.

### UX-A implementation evidence

UX-A is implemented by the pure TypeScript reducer in
`desktop/frontend/src/state/controller.ts` and the conforming KMP reducer in
`mobile/shared/src/commonMain/kotlin/com/garive/mobile/application/ProductController.kt`.
Both consume every ordered case in
`spec/fixtures/host/client-product-experience-v1.json`; strict readers reject
unknown root/case fields, duplicate case names, and omitted expected results.
The product projection adapters in `desktop/frontend/src/state/hostProjection.ts`
and `mobile/shared/src/commonMain/kotlin/com/garive/mobile/host/ProductHostMapping.kt`
map the complete public H2/H3 views without importing Engine, Ledger, Runtime,
database, or provider types.

Executable evidence covers exact result correlation, navigation-generation
invalidation, mutation survival, one mutation per Session, UTF-8 bounds,
bounded unknown activity, durable-ack draft clearing, transport-unknown exact
retry, disconnect/reconnect cursor retention, suspension coordinates, strict
preference/pending documents, and single-flight coalesced preference writes.
Desktop production TypeScript/Vite build, KMP JVM tests, and the iOS/macOS
XCFramework build are the UX-A delivery gates. UX-B and UX-C remain separate;
UX-A completion does not claim any product UI or live-device reachability.

## See also

- [`host-read-model-v1.md`](host-read-model-v1.md) — navigation and timeline data.
- [`host-agent-activity-v1.md`](host-agent-activity-v1.md) — typed public activity.
- [`live-host-clients.md`](live-host-clients.md) — H1 command/reducer semantics.
- [`desktop-system-configuration.md`](desktop-system-configuration.md) — backend-only Desktop configuration.
- [`desktop-configuration-onboarding.md`](desktop-configuration-onboarding.md) — first-run and rotation flow.
- [`desktop-memory-control.md`](desktop-memory-control.md) — Desktop Memory review flow.
- [`../../.agents/dependency-versions.md`](../../.agents/dependency-versions.md) — stable SDK and build governance.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
