# A-MOBILE-R — Native remote-work client v1

> Specifies the production iOS/Android client that controls a server-hosted
> Garive Runtime through an authenticated HTTPS Gateway. This contract covers
> remote transport, KMP product state, native UI behavior, secure device state,
> background return, failures, privacy, and release evidence.

## Audience

Gateway, Host, KMP, Android, iOS, security, accessibility, and release engineers
implementing or reviewing physical-device remote Agent work.

## Why

A-MOBILE proves native buildable shells over a loopback H1 client. It does not
support the product's primary mobile use: continue useful Agent work while the
computer is absent. A-MOBILE-R adds a remote client composition without
weakening H1's refusal to expose an unauthenticated non-loopback listener.

## Scope and dependencies

A-MOBILE-R consumes:

- H1 durable create/start/cancel/continue and committed SSE events;
- H2 installed-Agent, Session, and timeline read models;
- H3 redacted committed activity;
- an authenticated Gateway that preserves those contracts and actor scope.

It implements one shared KMP application controller plus native Compose and
SwiftUI presentation. It does not implement an on-device Runtime, file editing,
voice, attachments, media generation, Memory control, arbitrary tool approval,
or a second Session store.

The Gateway authentication and pairing service is a separate G0-R slice. A
client release may be verified against a compatible deployed implementation,
but this Spec does not authorize making `LiveHostServer` remotely reachable.

## Quick start

After native secure storage has resolved a valid access grant:

```kotlin
val connection = HostConnection.Remote(
    origin = "https://agent.example.test",
    authorization = AccessTokenProvider { secureStore.readAccessToken() },
)
val controller = MobileWorkController(
    host = LiveHostClient(connection, limits),
    preferences = preferences,
    identities = identities,
)
controller.dispatch(MobileIntent.Boot)
```

`Remote` rejects any non-HTTPS origin before token lookup or transport. The
loopback development profile remains a separate `HostConnection.Loopback`.

## Remote transport contract

```text
HostConnection =
  Loopback { origin: http://localhost | 127.0.0.1 | [::1] }
  | Remote { origin: https://<dns-host>[:port], access_token_provider }

AccessTokenProvider.read() -> non-empty opaque UTF-8 token | AuthUnavailable
```

The normalized origin has no user info, query, fragment, non-root path, or
trailing ambiguity. Remote IP literals, `.local`, `localhost`, cleartext HTTP,
and non-HTTPS default behavior reject. Loopback never carries Authorization.
Remote sends exactly one `Authorization: Bearer <token>` header on every H1,
H2, and H3 request, including SSE setup. It disables redirects, proxy discovery
where the platform engine permits explicit policy, automatic retries, cookies,
HTTP authentication negotiation, and body logging.

Token resolution occurs per request so native refresh can replace an expired
grant without rebuilding product state. Tokens never enter URL, JSON, protobuf,
errors, `toString`, analytics, preferences, tests snapshots, or screenshots.

The Gateway must return the underlying versioned response on success. It may
add only these redacted errors before Host routing:

| HTTP | Code | Meaning |
|---:|---|---|
| 401 | `authentication_required` | Access grant absent, expired, invalid, or revoked. |
| 403 | `actor_forbidden` | Authenticated actor lacks the requested installation/Session authority. |
| 409 | `device_reauth_required` | Device binding or account context must be re-established. |
| 429 | `rate_limited` | Edge admission refused this request; no mutation was routed. |
| 503 | `runtime_unavailable` | Bound Runtime route is unavailable. |

`Retry-After` is presentation guidance only. A mutation receiving an ambiguous
transport outcome remains `command_unknown`; it is retried only with the same
idempotency key and semantic request digest.

## H2 HTTP completion

A-MOBILE-R requires the loopback Host and Gateway route family below so the
same generated values serve every client:

| Method | Path | Response |
|---|---|---|
| GET | `/v1/agent-definitions` | `AgentDefinitionPageV1` |
| GET | `/v1/sessions?limit=N` | `SessionPageV1` |
| GET | `/v1/sessions/{session_id}` | `SessionViewV1` |
| GET | `/v1/sessions/{session_id}/timeline?after_position=P&limit=N` | `TurnTimelinePageV1` |

This implementation increment may omit `before` only while `next_before` is
always absent and the configured Session bound fits one page. The server must
reject an supplied `before` rather than ignore it. Adding cursor pagination
later preserves these paths and response tags.

Queries reject duplicates, unknown fields, zero/out-of-range limits, negative
positions, malformed percent encoding, and path identity mismatch. Responses
use `application/json`, exact `api_version = "v1"`, and JavaScript-safe unsigned
positions. H2/H3 stable failures remain as specified by their owners.

## Shared application contract

KMP `commonMain` owns the workflow. Native UI renders immutable values and
sends intents; it does not call `LiveHostClient` directly.

