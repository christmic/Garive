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

## Portable execution and sandbox vocabulary

T2 extends the closed execution capability set with
`browser_observe`, `browser_act`, `computer_observe`, and `computer_act`.
Observe capabilities may appear on `ReadOnly` definitions; act capabilities
are mutating and require a non-read access plus `NeverReplay`. Browser and
Computer capabilities are never represented as generic process authority.

Every Browser profile proves `browser_session_scope`, `snapshot_binding`, and
`resource_limits`. Every Computer profile proves `native_target_scope`,
`snapshot_binding`, and `resource_limits`; Computer act additionally proves
`focus_revalidation`. Definitions that may capture pixels also require
`screen_capture_scope`. Browser definitions capable of navigation additionally
declare Network and prove `network_origin_scope` plus
`redirect_revalidation`.

Runtime resource keys bind the admitted session/target identity used for
conflict planning; exact Network resources bind canonical origins. These are
separate accesses. A Browser or Computer access never implies filesystem,
process, clipboard, credential, microphone, camera, or unrestricted network
authority.

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
  failure_code?
  native_evidence_digest
  resulting_snapshot_id?
}
```

`completed` receipts carry no `failure_code`; trustworthy `failed` receipts
carry exactly one frozen native failure code. A post-dispatch failure with
known native evidence remains a receipt-backed failure. Missing trustworthy
terminal evidence is `native_action_uncertain` and has no fabricated receipt.

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

Target discovery is mode-specific, not an adapter-wide privilege. A managed
session may enable browser-level page-target discovery because the complete
profile is Garive-owned. An attached session must not enable global discovery;
new-page evidence must arrive through the verified attachment/native-messaging
boundary for the exact granted tab.

### New-page and popup boundary

Before a managed action, Runtime clears only queued `Page.windowOpen` and page
target-creation evidence, then correlates a bounded window-open intent with a
new page whose exact `openerId` is the admitted parent. An unrelated target,
missing/mismatched opener, non-page target, malformed URL, inconsistent target
origin or late evidence cannot be attributed to the action.

An action may create at most eight attributable pages and a session may retain
at most 32 pending pages. A requested canonical origin must appear in that
action's explicit `allowed_navigation_origins`; navigation itself supplies no
popup authority. A matching page becomes `pending` only: it is not observable
or actionable until Runtime separately attaches it, assigns session-local
identities and completes normal page admission. A denied, inconsistent or
over-bound page is closed by exact target identity. Any close failure is
`native_action_uncertain`.

Popup creation may change the browser foreground target. After auditing all
attributable pages, Runtime restores the exact admitted parent target before a
later action can use its snapshot. Attached sessions neither perform this
managed discovery nor infer authority from ambient tabs. Popup evidence and
pending identities remain Runtime-private; Core receives only the governed
action receipt and later separately admitted observations.

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
opaque nodes in v1. Independently admitting another origin requires a future
contract revision; top-level Network authority does not imply frame admission.

One browser observation freezes a bounded `Page.getFrameTree` before semantic
collection and requires an exactly equivalent typed frame tree after all frame
work. Frame identity includes the exact parent, loader, URL and security origin;
any difference rejects the mixed observation as stale. Runtime admits a child
only when its canonical security origin equals the main document origin and
its complete parent chain is already admitted. A same-origin descendant below
an opaque ancestor therefore remains opaque.

The CDP adapter resolves every child through `DOM.getFrameOwner`. Runtime reads
`Accessibility.getFullAXTree(frameId)` only for admitted same-origin frames and
combines them under one aggregate node/text bound. An unadmitted frame's proven
owner becomes exactly one `opaque_frame` with no name, value, actions or backend
node identity; descendant semantics are never requested. Acting on that
snapshot-local reference fails `browser_frame_opaque` before dispatch.

Chromium AX does not reliably identify native password inputs: it may expose a
password control as an ordinary `textbox` with a masked value and no protected
property. For every actionable text node, the adapter therefore performs one
fixed, depth-zero, non-piercing `DOM.describeNode` classification against the
exact backend node and retains only whether the bounded attribute pairs prove a
native `input[type=password]`. Runtime marks a proven password node redacted,
publishes no text actions, and never exposes DOM attributes. Immediately before
`type_text` or `clear`, it repeats that exact classification; a control changed
to password invalidates the binding and fails `native_sensitive_action_required`
before focus or input dispatch.

### Browser tools

```text
garive.browser.observe@1
garive.browser.navigate@1
garive.browser.act@1
```

All three require portable ASCII `session_id` and `page_id` values of at most
128 characters and bind the exact Runtime resource
`browser:{session_id}:{page_id}`. `observe` binds it `Read`; `navigate` and
`act` bind it `Write`. The catalogue receives admitted pages, canonical
origins and policy revision explicitly; it performs no environment, profile or
browser discovery.

`observe` requires `max_nodes <= 10,000` and
`max_text_bytes <= 1,048,576`; optional `expected_previous_snapshot_id` makes
incremental observation fail stale rather than silently switching history. Its
duration ceiling is 30 seconds and exact-access ceiling is one.

`navigate` requires `expected_snapshot_id`, `target_revision`,
`destination_url`, separately
declared canonical `destination_origin`, `wait_until`, `timeout_ms`,
`max_nodes`, and `max_text_bytes`. The pure resolver requires the URL origin to
equal `destination_origin`; that origin is a separate `Network(origin, Write)`
access. V1 canonical origins include an explicit port. `wait_until` is
`dom_content_loaded | load | network_idle`; timeout is at most 120 seconds.
Canonicalization parses the complete HTTP(S) URL, rejects user information and
missing, zero or out-of-range explicit ports, normalizes scheme, host and IP
representation, and returns `scheme://host:port`. Both the frozen origin
catalogue and every destination use that exact representation; authority string
slicing is not an origin security boundary.

