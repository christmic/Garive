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
passed 11 tests; strict all-target Clippy and Rustdoc passed.

The Runtime-owned concrete-port gate independently launched a fresh managed
Chrome profile, created and attached one blank target, observed its initial
snapshot through `CdpNativeAdapterPort`, navigated the same-origin redirect
through governed preflight/dispatch, verified the completed receipt and then
observed the form from a new target revision. It then clicked the exact semantic
button, used portable Enter against revalidated browser focus, proved the second
activation in a fresh AX observation, and accepted viewport scroll only after
layout metrics proved real movement and the resulting scroll effect was observed.
It passed 1 test in 0.87 seconds;
strict Runtime test-target Clippy and warning-free Rustdoc also passed.

```sh
cargo test -p garive-runtime --test native_cdp_managed_chromium -- --ignored --nocapture
```

## Open acceptance evidence

The Runtime mock-transport gate additionally proves concrete-port observe,
preflight, bound navigate/click/type/clear dispatch, receipt validation,
post-success invalidation and post-dispatch connection-loss classification as
uncertain with the old binding invalidated. Same-origin redirect rotates the
opaque target revision; cross-origin redirect produces a trustworthy failed
receipt with `browser_origin_denied`. The gate also proves focused key
revalidation, settled viewport scroll, private current-history stale detection,
exact back success, forward origin denial before dispatch, and reload waiting
for a fresh load event before revision rotation.

The baseline now covers one navigation redirect, one form, open shadow DOM and
actual click, Unicode text insertion and clear. Snapshot/node freshness is
deliberately enforced by Runtime's exact target/snapshot/revision binding; it
is not delegated to CDP backend-node lifetime. Click, type-text and clear
binding cases pass in the Runtime unit gate. The real managed-Chrome concrete
port gate now binds initial observation, governed navigation, click, focused
Enter activation, settled scroll, receipts and fresh observation/revision
evidence. Cross-origin frames, select and real-browser history actions, popups,
downloads, protected-field redaction in the real browser, attachment
loss and durable Started/crash fault injection remain open. This is not a
complete Browser Use claim.
