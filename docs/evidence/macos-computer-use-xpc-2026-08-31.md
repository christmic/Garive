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
duplicate native objects, requires unique focus, exposes portable press,
non-secure set-value and focused native-text support, and never reads
secure-text values.

The observation binding retains its parent-before-child AX objects. Semantic
action preflight rebuilds the exact bounded projection, compares it with the
frozen snapshot, and verifies that the selected node is the same AX object.
The binding is consumed once immediately before `AXUIElementPerformAction` or
`AXUIElementSetAttributeValue`. Permission revocation, semantic drift, object
replacement, unsupported actions and secure fields all fail before dispatch.
A fresh observation is required after success; failure to obtain trustworthy
post-dispatch evidence is classified uncertain and cannot trigger replay.

Native keyboard input extends that same one-shot boundary. Before preparing an
event it additionally requires the signed application to remain frontmost, the
retained AX window to be its exact focused window, and the snapshot-local node
to remain the unique focus. Unicode typing is limited to a focused,
non-secure, settable `AXTextField` or `AXTextArea`. The system dispatcher builds
events with a private CoreGraphics source and posts only to the verified PID
through `CGEvent.postToPid`; it does not use the clipboard or global session
tap. Portable keys are a closed 14-value set backed by SDK virtual-key
constants. Events are prepared before the observation binding is consumed and
posted only after that atomic one-shot transition.

The native integration test obtains the actual Swift test process's designated
requirement from Security framework, installs it on an anonymous listener,
connects through `NSXPCConnection`, validates the peer facts, and completes an
exported-object `ping` reply. Pure negative cases cover user/session mismatch,
invalid PID/session and malformed or `always` requirements.

Latest result: 26 Swift Testing tests passed, including four permission-posture
argument cases, one real XPC round trip and real current-process Security/
`proc_pidinfo` resolution. AX cases cover prompt-free denial before inspection,
exact-window revalidation, cross-observer rejection, node/text limits, cyclic
graphs, semantic projection and secure-value redaction. Negative cases also
cover semantic drift before action, secure set-value rejection, permission
revocation before native reinspection, one-shot binding consumption, bounded
set-value rejection, a wrong signer, unavailable PID, forged process start,
user/session mismatch and invalid requirements. Keyboard cases prove bounded
Unicode and portable-key dispatch through an injected boundary, frontmost and
exact-focused-window refusal before event preparation, and real SDK event
construction for all 14 keys without posting during preflight. No permission
prompt, screen capture, real application input injection, environment read, or
Engine/Ledger access occurs.

## Open evidence

This package gate is not a packaged-service claim. The generated Rust/Swift IDL,
separately signed service/backend rejection matrix, hardened-runtime bundle,
live permission-granted AX/keyboard fixture coverage, ScreenCaptureKit,
scroll/pointer input, permission revocation, and crash recovery remain open.

## See also

- [T2 native capability Spec](../../spec/design/native-browser-computer-use.md)
- [Desktop tier rules](../../desktop/AGENTS.md)

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: partial
