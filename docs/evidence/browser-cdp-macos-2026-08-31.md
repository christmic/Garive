# Managed CDP macOS baseline — 2026-08-31

## Candidate

- Host: macOS arm64
- Browser: Google Chrome `152.0.7977.65`
- Adapter: `garive-adapter-browser-cdp`
- Browser state: temporary dedicated profile, random loopback debugging port,
  headless managed process; no personal profile or ambient tab enumeration

## Verified path

The explicit native gate launched the managed browser, consumed its
profile-local `DevToolsActivePort` capability, connected through the bounded
WebSocket transport and passed:

1. `Browser.getVersion`;
2. `Target.createTarget` with `about:blank`;
3. flat `Target.attachToTarget`;
4. session-bound `Accessibility.enable`;
5. `Page.navigate` through a loopback HTTP `302` and exact committed final URL;
6. bounded `Accessibility.getFullAXTree` returning the form button, textbox and
   an open-shadow-root button by accessible name;
7. typed click using an adapter-private backend-node identity, with a refreshed
   AX tree observing the button name change from `Submit form` to `Submitted`;
8. typed Unicode text insertion into the exact textbox without clipboard or
   script, followed by semantic clear and an AX tree with no textbox value.

Command:

```sh
cargo test -p garive-adapter-browser-cdp --test managed_chromium -- --ignored --nocapture
```

Latest result: 1 passed, 0 failed, 0.77 seconds. The ordinary adapter suite also
passed 14 tests; strict all-target Clippy and Rustdoc passed.

The Runtime-owned concrete-port gate independently launched a fresh managed
Chrome profile, created and attached one blank target, bootstrapped it to an
explicit loopback HTTP seed, observed its initial snapshot through
`CdpNativeAdapterPort`, navigated the same-origin redirect
through governed preflight/dispatch, verified the completed receipt and then
observed the form from a new target revision. It selected the exact `stable`
value on a native form select and observed its real `change` handler effect,
then clicked the exact semantic button, used portable Enter against revalidated
browser focus, proved the second activation in a fresh AX observation, and
accepted viewport scroll only after layout metrics proved real movement and the
resulting scroll effect was observed. The same page embeds one same-origin and
one cross-origin iframe. The gate observed the same-origin button, proved the
cross-origin secret was absent, retained only a nameless `opaque_frame`, and
proved a click preflight returns `browser_frame_opaque`. It passed 1 test in
1.36 seconds;
strict Runtime test-target Clippy and warning-free Rustdoc also passed.

```sh
cargo test -p garive-runtime --test native_cdp_managed_chromium -- --ignored --nocapture
```

## Open acceptance evidence

The Runtime mock-transport gate additionally proves concrete-port observe,
preflight, bound navigate/click/type/clear/select dispatch, receipt validation,
post-success invalidation and post-dispatch connection-loss classification as
uncertain with the old binding invalidated. Same-origin redirect rotates the
opaque target revision; cross-origin redirect produces a trustworthy failed
receipt with `browser_origin_denied`. The gate also proves focused key
revalidation, settled viewport scroll, private current-history stale detection,
exact back success, forward origin denial before dispatch, and reload waiting
for a fresh load event before revision rotation. It also proves frame-tree
double-read stale rejection, action pre/post frame revalidation, and revision
rotation when a loader changes without a top-level history change.

The adapter revision is `garive.browser.cdp.v2`. Native select uses
`DOM.resolveNode`, one fixed `Runtime.callFunctionOn` declaration with the option
as a structured argument, and `Runtime.releaseObject`. It rejects non-native,
missing, duplicate and disabled choices without mutation, emits native
`input`/`change` effects on movement, and binds the returned `changed` evidence
into the receipt digest. Typed `DOM.getFrameOwner` binds child frame identities
to their embedding backend nodes. Runtime reads AX subtrees only for same-origin
frames with a fully admitted ancestor chain and collapses a cross-origin owner
to one nameless, valueless and actionless opaque node. The ordinary adapter
suite passes 14 tests and the focused Runtime mapping/port suites pass 17 tests
under strict Clippy.

The baseline now covers one navigation redirect, one form, open shadow DOM and
actual click, Unicode text insertion, clear and exact native option selection.
Snapshot/node freshness is
deliberately enforced by Runtime's exact target/snapshot/revision binding; it
is not delegated to CDP backend-node lifetime. Click, type-text and clear
binding cases pass in the Runtime unit gate. The real managed-Chrome concrete
port gate now binds initial observation, governed navigation, native select,
click, focused Enter activation, settled scroll, receipts and fresh
observation/revision evidence, including same-origin/cross-origin iframe
isolation. Real-browser history actions, popups, downloads, protected-field
redaction in the real browser, attachment loss and durable Started/crash fault
injection remain open. This is not a complete Browser Use claim.
