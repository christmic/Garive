# Mobile remote work product

> Defines Garive's iOS and Android product around one job: keep server-hosted
> Agent work moving when the user is away from a computer. Product, mobile,
> Runtime, and Gateway engineers use this decision before changing mobile APIs
> or native presentation.

## Audience

Product designers and engineers implementing the KMP application layer,
SwiftUI and Compose apps, Host/Gateway transport, push delivery, and release
evidence. Readers are expected to know Garive's Session, Turn, suspension, and
committed-event vocabulary.

## Why

The existing mobile apps are developer shells. They ask for a loopback URL,
definition ID, and message, always create a Session, then render one terminal
string. A physical phone cannot reach that loopback Host. The surface therefore
does not yet perform the mobile product's actual job.

Remote Agent control has a different interaction rhythm from desktop work:

1. dispatch or reopen durable work in seconds;
2. leave the app while the Agent continues on the server;
3. return because progress changed or a decision is required;
4. inspect safe evidence, answer, approve, cancel, or redirect;
5. confirm a durable result without depending on a laptop connection.

Official Codex mobile product material reinforces this rhythm: mobile is most
valuable for reviewing progress, answering questions, changing direction, and
approving the next step while work continues elsewhere. Garive adopts those
jobs without copying OpenAI APIs, branding, or client-owned execution state.

## Quick start

A successful first-run journey is:

```text
Welcome -> Scan pairing QR -> Verify server name -> Device secured
        -> Work inbox -> New task -> choose Agent -> send
        -> live progress -> leave app -> actionable notification
        -> review request -> approve/answer -> durable completion
```

The shipping app never asks a normal user for a raw Host URL, Agent ID, token,
database path, provider, model, or command identity. A developer-only build may
expose diagnostics behind an explicit build flag.

## Product promise

Garive Mobile is a remote control and supervision surface, not an on-device
Agent Runtime and not a compressed desktop IDE.

| Promise | User-visible outcome | Durable owner |
|---|---|---|
| Stay oriented | See running, waiting, completed, and failed work immediately. | Runtime projections |
| Keep work moving | Answer a question, approve an admitted action, cancel, retry exactly, or add direction. | Runtime commands |
| Start useful work | Create a Session from an installed Agent and submit a bounded objective. | Runtime commands |
| Leave safely | Agent work continues after app suspension or device loss. | Server Runtime |
| Return at the right moment | Content-free push opens the exact authorized Session/Turn. | Gateway routing + Runtime truth |
| Trust the display | Every terminal, activity, request, and result comes from committed public state. | Runtime/Ledger |

## Information architecture

Compact phones expose four stable destinations in an overlay drawer so the
conversation keeps the full vertical canvas. Regular-width tablets keep the
same destinations in an approximately 300-point persistent sidebar with an
independent detail stack. Navigation placement changes with width; destination
identity and state do not.

| Destination | Purpose | Primary contents |
|---|---|---|
| Work | Triage what needs attention now. | Attention queue, running work, recent completions, connection state. |
| Sessions | Browse and search durable work. | Filtered Session list, status, Agent, updated time, pagination. |
| Agents | Start work with an installed Agent. | Capabilities, availability, recent use, new-task action. |
| Settings | Manage this device and remote service. | Paired service, device security, notifications, appearance, diagnostics, sign out. |

Selecting work opens a full-screen conversation. Its hierarchy is deliberately
shallow: identity/status header, durable timeline, optional activity drawer,
then one safe-area composer or blocking decision card.

## Functional scope

### Work inbox

- Attention requests sort first, then running work, then recent terminals.
- Status uses icon, label, and accessible text rather than color alone.
- Pull-to-refresh performs a bounded snapshot refresh; it does not restart work.
- Offline mode retains the last verified snapshot with its age and disables
  mutations until authenticated connectivity returns.
- Deep links from notifications resolve an opaque route token through the
  authenticated service. Notification payloads contain no Session title, user
  input, output, path, tool name, or credential value.

### Session and conversation

