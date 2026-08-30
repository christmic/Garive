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

## Open acceptance evidence

The baseline now covers one navigation redirect, one form, open shadow DOM and
actual click, Unicode text insertion and clear. Snapshot/node freshness is
deliberately enforced by Runtime's exact target/snapshot/revision binding; it
is not delegated to CDP backend-node lifetime. Click, type-text and clear
binding cases pass in the Runtime unit gate. Concrete `NativeAdapterPort`
composition, cross-origin frames, select/key/scroll/history actions, popups,
downloads, protected-field redaction in the real browser, attachment loss and
Started/crash fault injection remain open. This is not a complete Browser Use
claim.
