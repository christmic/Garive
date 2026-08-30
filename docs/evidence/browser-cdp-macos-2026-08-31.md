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
   an open-shadow-root button by accessible name.

Command:

```sh
cargo test -p garive-adapter-browser-cdp --test managed_chromium -- --ignored --nocapture
```

Latest result: 1 passed, 0 failed, 1.06 seconds. The ordinary adapter suite also
passed 9 tests; strict all-target Clippy and Rustdoc passed.

## Open acceptance evidence

The baseline now covers one navigation redirect, one form and open shadow DOM.
Cross-origin frames, stale nodes, actual form actions, popups, downloads,
protected-field redaction in the real browser, attachment loss and
Started/crash fault injection remain open. This is not a complete Browser Use
claim.
