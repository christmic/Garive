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

Application identity is resolved independently from the XPC caller. The
verifier accepts an explicit Security requirement and PID, freezes
`proc_pidinfo` start seconds/microseconds, validates the dynamic `SecCode`, and
records its signing identifier and CodeDirectory hash. Code validity and
process-start evidence are checked again after collection. Revalidation
repeats the full resolver and requires exact identity equality.

AX observation begins with the prompt-free permission inspector and then
revalidates that signed application instance. Because the public SDK exposes
no AX window number or AX-to-CoreGraphics window conversion, the observer
retains the exact enumerated `AXUIElement`. It requires that same object in a
fresh `AXWindows` enumeration before and after reading. A binding is scoped to
one observer instance; replacement, disappearance and cross-observer use fail
as a changed target.

The semantic reader uses public AX attributes/actions only. It iteratively
builds and projects a bounded parent-before-child tree, rejects cycles and
duplicate native objects, requires unique focus, exposes only portable press
and non-secure set-value support, and never reads secure-text values.

The native integration test obtains the actual Swift test process's designated
requirement from Security framework, installs it on an anonymous listener,
connects through `NSXPCConnection`, validates the peer facts, and completes an
exported-object `ping` reply. Pure negative cases cover user/session mismatch,
invalid PID/session and malformed or `always` requirements.

Latest result: 14 Swift Testing tests passed, including four permission-posture
argument cases, one real XPC round trip and real current-process Security/
`proc_pidinfo` resolution. AX cases cover prompt-free denial before inspection,
exact-window revalidation, cross-observer rejection, node/text limits, cyclic
graphs, semantic projection and secure-value redaction. Negative cases also
cover a wrong signer, unavailable PID, forged process start, user/session
mismatch and invalid requirements. No permission prompt, screen capture, input
dispatch, environment read, or Engine/Ledger access occurs.

## Open evidence

This package gate is not a packaged-service claim. The generated Rust/Swift IDL,
separately signed service/backend rejection matrix, hardened-runtime bundle,
AX actions and live permission-granted fixture coverage,
ScreenCaptureKit, native input, permission revocation, and crash recovery remain
open.

## See also

- [T2 native capability Spec](../../spec/design/native-browser-computer-use.md)
- [Desktop tier rules](../../desktop/AGENTS.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: partial