```text
MobileWorkState {
  destination: work | sessions | agents | settings | conversation
  connection: signed_out | connecting | online | reconnecting | offline | security_error
  definitions: AgentCard[]
  sessions: SessionCard[]
  attention: WorkCard[]
  selected_session_id?
  timeline: TimelineItem[]
  timeline_cursor
  draft
  pending_command?
  notice?
  refreshing
}

MobileIntent =
  Boot | Refresh | SelectDestination | OpenSession | Back
  | BeginTask | SelectAgent | EditDraft | Submit
  | CancelTurn | ContinueTurn | RetryExact | Reconnect
  | OpenActivity | DismissNotice | SignOut
```

Effects carry an opaque effect identity and generation. Results with a stale
generation, wrong effect identity, wrong Session, or wrong semantic request
digest are ignored. One mutation may be pending per Session; reads and one SSE
follow may coexist. Switching destinations never abandons a pending command.

Boot orders effects as secure connection resolution, definitions, Session
page, selected timeline, then follow from the observed watermark. Failure in a
later effect retains earlier verified state. A refresh snapshots navigation and
the selected timeline before resuming follow. SSE never supplies initial truth.

## Immutable product values

UI values contain public names, localized keys, stable state, safe error codes,
durable positions, and bounded display text. Internal Host IDs are retained for
command correlation and local matching, but are absent from ordinary rows,
accessibility labels, telemetry, and screenshots. Only Agent definition IDs and
revisions have an explicit copyable details sheet.

Known presentation states are:

| Host state | Product label | Action |
|---|---|---|
| no Turn | Ready | Send a new Turn. |
| running | Working | Cancel request available. |
| suspended + supported prompt | Needs you | Render exact response action. |
| suspended + unsupported kind | Paused | Status and cancel only. |
| completed | Completed | Share/copy and send another Turn. |
| stopped | Stopped | Send another Turn when admitted. |
| failed | Failed | Stable code/details and new Turn; exact retry only for unknown command. |
| unknown | Updated | Neutral display, no inferred action. |

H3 activity is secondary detail. `terminal` is authoritative; unknown
kind/state/code uses neutral localized text and enables no command.

## Command behavior

- Command IDs are 26-character lowercase sortable opaque values from an
  injected generator, allocated once per semantic mutation.
- The semantic digest binds command kind, Session/Turn/suspension identities,
  durable watermark/version, selected definition, and exact input bytes.
- Draft input is trimmed only for empty validation; submitted bytes are not
  silently rewritten. The draft clears after a durable start response.
- Native composers and both controllers admit at most 16,384 UTF-8 input bytes.
  Their HTTP command envelope bound is 65,536 bytes, matching the configured
  product Runtime Host and leaving serialization overhead outside the user's
  input budget. UI and controller limits must never disagree.
- Create + first Turn is a two-command workflow. If create succeeds and start
  is unknown, exact retry reuses the created Session and start identity.
- Cancel binds the latest verified Turn and position. UI continues to show
  Working/Needs you until a committed stopped or other terminal state arrives.
- Continue binds exact suspension ID, Session version, representation, schema
  digest when present, and response bytes.
- App suspension persists a bounded pending-command record. Relaunch resolves
  durable state before offering exact retry or abandonment.

## Local storage

```text
MobilePreferencesV1 {
  schema_version: 1
  selected_destination
  selected_session_id?
  theme: system | light | dark
  notification_preview: hidden | status_only
  drafts: [{session_id, text}] // at most 20, each at Host input bound
}

PendingMobileCommandV1 {
  schema_version: 1
  kind: create | start | cancel | continue
  semantic_digest, created_at_epoch_ms
  command_id?                         // single-stage mutation
  create_command_id?, start_command_id? // create + first Turn workflow
  definition_id?, session_id?, turn_id?
  position?, suspension_id?, session_version?, input_json?
}
```

The exact bounded input for an ambiguous start/continue is stored separately
as the matching local draft payload. The record digest binds its byte digest,
the current command identity, both create/start identities when applicable,
and every durable coordinate. Storage writes payload before record and clears
record before payload, so a torn write cannot produce an admitted retry.
Relaunch rejects unknown keys, invalid shapes, over-bound payloads, modified
identities or payload/digest mismatches and clears only these disposable local
values.

Preferences and pending records contain no access token, endpoint, account
name, Agent output, activity details, or response body. Unknown keys/versions,
duplicates, invalid enums, or exceeded bounds reject the complete document.

Connection grants live only in Keychain/Keystore-backed native storage. Sign
out removes the grant first, unregisters push on a best-effort authenticated
path, then clears account-scoped local values. Failure to contact the service
does not retain the local credential.

## Native screen contract

### Work