`act` requires `expected_snapshot_id`, `target_revision`, one action, and at most 16 explicit
`allowed_navigation_origins`; an empty array means action-caused navigation is
blocked. Exact action shapes are:

| Action | Required detail | Forbidden fallback |
|---|---|---|
| `click`, `clear` | `node_ref` | coordinates |
| `type_text` | `node_ref`, bounded `text` | clipboard |
| `select_option` | `node_ref`, bounded `option` | free-form script |
| `press_key` | one closed portable `key` | raw scan codes or caller-selected focus |
| `scroll` | non-zero integer `delta_x`, `delta_y` | caller-supplied pointer coordinates |
| `go_back`, `go_forward`, `reload` | no detail field | ambient tab selection |

Portable keys are `enter`, `tab`, `escape`, `backspace`, `delete`, four arrow
keys, `home`, `end`, `page_up`, `page_down`, and `space`. Unknown fields,
mixed action shapes, duplicate origins, zero scroll, an unadmitted page, or an
origin outside the frozen catalogue fail during preparation.

`observe` is read-only. `navigate` binds an exact canonical destination origin
and uses `NeverReplay` after dispatch unless the adapter proves no navigation
started. `act` supports the closed set:

`Click`, `TypeText`, `Clear`, `SelectOption`, `PressKey`, `Scroll`,
`GoBack`, `GoForward`, `Reload`.

Click/type/select require a snapshot-local node reference and supported action.
`select_option` treats `option` as one exact native `<option>.value`, bounded to
4,096 Unicode scalar values and 16,384 UTF-8 bytes at the adapter boundary. The
CDP adapter resolves the bound backend node and invokes one versioned constant
function; caller text is passed only as a structured argument and is never
interpolated into executable source. The function accepts only a native
`HTMLSelectElement`, requires exactly one matching enabled option, changes the
value, and emits bubbling `input` then `change` only when the value moved. It
returns a bounded selected/unavailable outcome, verifies the resulting exact
value, and releases the resolved remote object. Missing, duplicate, disabled or
non-native options produce a trustworthy unsupported receipt without mutation;
protocol/transport loss after dispatch remains uncertain.
Press-key binds the snapshot's unique focused semantic node and revalidates the
same adapter-private backend focus immediately before input; absent, ambiguous
or changed focus fails before input. Scroll is bound to the page snapshot and
the adapter derives its event point from the current browser-reported visual
viewport center. A possible scroll completes only after bounded layout metrics
prove page-position movement; an existing edge may complete without movement,
and missing settlement evidence is uncertain. Back and forward derive one exact
adjacent entry from bounded browser history and prevalidate its origin; reload
waits for a fresh load event. Every history mutation proves the resulting
current entry, invalidates the prior snapshot, and rotates the target revision.
The private observation binding includes current history identity, so ambient
history changes make old input stale. Key names use a portable closed catalogue;
text is one bounded UTF-8 value.
Every action also re-reads the exact frame tree immediately before dispatch and
again after trustworthy action/history evidence. A changed loader or frame tree
rotates `target_revision` even when the top-level URL/history entry is unchanged;
post-dispatch frame evidence loss is uncertain. The receipt digest binds whether
the frame tree changed.
Coordinates are not accepted by browser v1. File upload requires a separate
opaque workspace file capability. Download requires a separately authorized
Artifact target and receipt; it never writes to ambient Downloads.

