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
5. bounded `Accessibility.getFullAXTree` returning a non-empty tree.

Command:

```sh
cargo test -p garive-adapter-browser-cdp --test managed_chromium -- --ignored --nocapture
```

Result: 1 passed, 0 failed, 1.93 seconds. The ordinary adapter suite also
passed 8 tests; strict all-target Clippy and Rustdoc passed.

## Open acceptance evidence

Navigation and redirect origins, frames, shadow DOM, stale nodes, popups,
forms, downloads, protected-field redaction, attachment loss and Started/crash
fault injection remain open. This document is a protocol-connectivity and AX
observation baseline, not a complete Browser Use claim.
