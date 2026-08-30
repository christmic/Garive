# macOS Computer Use XPC admission baseline — 2026-08-31

> Evidence for engineers packaging the macOS Computer Use broker. It admits the
> listener identity boundary while leaving application control explicitly open.

## Audience

Engineers implementing or reviewing the native broker and release bundle.

## Why

An XPC method must not become native authority merely because a process knows
the service name. Admission must bind the signed Garive backend and login scope.

## Candidate

- Host: macOS arm64
- Toolchain: Apple Swift 6.3.3, package language mode 6.3
- Deployment target: macOS 14
- Native package: `desktop/macos-native`

## Quick start

```sh
swift test --package-path desktop/macos-native
```

## Reference

`NativeXPCPeerAdmissionPolicy` receives every value through construction. It
rejects empty, oversized, NUL-containing, malformed, or unconditionally broad
code-signing requirements before a listener exists. Security framework parses
the requirement without an Objective-C exception path.

Before activation, the policy installs the exact requirement with
`NSXPCListener.setConnectionCodeSigningRequirement`. XPC therefore evaluates
the connecting peer's authenticated identity before the listener delegate. The
delegate then validates positive PID, effective user and login audit session;
none of those public values substitutes for the signature gate.

The native integration test obtains the actual Swift test process's designated
requirement from Security framework, installs it on an anonymous listener,
connects through `NSXPCConnection`, validates the peer facts, and completes an
exported-object `ping` reply. Pure negative cases cover user/session mismatch,
invalid PID/session and malformed or `always` requirements.

Latest result: 4 Swift Testing tests passed, including four permission-posture
argument cases and one real XPC round trip. No permission prompt, application
enumeration, screen capture, input dispatch, environment read, or Engine/Ledger
access occurs.

## Open evidence

This package gate is not a packaged-service claim. The generated Rust/Swift IDL,
separately signed service/backend rejection matrix, hardened-runtime bundle,
dynamic application-instance identity, AX observation/actions,
ScreenCaptureKit, native input, permission revocation, and crash recovery remain
open.

## See also

- [T2 native capability Spec](../../spec/design/native-browser-computer-use.md)
- [Desktop tier rules](../../desktop/AGENTS.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: partial