`navigate` and every redirect revalidate exact origins. Popups/new tabs become
new page identities and remain unavailable until policy admits them.

## Computer Use capability

### Session and target scope

A Desktop session freezes an admitted application identifier set, window
selection policy, display scope, input permission posture, screenshot policy
and resource limits. On macOS, an admitted application target binds its dynamic
code-signing identity plus a broker-private process-instance identity. The
broker revalidates both immediately before observation or input; a bundle name,
bundle identifier, or reusable PID alone is insufficient. XPC caller identity
is a distinct boundary authenticated from the connection's audit token by the
operating system as specified below.

The macOS application-instance binding takes one explicitly admitted Security
requirement and PID; it performs no application-name discovery. It reads the
PID and process start seconds/microseconds with `proc_pidinfo`, resolves dynamic
code with `SecCodeCopyGuestWithAttributes`, validates that code against the
requirement, and records the signed identifier plus bounded CodeDirectory hash.
It validates the dynamic code again and rereads process start evidence before
returning. Changed evidence fails the operation. Preflight repeats this resolver
and requires byte-identical process-start, identifier, and CodeDirectory data;
therefore a restarted process, reused PID, replaced executable, or differently
signed instance cannot inherit an old target revision.

Observation enumerates only admitted applications/windows. The Accessibility
tree exposes bounded role, label, value summary, enabled/focused/selected state,
supported actions and geometry. Secure text fields expose no value. Garive's
own credential/configuration surfaces and OS security prompts are protected
targets unless a focused policy explicitly allows observation without input.

The public macOS Accessibility SDK does not expose an AX window-number
attribute or a public AX-to-`CGWindowID` conversion. Consequently the adapter
must not treat a title, geometry, enumeration index or separately discovered
CoreGraphics window as AX identity. The broker retains the exact enumerated
`AXUIElement` behind an opaque Runtime window ID. Before and after observation
it revalidates the signed application instance, re-enumerates that process's
`AXWindows`, and requires CoreFoundation equality with the retained element.
Bindings are local to one broker observer and cannot cross observer ownership.
Restart, replacement, disappearance or a foreign binding fails as a changed
target before any input.

Native semantic projection is iterative and rejects cycles or duplicate AX
objects. It enforces the caller's node and visible UTF-8 limits while reading,
then emits a flat parent-before-child tree with one optional unique focus.
Portable `press`, non-secure `set_value`, and native `type_text` capabilities
are exposed only when AX proves their exact semantic support. `type_text` is
limited to non-secure `AXTextField` and `AXTextArea` nodes that are both
settable and focused. Unknown native actions remain unavailable. Secure text
values are never read, all text capabilities are withheld, and the result
records native redaction.

The native observation result retains a broker-private, positionally exact
mapping from every snapshot node index to the AX object read for that node.
Before `press`, `set_value`, `type_text`, or `press_key`, native preflight
rechecks permission, process and window identity, rebuilds the bounded semantic
projection and requires exact snapshot equality, then requires CoreFoundation
equality for the selected node. Keyboard input additionally requires the
admitted application to remain frontmost, the exact retained window to equal
the application's focused AX window, and one unique focused snapshot node.
`type_text` requires that node to be the explicitly selected text node.
An observation binding is atomically consumed immediately before native
dispatch and can dispatch at most once. Native keyboard events use a private
CoreGraphics event source and `CGEventPostToPid` for the exact verified process;
they are not posted to a global event tap and never use the clipboard. The
closed portable key set is Enter, Tab, Escape, Backspace, Forward Delete,
arrows, Home, End, Page Up, Page Down and Space. Permission loss, focus change,
changed semantic state, replaced nodes and protected values fail before
dispatch. After dispatch, the adapter obtains a new observation; loss of
trustworthy post-dispatch evidence is uncertain and the consumed action is
never repeated. `set_value` and `type_text` accept at most 32,768 Unicode scalar
values and 131,072 UTF-8 bytes at this native boundary, matching the stricter
Runtime tool-schema character bound.