- Reopen existing Sessions and continue with additional Turns.
- Render user input, committed Agent output, suspension prompts, stable errors,
  and redacted public activity in durable order.
- Preserve a per-Session draft locally with an explicit size and count bound.
- Submit once with a stable command identity. Unknown outcomes offer exact
  retry and never create a new command silently.
- Cancellation is presented as a request until a committed terminal arrives.
- Disconnect keeps the observed cursor and performs bounded reconnect from the
  same position without declaring failure from stream EOF.
- Long output is selectable and shareable only through an explicit user action.
  Code blocks use horizontal scrolling and copy only their visible content.
- Expanded Activity shows its public label, committed state, and optional
  stable safe code. The code is selectable for support but never replaced by a
  raw provider error, tool argument, path, or internal Ledger value.

### Decisions and intervention

- A supported suspension becomes a prominent decision card with the Runtime's
  public title, message, response shape, and exact suspension coordinates.
- Destructive or externally visible actions require a separate confirmation
  step with the server-projected risk summary. Mobile does not invent risk.
- The user may answer, approve, deny when admitted, or request cancellation.
- Free-form steering is a new durable Turn after the current lifecycle admits
  it; it is never injected into a running execution outside a contract.
- `attention_required` activity is informative until a Runtime command exists.

### Starting work

- New task defaults to the last used installed Agent but always shows its name.
- The composer supports text first. Attachments, voice, camera, and local file
  upload remain absent until their own capability and privacy contracts land.
- Templates are local prompt starters only. They carry no authority and do not
  bypass server validation.
- The app shows durable acknowledgement before navigating to a newly created
  Session. A lost create/start response remains recoverable by exact retry.

### Background and notifications

- Foreground uses authenticated bounded HTTPS plus SSE snapshot-then-follow.
- Background execution is not kept alive indefinitely. The app saves only
  non-secret preference state and pending command identity, then reconnects.
- Push is a wake-up hint, never truth. Opening it refreshes the exact Session
  before presenting a decision or terminal.
- Notification categories are `attention`, `completed`, `failed`, and
  `connection_security`. Content previews default off on the lock screen.
- The user can configure per-category notifications and quiet hours without
  changing Runtime execution policy.

## Pairing and remote security

Mobile connects to an authenticated HTTPS Gateway in front of a Runtime Host.
It never makes an unauthenticated non-loopback H1 Host reachable.

The pairing artifact is short-lived, single-use, and contains only a public
HTTPS service origin, opaque pairing exchange identity, expiry, and service
display name. A QR code or universal/app link carries the artifact. The app:

1. validates scheme, host, bounds, expiry, and exact link version;
2. shows the service name and registrable host for user confirmation;
3. creates a device key in Keychain or Android Keystore;
4. exchanges the one-time artifact for a device-bound access grant;
5. stores only the grant/refresh material in OS secure storage;
6. confirms account/device identity before showing durable work.

The transport refuses cleartext remote origins, redirects, URL credentials,
query tokens, wildcard certificates, and silent fallback to loopback. Local
developer loopback remains a distinct explicit profile. Revocation, expiry,
account mismatch, clock skew, and server key change produce typed signed-out or
security states rather than generic connection errors.

The Gateway owns authentication, actor binding, rate limits, route admission,
push registration, and Runtime routing. It must preserve H1/H2/H3 bodies,
idempotency keys, durable positions, and stable errors without becoming a
second Session store or interpreting Agent facts.

## Native visual direction

The visual system is calm, tactile, and content-first:

- deep ink backgrounds with warm ivory content in dark mode;
- pale mineral surfaces with charcoal text in light mode;
- coral for the single primary action, mint for verified completion, amber for
  attention, and red only for destructive/failure semantics;
- platform system fonts, monospaced treatment only for code and opaque details;
- large 28–34 point destination titles, 16–17 point reading text, and generous
  12/16/24 point spacing;
- rounded 16–24 point cards, hairline separators, restrained shadows, and no
  decorative gradients behind long text;
- one ambient Agent pulse only while following live work, disabled by reduced
  motion and never used as the sole running indicator.

