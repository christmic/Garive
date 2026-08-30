# T2 — Native Browser and Computer Use capabilities

## Status

Accepted implementation contract. T2 adds semantic browser control and native
desktop interaction after F0/T1. It does not authorize invisible remote control
or use generic shell commands as an automation substitute.

## Product decision

Use the highest-fidelity native boundary available:

1. browser work uses a browser-native semantic protocol and DOM/accessibility
   snapshot whenever possible;
2. application work uses operating-system Accessibility APIs for target
   discovery and actions;
3. pixels/coordinates are a declared fallback, never the default semantic
   identity;
4. screenshots supplement state; they are not authority or durable truth;
5. every action binds the exact prior observation and returns a new
   observation/receipt.

Browser capability and Computer Use are separate execution domains. A browser
action cannot silently fall back to global mouse/keyboard control when its
semantic target fails.

## Ownership

`engine/tools` owns provider-neutral observation/action schemas, immutable tool
definitions and pure binding checks. Runtime owns session identities, policy,
F0 authorization, snapshot storage, redaction, action dispatch, receipts and
recovery.

Platform adapters own only native mechanics:

- managed/attached browser adapters use the browser's supported automation
  protocol or extension/native-messaging boundary;
- macOS Computer Use is an XPC-brokered Swift/AppKit service under
  `desktop/macos-native/`, using Accessibility, ScreenCaptureKit and native
  event APIs only after OS permission grants;
- Windows/Linux later provide capability-equivalent adapters and explicitly
  report unsupported controls until verified.

The native broker never embeds Engine, opens the Ledger or decides authority.
The Rust backend sends one already-authorized bounded command and validates one
typed reply.

## Distinct identities

```text
BrowserSessionId, BrowserPageId, BrowserNavigationId,
BrowserSnapshotId, BrowserNodeRef,
DesktopSessionId, ApplicationId, WindowId,
DesktopSnapshotId, AccessibilityNodeRef, NativeActionId
```

All are Runtime-owned, non-empty and non-interchangeable. Node references are
ephemeral capabilities scoped to one snapshot. Navigation, window replacement,
process restart or snapshot revision invalidates them.

## Common observe/action protocol

```text
ObservationRequest {
  session_id
  target_id
  expected_previous_snapshot_id?
  bounds
}

Observation {
  snapshot_id
  target_revision
  semantic_tree
  focused_node?
  screenshot_reference?
  redaction_summary
  bounds_applied
}

ActionRequest {
  action_id
  session_id
  expected_snapshot_id
  target_revision
  action
  expected_postcondition?
}

ActionReceipt {
  action_id
  invocation_id, grant_id
  adapter_id, adapter_revision
  prior_snapshot_id
  terminal_classification
  native_evidence_digest
  resulting_snapshot_id?
}
```

Semantic trees are bounded, provider-neutral JSON with stable field names and
snapshot-local node references. Text/value fields carry sensitivity labels and
may be replaced by redaction tokens. Raw screenshots and full native trees are
Runtime content references with separate retention/access policy.

An action starts only after C4 preparation, exact resource resolution, F0
safety decision, C5 authorization and adapter preflight. Changed snapshot,
target revision, focus ownership or permission state rejects before native
dispatch.

## Browser capability

### Session modes

```text
BrowserSessionMode = Managed | Attached
```

- **Managed** creates a dedicated Garive automation profile with explicit
  storage/download/network policy. It never reuses the user's personal profile.
- **Attached** controls only tabs explicitly granted through a verified browser
  attachment/extension handshake. It cannot enumerate other profiles/tabs.

Credentials, cookies, local storage and password-manager contents are never
returned to Core. Existing authenticated page state may be used only inside the
granted attached/managed session and policy scope.

### Browser snapshot

The observation contains page/navigation identity, exact current URL origin,
title, load state, viewport, focused element and a bounded semantic tree:

```text
BrowserNode {
  node_ref
  role
  name?
  value_summary?
  states: canonical set
  actions: canonical set
  bounds?
  sensitivity
  children
}
```

DOM/backend identifiers remain adapter-private. Shadow DOM, frames and native
browser chrome are separate scope boundaries. Cross-origin frames appear as
opaque nodes unless that origin and frame are independently admitted.

### Browser tools

```text
garive.browser.observe@1
garive.browser.navigate@1
garive.browser.act@1
```

`observe` is read-only. `navigate` binds an exact canonical destination origin
and uses `NeverReplay` after dispatch unless the adapter proves no navigation
started. `act` supports the closed set:

`Click`, `TypeText`, `Clear`, `SelectOption`, `PressKey`, `Scroll`,
`GoBack`, `GoForward`, `Reload`.