### Computer Use tools

```text
garive.computer.observe@1
garive.computer.act@1
```

Both tools bind exactly one Runtime resource
`computer:{desktop_session_id}:{application_id}:{window_id}`. These are opaque
Runtime identifiers of at most 128 portable ASCII characters; platform
adapters map them to verified code-sign/audit-token/window identities. The
catalogue receives the exact admitted targets and policy revision explicitly.

`observe` requires `max_nodes <= 10,000`, `max_text_bytes <= 1,048,576`,
`capture = none | window`, `max_capture_bytes <= 8,388,608`, and
`max_capture_pixels <= 16,777,216`. Capture bounds are explicit even when
capture is `none`, so changing capture posture changes the Prepared Call.

`act` requires `expected_snapshot_id`, `target_revision`, and one exact action
shape. Semantic actions are `press(node_ref)`,
`set_value(node_ref,value)`, `type_text(node_ref,text)`,
`press_key(key)`, and non-zero `scroll(delta_x,delta_y)`. They reject
every coordinate field; an unsupported AX action never switches to pixels.

Coordinate actions are `move_pointer`, `click_point`, and `drag`. Each binds
`display_id`, snapshot pixel width/height, `scale_milli`, and visible-frame
origin/size. Points use snapshot-local integer pixels, must lie inside the
half-open visible frame, and the frame itself must fit inside the snapshot.
Drag binds distinct start/end points and both must pass the same check.
Display identity uses the same portable opaque-ID grammar. Missing geometry,
mixed semantic fields, zero movement, out-of-frame points, and an unadmitted
target fail during preparation.

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

### macOS XPC caller admission

The packaged native service accepts only the already-authorized Garive backend.
Garive system configuration supplies one exact code-signing requirement; the
service does not discover it from an environment variable or infer it from a
process name. The requirement is non-empty, at most 4,096 UTF-8 bytes, valid
Security requirement syntax, and cannot be the broad `always` expression.

Before activating its listener, the service installs that requirement with
`NSXPCListener.setConnectionCodeSigningRequirement`. macOS evaluates it against
the connecting peer identity carried by XPC before calling the listener
delegate. The delegate additionally requires a positive peer PID, the exact
configured effective user and the exact configured login audit-session ID.
Those public connection facts are correlation and scope checks; they do not
replace the system code-signing decision and PID is never an authority by
itself. A rejected connection receives no exported Computer Use object.

macOS Accessibility, Screen Recording and Automation permissions are requested
only at first use after an in-product explanation. Denial/revocation produces a
typed unsupported state without affecting other Agent capabilities. Screen
capture is skipped when semantic observation suffices.

The native package exposes a side-effect-free permission inspector first:
`AXIsProcessTrusted()` and `CGPreflightScreenCaptureAccess()` report current
posture but never request trust. Prompting APIs may be called only by a later
explicit product interaction. Automation permission is target-specific and is
not inferred from Accessibility or Screen Recording posture.

Screenshots are cropped to admitted displays/windows, redact protected regions
before persistence, have explicit byte/pixel/retention bounds and never appear
in telemetry. Clipboard is not read or written in v1. Microphone/camera are not
part of T2.

## Durability and recovery

Runtime commits observation metadata/content bindings before a model may refer
to node references. It commits `native.action.prepared`, safety/grant and
adapter binding before dispatch, `native.action.started` immediately before
the native boundary, then a trustworthy receipt and resulting observation.

The Runtime-facing adapter contract is `NativeAdapterPort`. Its platform-neutral
v1 values use distinct typed target/snapshot/node/action identities, a bounded
flat parent-before-child semantic tree, snapshot-scoped focus, a non-dispatching
`preflight_action`, and a single post-Started `dispatch_action`. Adapter
implementations must return only the frozen stable failures below. The port is
not an alternate authority or ledger boundary and does not make a platform
implementation claim by itself.

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

The initial Swift package gate validates prompt-free permission mapping and the
XPC caller admission policy. Its real anonymous-listener test derives the test
process's designated requirement, installs it at the listener, admits the exact
same-user/audit-session peer, and completes one exported-object round trip.
The package also resolves the running test process through dynamic Security
validation plus `proc_pidinfo`, proves exact revalidation, rejects a wrong
signer and unavailable PID, and rejects a forged process-start identity.
Packaged service identity and rejection from a separately signed process remain
release evidence, not a claim of this package-level gate.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