Compose uses Material 3 semantics with Garive tokens. SwiftUI uses native
navigation, sheets, lists, materials, Dynamic Type, haptics, and safe areas.
Both platforms share behavior and visual intent, not pixel-for-pixel layout.

The shared Desktop/Web visual contract is also the source hierarchy for native
mobile adaptation. Mobile preserves its platform navigation and 44-point touch
targets while mapping the same work grammar: one continuous transcript,
width-bounded continuous-corner user prompts, flush Agent output, one
borderless progressive composer, one trailing Send-or-Stop action, and an
attached neutral decision rail. New-task starters are compact command rows and
disappear as soon as a draft exists. Native UI must not regress these concepts
into messenger bubbles, avatar tiles, horizontally scrolling suggestion cards,
permanent durability captions, or duplicate toolbar/composer actions.

## Responsive and accessibility behavior

- Phones use an overlay navigation drawer and full-screen conversation destinations,
  preserving the conversation's vertical canvas; tablets use the same destination
  model in a persistent sidebar.
- Tablets use a navigation rail or split view with Session list and detail.
- Every touch target is at least 44 platform points; Android also meets its
  density-independent minimum.
- Dynamic Type/font scaling to 200% keeps actions reachable and never overlays
  the composer on the timeline.
- TalkBack and VoiceOver receive names, values, roles, errors, and polite
  batched timeline updates; streaming tokens are not announced individually.
- Keyboard and switch access traverse in visual order. Focus moves only for a
  blocking decision/error or a direct user navigation action.
- Reduced motion removes ambient pulse and cross-fade travel. Increased
  contrast retains status borders and text labels.

## Reliability and privacy

- Durable snapshots are bounded and keyed by account, installation, Session,
  and watermark. Signing out erases credentials, push registration, cached
  snapshots, drafts, and pending command records for that account.
- Whenever a paired workspace becomes inactive or enters the background, both
  native shells replace Remote content and accessibility semantics with a
  content-free privacy shield before the OS task-switcher preview is retained.
  Deliberate foreground screenshots remain a platform/user choice unless
  enterprise policy disables them.
- Analytics contain only stable route, result code, latency bucket, app/build
  version, and approved anonymous trace token. They exclude prompts, outputs,
  titles, URLs, IDs, paths, headers, credentials, and raw errors.
- A mobile crash cannot affect Agent execution. Recovery starts from a bounded
  navigation snapshot and exact durable cursor.

## Release quality

A release candidate requires:

- KMP controller/transport unit, property, contract, and reconnect tests;
- Android lint, unit tests, APK/AAB build, API-level device UI flows, TalkBack
  semantics, rotation, background/foreground, offline, and screenshot review;
- iOS XCFramework, Swift tests, app archive, simulator/device UI flows,
  VoiceOver labels, Dynamic Type, background/foreground, offline, and screenshot
  review;
- a credentialed physical iOS and Android run through the authenticated Gateway
  to a disposable real Runtime, including create, reconnect, decision, cancel,
  terminal, sign-out, and revoked-device paths;
- the repository physical-admission gate passing on the same clean revision;
  it rejects emulators, untrusted/private Gateway origins, missing push assets,
  unsigned or wrong-revision apps, mutable failure records, and evidence
  containing free-form or device/service identity fields;
- source scans proving no Engine/database/provider configuration, fixture
  transport, hard-coded identity, secret log, or cleartext remote origin ships.

Compile-only, screenshot-only, fixture-only, and same-process loopback evidence
cannot close remote mobile delivery.

## See also

- [`system.md`](system.md) — product ownership and dependency direction.
- [`../../spec/design/mobile-remote-work-client.md`](../../spec/design/mobile-remote-work-client.md) — normative mobile contract.
- [`../../spec/design/client-product-experience.md`](../../spec/design/client-product-experience.md) — shared client behavior.
- [`../../mobile/AGENTS.md`](../../mobile/AGENTS.md) — KMP/native ownership rules.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: implemented locally; physical remote release evidence pending
