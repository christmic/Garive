# T2-CDP — Managed Chromium native Browser adapter

## Status

Accepted implementation contract. This adapter is the first high-fidelity
Browser implementation of T2. It is Chromium-specific protocol mechanics and
does not change Engine tool schemas, Runtime authority or recovery ownership.

## Protocol posture

Chrome DevTools Protocol tip-of-tree changes without compatibility guarantees;
the old stable 1.3 subset does not cover Garive's required Accessibility tree.
The adapter therefore freezes its own admitted method set, obtains
`Browser.getVersion.protocolVersion` from each managed browser, records the
adapter/browser revision in evidence and fails closed when a required command
is unavailable. It never treats the online tip-of-tree document as a mutable
runtime dependency.

V1 admits only `Browser.getVersion`, managed blank-page `Target.createTarget`,
flat `Target.attachToTarget`, `Accessibility.enable|disable|getFullAXTree`,
bounded Page navigation/history/layout metrics, `DOM.focus|scrollIntoViewIfNeeded|getBoxModel`,
and bounded `Input.dispatchKeyEvent|dispatchMouseEvent|insertText`. The adapter
freezes the experimental `Input.insertText` shape reviewed against the official
tip-of-tree protocol; absence is a closed protocol failure. Every semantic
element operation needs a constrained typed builder; v1 does not expose `Runtime.evaluate`,
`Runtime.callFunctionOn`, arbitrary scripts or unknown CDP methods.

## Construction and scope

`CdpAdapterConfig` is constructed with one exact `ws://` loopback browser
endpoint and explicit frame, in-flight, event-queue and operation-time limits.
V1 requires exactly one in-flight command, and the time limit covers both
handshake and each command/response exchange. It performs no environment,
profile, port or browser discovery. Credentials, query strings,
fragments, non-loopback hosts and TLS/remote endpoints are rejected. Managed
browsers use a dedicated Garive profile; attached personal tabs require the
separate verified extension/native-messaging contract.

Every command uses a positive JavaScript-safe correlation ID. Page commands
carry the exact flat Target session ID. One complete text frame is bounded
before JSON parsing; mixed result/error responses, empty session IDs and
unmatched correlations fail closed. Unknown event methods may be retained only
inside the bounded event queue and cannot grant an action.

The typed observation client performs `Browser.getVersion`, flat
`Target.attachToTarget`, `Accessibility.enable`, then bounded
`Accessibility.getFullAXTree`. Browser protocol/build evidence and raw AX
values remain adapter types. It accepts explicit target/session/frame inputs;
it does not enumerate or select ambient pages.

Typed navigation enables the Page domain, dispatches one admitted HTTP(S) URL,
then waits for the exact `domContentEventFired`, `loadEventFired`, or main-frame
`lifecycleEvent{name=networkIdle}` requested by T2. It separately consumes the
same main frame's `frameNavigated` event and returns the committed final URL for
Runtime redirect-origin revalidation. The early `Page.navigate` response alone
is never treated as completion.

Engine and Runtime share one parsed canonical HTTP(S) origin function. It
requires an explicit valid port, rejects URL user information, normalizes host
and IP representation, and compares the committed final URL against the exact
prepared origin. CDP URL acceptance alone never grants Network authority.

## Semantic observation

The adapter enables Accessibility and requests `getFullAXTree` with an explicit
depth. Runtime maps AX nodes into its bounded parent-before-child semantic
observation, replaces CDP node IDs with snapshot-local Garive node references,
redacts protected values and stores browser revision plus content evidence.
Cross-origin frames are separate target/origin scopes; an unadmitted frame is
opaque. AX node IDs and backend DOM IDs never reach Core.

The Runtime mapper accepts only Browser targets, rejects duplicate, missing or
cyclic parent evidence, folds ignored AX nodes into the nearest visible
ancestor, normalizes roles/states/actions to portable tokens, and hashes node
references with the Runtime snapshot identity. Its non-configurable v1 baseline
redacts secure/password roles and protected/password/secure properties before
the observation can pass the common bounds validator.

