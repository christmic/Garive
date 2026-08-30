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

V1 admits only `Browser.getVersion`, managed blank-page `Target.createTarget`, flat `Target.attachToTarget`,
`Accessibility.enable|disable|getFullAXTree`, bounded Page navigation/history,
and bounded Input key/mouse dispatch. A future semantic element operation may
add a constrained typed builder, but v1 does not expose `Runtime.evaluate`,
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

## Acceptance

- pure config/wire tests reject remote/discovered endpoints, invalid limits,
  unknown commands, oversized frames and mixed terminals;
- a local managed Chromium suite proves AX tree bounds, stale snapshots/nodes,
  navigation/redirect origin checks, shadow DOM, cross-origin frame opacity,
  popups, forms, redaction and attachment loss;
- dispatch fault injection proves no blind replay after Started;
- no bundled Chromium, ambient personal profile, environment configuration or
  Computer Use fallback.

The first native baseline is automated as an explicit macOS ignored gate. It
launches an installed Chrome with a temporary dedicated profile and random
debugging port, reads that child process's capability endpoint, and proves
version/create-blank-target/flat-attach/enable-Accessibility/non-empty-full-tree.
This baseline does not satisfy the remaining page/action matrix by itself.

## Meta

- Owner: `@christmic`
- CDP source reviewed: canonical protocol pages, 2026-08-31
- Status: accepted