- The compact native top bar shows `Remote`, authenticated Host context, a
  navigation affordance, and new-task action; it does not duplicate a desktop
  window title at mobile large-title scale.
- A leading navigation drawer adapts the desktop sidebar: Work, Sessions,
  Agents, Settings, and bounded recent Sessions. The conversation canvas does
  not lose vertical space to a permanent four-item bottom bar.
- Compact-width phones use the drawer as an overlay. Regular-width iPad uses a
  persistent approximately 300-point Remote sidebar and an independent detail
  navigation stack instead of stretching the phone column.
- Android widths of at least 700 dp use the same persistent split-workspace
  principle with an approximately 300 dp sidebar; smaller widths retain the
  modal drawer.
- `Needs you` rows come first with one clear action and safe context.
- Running and recent Sessions use flat text-first rows, compact metadata, and
  a redundant status label/mark. Cards are reserved for decisions, composer,
  errors, and content that benefits from containment.
- Empty state explains how to start work and offers `New task`.

### Sessions

- Search is local over loaded public labels/text and internal Session IDs only
  until an admitted server search contract exists. A matched internal ID is not
  rendered in the result row or exposed through accessibility semantics.
- Filter chips are `All`, `Working`, `Needs you`, and `Done`.
- Exactly one filter is visibly and semantically selected; selection is not
  conveyed by list contents or color alone.
- Rows expose Agent label, state, safe time decoration, and latest public text
  preview when admitted; swipe does not delete because deletion is absent.

### Conversation

- Navigation title uses Agent label; status sits beneath it.
- Timeline bubbles distinguish user, Agent, system status, suspension, and
  activity. Activity is collapsed by default.
- Expanded Activity renders the public label and committed state plus an
  optional selectable stable safe code. Raw provider bodies, paths, arguments,
  and internal identifiers remain absent.
- Sticky composer respects keyboard/safe area and supports multiline input.
- Blocking suspension replaces composer actions but never hides prior content.
- Approval exposes two explicit actions, `Decline` and `Approve once`, and
  states that scope is the current Turn and committed history remains.
- Cancel uses a confirmation sheet. Share/copy is an explicit menu action.

### Agents and new task

- Cards show installed public Agent label, availability, and public capability
  labels. Raw definition IDs remain in details.
- The mobile composer adapts the desktop Work starters into one horizontal,
  glanceable row: `Synthesize` writes `Turn notes into a clear decision memo`,
  `Analyze` writes `Find the key patterns and recommend next steps`, and
  `Create` writes `Draft a polished project brief from my outline`.
- A starter replaces only the editable draft. It never submits, selects broader
  authority, or changes the chosen Agent; users can edit the outcome before
  starting server work.
- Selecting an Agent opens the new-task composer; send remains disabled for
  empty/oversized text or offline/auth-invalid state.

### Settings and pairing

- Pairing accepts QR, universal/app link, or manual code; raw tokens are never
  accepted in a general text field.
- Settings shows service display name, verified host, device name, notification
  controls, theme, build version, diagnostics, and destructive sign out.
- Diagnostics copy contains only stable version, safe codes, connection state,
  and anonymous trace token.

## Visual and accessibility tokens

Semantic tokens are shared by name, with native color values selected for
platform contrast:

```text
surface.canvas, surface.raised, surface.sunken
text.primary, text.secondary, text.inverse
accent.primary, status.running, status.attention,
status.success, status.failure, border.subtle, focus.visible
space.1=4, space.2=8, space.3=12, space.4=16, space.6=24, space.8=32
radius.control=12, radius.card=20, radius.sheet=28
```

The light palette follows desktop Work's warm paper canvas (`#fbfaf6`), strong
paper surface, ink text, and `#315fcf` primary action. Dark appearance maps the
same roles to a black canvas, near-black raised surfaces, high-contrast text,
and a brighter blue action. It is not a separate layout. Default-size hierarchy
uses native system typography near 17 pt for primary rows, 13 pt for metadata,
and compact inline navigation titles; accessibility sizes reflow instead of
shrinking or clipping.

All text/control combinations meet WCAG AA. Touch targets are at least 44×44
points. Dynamic Type/font scaling 200%, TalkBack/VoiceOver, keyboard traversal,
reduced motion, increased contrast, RTL-safe layout, dark/light appearance,
320-point width, rotation, and tablet split presentation are release scenarios.

## Background and push

The Gateway registers an APNs device token or FCM Firebase Installation ID
against the authenticated device grant. Registration is platform-bound and
sign-out unregisters it before best-effort grant revocation.
The payload is versioned and content-free:

```text
MobileWakeHintV1 {
  schema_version: 1
  route_token
  category: attention | completed | failed | connection_security
  collapse_key
}
```

