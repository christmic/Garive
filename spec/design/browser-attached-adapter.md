# T2-ATTACHED — Explicit-tab browser attachment

## Status

Accepted implementation contract. This package adds Attached Browser transport
without changing Engine tool schemas, Runtime authority or the Managed CDP
adapter. It controls only a tab selected by an immediate user extension action.

## Package ownership

```text
browser/extension/chromium/       Manifest V3 extension and debugger transport
adapters/browser-attached/        bounded Native Messaging wire/protocol types
runtime/replica/                  grant admission, policy and durable effects
```

The extension owns Chrome APIs and private `tabId`/debugger session routing.
The adapter owns framing and exact protocol validation only. Runtime owns
opaque grant/page identities, origin policy, snapshot bindings and revocation.
Core sees only normal governed T2 observations and receipts. The Managed CDP
adapter must not import this package, and the extension must not import Runtime
or Engine code.

## Permission posture

The extension manifest declares only `activeTab`, `debugger` and
`nativeMessaging`; it declares no host patterns, content scripts, externally
connectable pages, tab enumeration permission, storage, downloads, clipboard,
webRequest or scripting permission. Installation alone grants no page. The
user must press the extension action in the foreground tab for each grant.

The service worker reads only the active tab returned for that action, attaches
`chrome.debugger` to that exact `tabId`, and opens one long-lived
`runtime.connectNative` port. It never calls a targets/tabs enumeration API and
never auto-attaches related targets. Popups and cross-process frames require a
new explicit Runtime admission delivered by the same parent-bound rules as T2.

The Native Messaging host manifest contains one exact extension origin; no
wildcards are valid. The host is constructed with the expected caller origin,
maximum frame size, protocol revision and local Runtime broker supplied by the
Garive application. It reads no environment variables and discovers no socket,
profile, browser, extension or credential.

## Wire boundary

Native Messaging frames use one native-endian unsigned 32-bit byte length and
one UTF-8 JSON object. Garive caps both directions at 1 MiB, rejects zero,
truncated, trailing, non-object, duplicate-key and non-canonical protocol
values, and writes nothing except framed protocol bytes to stdout. Diagnostics
go to stderr without page content.

Every message contains `revision`, `connection_id`, positive `sequence` and one
closed `kind`. Stable kinds are:

```text
host.challenge
extension.grant
host.command
extension.result
extension.event
extension.revoked
host.detach
```

Unknown kinds or fields fail closed. Sequences are strictly increasing in each
direction. Requests bind one exact response; events cannot satisfy a request.
Content, parameters and results remain bounded JSON values and are never logged.

## Grant handshake

Runtime creates an unpredictable single-use challenge and opaque
`BrowserSessionId`/`BrowserPageId`. `host.challenge` binds their digests,
expected canonical origin, expiry and the closed CDP method catalogue. The
extension action attaches the exact active tab, reads its URL only after the
user gesture, canonicalizes its HTTP(S) origin and returns `extension.grant`
with the challenge, a connection-local opaque tab handle and debugger protocol
version. Raw `tabId`, window id and Chrome target/session ids never cross the
Native Messaging boundary.

Runtime admits only an unexpired exact challenge whose origin agrees with its
policy. A mismatch sends `host.detach`; the extension detaches before returning
the terminal result. Challenges and tab handles cannot be replayed across a
connection or after revocation.

## CDP relay

`host.command` carries one command id, opaque tab handle, admitted method and
bounded params. The extension maps the handle to its private `tabId`, calls
`chrome.debugger.sendCommand`, and returns exactly one result or stable error.
It forwards only events from that same debuggee and only admitted methods.
Runtime reuses the semantic mapping and preflight/receipt rules already frozen
for Browser T2; the relay grants no new CDP method.

Download commands and browser-global Target discovery are absent. Attached
navigation that becomes a download is uncertain unless an equivalent
per-grant native denial can be proved. Cookies, storage, Network bodies,
arbitrary Runtime evaluation and ambient targets remain unavailable.

## Revocation and recovery

Extension action toggle, debugger detach, tab close/navigation outside the
grant, Native Messaging disconnect, extension reload and browser exit emit or
imply one revocation. Runtime immediately poisons the page port, clears private
bindings and commits attachment loss. After native dispatch, loss remains
uncertain. The extension detaches the debugger best-effort and forgets its
handle; it stores no grant across service-worker or browser restart.

Recovery requires another user action and a new challenge/connection/page
admission. Neither side replays commands or revives an old grant.

## Acceptance

- Rust framing/protocol tests cover bounds, duplicate keys, truncation,
  sequence, request/result/event separation, origin/challenge replay and
  content-free Debug output;
- extension tests use mocked Chrome APIs to prove action-only active-tab attach,
  exact debugger routing, no enumeration, detach/revocation and no persistence;
- a native-host stdio fixture proves exact caller-origin admission and framed
  full-duplex exchange without stdout contamination;
- Runtime tests prove grant admission, origin mismatch detach, pre/post-dispatch
  loss classification and fresh-grant recovery;
- a temporary unpacked extension plus registered test host proves one real
  foreground-tab observation/action/revocation journey where locally possible.

## Sources

- Chrome Extensions `activeTab`, `chrome.debugger` and Native Messaging official
  references, reviewed 2026-08-31.