Runtime separately retains a private snapshot binding from each Garive node
reference to the adapter backend node, frame and semantic action set. Click
resolution requires exact target, snapshot ID, target revision, node reference,
declared `click` support and a backend node. CDP scrolls that backend node into
view, obtains its current box and emits one bounded move/press/release sequence;
neither coordinates nor CDP/backend identities enter the Browser tool schema or
Core observation.

Text insertion resolves an exact editable node, focuses it with `DOM.focus`,
then sends one bounded UTF-8 `Input.insertText`; it never reads or writes the
clipboard. Clear focuses the same node, executes the closed `selectAll` editor
command and dispatches one Backspace down/up pair. These typed adapter
operations have separate exact target/snapshot/revision/node/action resolvers.
The concrete CDP `NativeAdapterPort` owns their binding lifetime and receipt
path.

## Runtime composition

`CdpNativeAdapterPort` owns one explicit Runtime Browser target, flat CDP
session, target revision, Runtime-supplied snapshot namespace and connected
client. It performs no target/session discovery. Observe enables Accessibility
once, applies the requested bounds, maps the AX tree and retains only the
current private snapshot binding.

Preflight supports navigate plus `click`, `type_text`, `clear`, `press_key` and
`scroll`. It resolves
the exact semantic operation and hashes canonical command, adapter and backend
evidence into the frozen binding. Dispatch recomputes that binding, invalidates
the old snapshot before crossing CDP, executes exactly once and returns a
receipt with no invented resulting snapshot. Navigation applies the prepared
whole-operation timeout, waits for the requested completion event, revalidates
the committed final origin and rotates the opaque target revision. A
cross-origin redirect returns a trustworthy failed receipt carrying
`browser_origin_denied`; an allowed commit returns completed. Any CDP failure
without trustworthy terminal evidence after dispatch is
`native_action_uncertain`. Press-key freezes the snapshot's one focused private
backend node and refreshes the bounded AX tree before input; changed focus
returns `native_focus_changed` without input. Scroll gets the current
`Page.getLayoutMetrics` visual viewport and emits one `mouseWheel` event at its
center, so tool input never controls pointer coordinates. Every act freezes the
sorted canonical allowed-navigation origins and compares the current
`Page.getNavigationHistory` entry before and after input. A changed committed
entry rotates the target revision; an origin outside that frozen set returns a
trustworthy `browser_origin_denied` receipt, while loss before post-action
history is uncertain. A later observe, explicitly chained from the prior
snapshot, creates the next observation. Select and explicit history commands
stay unsupported until their bindings land.

## Acceptance

- pure config/wire tests reject remote/discovered endpoints, invalid limits,
  unknown commands, oversized frames and mixed terminals;
- Runtime unit gates prove stale target/snapshot/revision/node rejection before
  dispatch; adapter gates must not rely on CDP backend-node lifetime for this;
- a local managed Chromium suite proves AX tree bounds,
  navigation/redirect-origin checks, shadow DOM, cross-origin frame opacity,
  popups, forms/actions, redaction and attachment loss;
- dispatch fault injection proves no blind replay after Started;
- no bundled Chromium, ambient personal profile, environment configuration or
  Computer Use fallback.

The first native baseline is automated as an explicit macOS ignored gate. It
launches an installed Chrome with a temporary dedicated profile and random
debugging port, reads that child process's capability endpoint, and proves
version/create-blank-target/flat-attach/enable-Accessibility, a loopback 302
with exact final URL, and a full tree containing form and open-shadow-root
controls. It also clicks the form button through the typed adapter operation
using an unexposed backend identity and observes the resulting AX-name change.
The same gate inserts Unicode text and clears the textbox, observing both AX
states. Runtime separately proves the exact click binding gate; text-action
binding now passes the same exact gate. Mock-transport concrete-port gates cover
observe, navigate/click/type/clear/key/scroll and success/failed/uncertain binding
invalidation. A second managed-Chrome gate passes initial observation,
same-origin redirected navigation, completed receipt, target-revision rotation
and fresh semantic observation through the concrete Runtime port itself. This
baseline does not satisfy the remaining frame/action/fault matrix by itself.

## Meta

- Owner: `@christmic`
- CDP source reviewed: canonical protocol pages, 2026-08-31
- Status: accepted