Click/type/select require a snapshot-local node reference and supported action.
Key names use a portable closed catalogue; text is one bounded UTF-8 value.
Coordinates are not accepted by browser v1. File upload requires a separate
opaque workspace file capability. Download requires a separately authorized
Artifact target and receipt; it never writes to ambient Downloads.

`navigate` and every redirect revalidate exact origins. Popups/new tabs become
new page identities and remain unavailable until policy admits them.

## Computer Use capability

### Session and target scope

A Desktop session freezes an admitted application identifier set, window
selection policy, display scope, input permission posture, screenshot policy
and resource limits. On macOS, application identity is the code-signed bundle
identity plus process audit token; a bundle-name string alone is insufficient.

Observation enumerates only admitted applications/windows. The Accessibility
tree exposes bounded role, label, value summary, enabled/focused/selected state,
supported actions and geometry. Secure text fields expose no value. Garive's
own credential/configuration surfaces and OS security prompts are protected
targets unless a focused policy explicitly allows observation without input.

### Computer Use tools

```text
garive.computer.observe@1
garive.computer.act@1
```

`observe` is read-only. `act` supports the closed set:

`Press`, `SetValue`, `TypeText`, `PressKey`, `Scroll`, `MovePointer`,
`ClickPoint`, `Drag`.

Semantic `Press`/`SetValue` is preferred and requires an Accessibility node
reference. Coordinate actions additionally bind display ID, window ID,
snapshot dimensions, scale, visible frame and an exact point/segment inside
the admitted window. Runtime rechecks the frontmost/target window immediately
before dispatch; it never clicks through an unexpected overlay.

Global shortcuts, system dialogs, permission panes, password fields,
credential prompts and destructive controls are separate protected action
classes. Unsupported native roles/actions fail closed rather than switching to
coordinate fallback.

## Sensitive-action policy

Runtime classifies action targets before authorization:

| Class | Examples | Required posture |
|---|---|---|
| observe | read page/app structure | admitted scope and redaction |
| reversible edit | type in an unsent draft | exact target, bounded action |
| navigation | open URL, change page | origin policy and receipt |
| external commit | send message, submit form, publish, purchase | fresh explicit interaction bound to action digest |
| credential/security | password, keychain, permissions, account recovery | denied by default; dedicated future contract |
| destructive | delete, overwrite, revoke, close unsaved work | fresh explicit interaction and recoverability policy |

The target can become more sensitive after observation. Preflight reclassifies
from current native state and requires a new interaction/grant when necessary.
Approval of one button/node/snapshot does not authorize another.

## Permissions and privacy

macOS Accessibility, Screen Recording and Automation permissions are requested
only at first use after an in-product explanation. Denial/revocation produces a
typed unsupported state without affecting other Agent capabilities. Screen
capture is skipped when semantic observation suffices.

Screenshots are cropped to admitted displays/windows, redact protected regions
before persistence, have explicit byte/pixel/retention bounds and never appear
in telemetry. Clipboard is not read or written in v1. Microphone/camera are not
part of T2.

## Durability and recovery

Runtime commits observation metadata/content bindings before a model may refer
to node references. It commits `native.action.prepared`, safety/grant and
adapter binding before dispatch, `native.action.started` immediately before
the native boundary, then a trustworthy receipt and resulting observation.

Browser/desktop mutations are `NeverReplay` by default. If Started has no
receipt, Runtime observes current native state and may satisfy an exact declared
postcondition only as reconciliation evidence; it never repeats the action
automatically. Observation itself may retry under the same bounded scope.

## Stable failures

`native_capability_unavailable`, `native_permission_required`,
`native_permission_revoked`, `native_target_not_admitted`,
`native_snapshot_stale`, `native_node_stale`, `native_action_unsupported`,
`native_focus_changed`, `browser_origin_denied`, `browser_frame_opaque`,
`browser_attachment_lost`, `native_sensitive_action_required`,
`native_result_bound_exceeded`, `native_receipt_invalid`, and
`native_action_uncertain` are compatibility codes.

## Acceptance evidence

- shared Rust/Kotlin semantic value/binding fixtures; no Kotlin native-adapter
  claim;
- browser adapter contract suite against real locally served pages covering
  navigation, frames, shadow DOM, stale nodes, redirects, popups, forms,
  downloads, redaction and attachment revocation;
- macOS native XPC tests for code-sign identity, AX trees/actions, focus/window
  races, coordinate transforms, scale/multi-display, permission denial and
  revocation;
- screen-capture redaction/cropping/retention tests and protected-field canaries;
- explicit interaction tests for send/submit/purchase/destructive controls;
- fault injection before/after Started/receipt/result proving no blind replay;
- packaged-app tests on clean macOS with hardened runtime, entitlements and
  first-use System Settings flows.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
