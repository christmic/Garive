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

V1 admits only `Browser.getVersion`, flat `Target.attachToTarget`,
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

## Semantic observation

The adapter enables Accessibility and requests `getFullAXTree` with an explicit
depth. Runtime maps AX nodes into its bounded parent-before-child semantic
observation, replaces CDP node IDs with snapshot-local Garive node references,
redacts protected values and stores browser revision plus content evidence.
Cross-origin frames are separate target/origin scopes; an unadmitted frame is
opaque. AX node IDs and backend DOM IDs never reach Core.

## Acceptance

- pure config/wire tests reject remote/discovered endpoints, invalid limits,
  unknown commands, oversized frames and mixed terminals;
- a local managed Chromium suite proves AX tree bounds, stale snapshots/nodes,
  navigation/redirect origin checks, shadow DOM, cross-origin frame opacity,
  popups, forms, redaction and attachment loss;
- dispatch fault injection proves no blind replay after Started;
- no bundled Chromium, ambient personal profile, environment configuration or
  Computer Use fallback.

## Meta

- Owner: `@christmic`
- CDP source reviewed: canonical protocol pages, 2026-08-31
- Status: accepted