`route_token` is short-lived, opaque, single-use or replay-safe, and resolves
only after authentication. A notification never directly authorizes a command.
Foreground receipt coalesces by `collapse_key`, refreshes snapshots, and then
announces one semantic change. Background limits never become failure truth.
Runtime's private loopback wake projection assigns categories from durable Turn
state. Gateway suppresses startup history, pages all Sessions, and relays only
category transitions. iOS registers for APNs/background notification delivery;
Android uses FCM's current FID registration callbacks. Both reject extended or
malformed envelopes, resolve the opaque token with the device grant, and open
the verified Session or Settings destination only after authenticated refresh.

## Failures and recovery

| Family | Product response |
|---|---|
| validation | Inline accessible explanation; no effect emitted. |
| authentication | Sign-in/pairing gate; retain no secret in error state. |
| authorization | Close inaccessible detail and refresh scoped navigation. |
| command_unknown | Preserve pending identity across restart; keep exact retry or explicit warned abandonment visible even after reads reconnect. |
| host | Show stable localized code; refresh durable state when appropriate. |
| transport | Retain verified snapshot/cursor; bounded reconnect. |
| protocol | Stop applying response, show security-safe error, require refresh. |
| local_storage | Reset disposable preferences; preserve secure grant if valid. |
| security | Stop remote calls, obscure sensitive decision UI, require re-pair. |

Every non-authentication failure and local validation notice is rendered in an
accessible native banner. Dismissing the banner clears presentation only; it
does not clear a pending identity, editable draft, verified history, or any
server fact. Unknown mutations keep their Retry exact and warned abandonment
actions even if the stable notice itself was dismissed.

Retries use capped exponential backoff with full jitter for reads/follow only.
Mutations never retry automatically. Network restoration may trigger one
coalesced refresh. Repeated unauthorized responses do not form a retry loop.

## Privacy and logging

Shipping code and tests enforce that logs, analytics, diagnostics, notification
payloads, crash breadcrumbs, and accessibility identifiers exclude prompts,
outputs, titles, Host URL, headers, tokens, account values, Session/Turn/tool
IDs, paths, raw activity labels, response bodies, and exception strings.

Permitted telemetry is app/build version, platform/API version, view name,
stable error family/code, latency bucket, reconnect count bucket, and an
approved rotating opaque trace token.

## Verification

### Shared KMP

- origin and authentication matrix, including failure before token lookup;
- exact header/path/query/body tests for every H1/H2 route and remote SSE;
- generated protobuf round trips for H2/H3 presence and unknown strings;
- complete controller scenarios for boot, navigation, create/start, multi-Turn,
  stale results, unknown mutation, exact retry, reconnect, cancel, suspension,
  unknown activity, background return, sign out, and every error family;
- property checks for monotonic cursors, one pending mutation per Session,
  stale generation isolation, digest stability, and bounded local state;
- secret/content canary scans over errors, debug strings, logs, fixtures, and
  serialized preferences.

### Android

- lint, unit tests, debug/release build, and native Compose device tests;
- pairing, Work, Sessions, Agents, conversation, decision, offline, revoked
  grant, background/foreground, deep link, rotation, keyboard, font scale,
  TalkBack semantics, reduced motion, and tablet layout;
- visual snapshots in light/dark and compact/expanded width.

### iOS

- XCFramework, Swift tests, simulator/device app build, and native UI tests;
- the same semantic journeys using Keychain, universal links, APNs wake hints,
  Dynamic Type, VoiceOver, reduced motion, rotation, and split view;
- visual snapshots in light/dark and compact/regular width.

### Physical remote E2E

One disposable configured Runtime and authenticated Gateway must prove on both
platforms: pair, discover Agents, create, follow, background, reconnect, answer
a suspension, cancel another Turn, observe terminal after Runtime restart,
revoke the device, fail closed, re-pair, and sign out. Evidence records only
build revisions, stable codes, timestamps, and pass/fail steps.

## Release boundary

A-MOBILE-R is `done` only when client API/code/tests and the physical remote E2E
are verified. Native UI/controller completion without G0-R stays `partial` and
must say remote deployment is gated. G0-R implementation without physical
native evidence does not close A-MOBILE-R.

## See also

- [`../../docs/architecture/mobile-remote-work.md`](../../docs/architecture/mobile-remote-work.md) — product and visual decision.
- [`mobile-gateway-v1.md`](mobile-gateway-v1.md) — authenticated pairing and narrow Runtime edge.
- [`client-product-experience.md`](client-product-experience.md) — shared controller semantics.
- [`host-read-model-v1.md`](host-read-model-v1.md) — H2 navigation/timeline truth.
- [`host-agent-activity-v1.md`](host-agent-activity-v1.md) — H3 redacted activity.
- [`live-host-clients.md`](live-host-clients.md) — H1 command/replay behavior.
- [`../../mobile/AGENTS.md`](../../mobile/AGENTS.md) — native/KMP ownership.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted for implementation
