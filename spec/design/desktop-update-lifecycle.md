# Desktop Update Lifecycle

> This accepted contract defines Garive Desktop update discovery, signature
> verification, installation, restart, refusal, and release configuration. It
> drives the macOS Tauri updater slice without claiming a publishable channel.

## Audience

Engineers changing the Desktop backend, React Settings surface, release
pipeline, or M80–M83 evidence.

## Why

An updater is an external effect against the installed application. A button
that downloads bytes is not sufficient: Garive must distinguish an unconfigured
build, no newer release, a verified candidate, a failed verification, an
installed update awaiting restart, and an unknown installation outcome. The
current repository has no production channel or signing key, so the default
build must remain truthfully unavailable while the implementation and release
admission path land.

This contract refines the security, operations, and acceptance rules in
[desktop-work-product.md](desktop-work-product.md) and the M80–M83 capture
requirements in
[desktop-visual-manual-evidence.md](desktop-visual-manual-evidence.md).

## Quick start

The default development configuration contains no update endpoint or public
key. It must build and report `updater=false`:

```sh
pnpm --dir desktop/frontend test
cargo test -p garive-desktop
```

A release runner supplies a Tauri configuration overlay containing the public
channel values and supplies the signing private key only through its protected
environment. No private key or private endpoint enters Git, application
storage, frontend state, logs, or evidence.

## Ownership

| Concern | Owner |
|---|---|
| current application version | signed Tauri bundle metadata |
| channel endpoint and verification public key | protected release configuration overlay |
| signing private key | protected release runner environment |
| HTTPS fetch, archive signature verification, install | Tauri updater 2.10.1 |
| downgrade policy and UI lifecycle | Desktop update client |
| durable Runtime/config schema migration | owning Runtime/config module |
| restart | existing closed Desktop restart command |
| release manifest and artifact retention | release pipeline |

The frontend receives version strings, bounded byte progress, and stable error
codes. It never receives an endpoint, public key, signature, archive bytes,
filesystem path, installer command, or private release metadata.

## Configuration admission

The updater capability is available only when the merged Tauri configuration
contains one `plugins.updater` object that satisfies every rule below.

| Field | Rule |
|---|---|
| `endpoints` | one or two public HTTPS URLs; no credentials, fragments, loopback, IP literals, or insecure transport |
| `pubkey` | non-empty Minisign public key, at most 16 KiB |
| `dangerousInsecureTransportProtocol` | absent or `false` |
| `dangerousAcceptInvalidCerts` | absent or `false` |
| `dangerousAcceptInvalidHostnames` | absent or `false` |
| `bundle.createUpdaterArtifacts` | `true` in the release overlay |

Missing or rejected configuration leaves `updater=false`; it does not prevent
the app from launching. The release verifier fails if a public candidate does
not prove the same endpoint/public-key digest used to create its signed update
manifest.

## Product state

`DesktopUpdateState` is a closed union:

| State | Required values | User action |
|---|---|---|
| `unavailable` | current version | none; explains that this build has no configured channel |
| `idle` | current version | Check for updates |
| `checking` | current version | none |
| `current` | current version | Check again |
| `available` | current and newer version | Download update |
| `downloading` | versions, received bytes, optional total bytes | none |
| `ready_to_install` | versions | Install verified update |
| `installing` | versions | none |
| `restart_required` | installed version | Restart Garive |
| `refused` | current version, stable reason | Check again |
| `failed` | current version, stable reason | Retry check; the running app remains usable |

Allowed transitions are exact:

```text
unavailable
idle -> checking -> current | available | refused | failed
current | refused | failed -> checking
available -> downloading -> ready_to_install | refused | failed
ready_to_install -> installing -> restart_required | failed
restart_required -> process restart
```

Navigation, sleep, wake, and a second click never start a second check,
download, or install. Leaving Settings keeps the operation state; it does not
cancel or duplicate an external effect.

## Candidate validation

- Checks always set `allowDowngrades=false`.
- Versions must be stable SemVer values. Pre-release candidates are refused on
  the stable channel.
- The candidate must compare strictly greater than the signed bundle version.
- Release notes and remote HTML are not rendered in the WebView.
- Download progress uses finite non-negative integers. A declared total must be
  positive and never smaller than received bytes.
- A completed Tauri download is the signature-verification boundary. Only then
  may the UI expose Install.
- Signature, archive, transport, response-shape, or version failures collapse
  to stable localised reasons and never include a URL, path, response body,
  signature, key, or raw exception text.

Tauri owns archive signature verification and cannot disable it. Garive does
not override the version comparator and never enables downgrade. These choices
follow the official Tauri updater contract inspected at version 2.10.1:

- [Updater guide](https://v2.tauri.app/plugin/updater/)
- [`plugins/updater/src/updater.rs`](https://github.com/tauri-apps/plugins-workspace/blob/plugins/updater-v2.10.1/plugins/updater/src/updater.rs)
- [`plugins/updater/guest-js/index.ts`](https://github.com/tauri-apps/plugins-workspace/blob/plugins/updater-v2.10.1/plugins/updater/guest-js/index.ts)

## External-effect boundary

Check is read-only and retryable. Download is repeatable only after a terminal
failure because it has not changed the installed application. Install is an
external mutation with three outcomes:

| Observation | Classification | Recovery |
|---|---|---|
| install resolves | committed | expose restart; do not install again |
| install rejects before replacement | failed | preserve current version and allow a fresh check |
| process/window disappears or outcome cannot be observed | unknown | do not retry automatically; on next launch compare signed bundle version with the pending target |

Before install, the client writes a bounded pending record containing only
schema version, current version, target version, and phase. On launch it may
clear the record when the signed bundle equals the target, classify the old
version as not installed, or show reconciliation required. It never treats an
unpaired call as proof that installation did not occur.

Application data is not rolled back with the `.app`. Before a release enables
installation, every Runtime and configuration migration between retained
versions must prove forward compatibility or an explicit backup/restore path.
Downgrade is a separately authorized operator procedure and is never offered by
the normal update UI.

## UI and accessibility

Settings contains one Update card after Language and before Runtime. It shows
the exact current version in every state and the target version only after an
admitted check. One primary action is visible at a time. Progress uses a native
`progressbar` value when total bytes are known and an indeterminate text state
otherwise. Status changes use a polite live region; refusal and failure use an
alert without moving focus. Restart is explicit and never occurs while a
Session mutation is pending.

English and Simplified Chinese are complete. Pseudolocale covers the longest
status and error strings. At 200% zoom the action, version, progress, and error
remain reachable without horizontal scrolling.

## Stable failures

| Code | Meaning |
|---|---|
| `update_not_configured` | endpoint/public-key configuration is absent or rejected |
| `update_invalid_version` | current or candidate version is not admitted stable SemVer |
| `update_not_newer` | manifest candidate is equal, older, or pre-release |
| `update_check_failed` | endpoint, TLS, timeout, status, or manifest check failed |
| `update_download_failed` | archive download failed before verification |
| `update_signature_invalid` | archive signature verification failed |
| `update_install_failed` | updater reported a known pre-commit install failure |
| `update_outcome_unknown` | install began but a terminal result is unavailable |
| `update_busy` | another update effect is already active |

The UI may group codes into concise copy, but tests retain the exact code. Raw
plugin errors are never displayed or persisted.

## Release evidence

M82 requires a signed, notarized older app installed on a clean supported Mac,
a valid signed update archive, visible download/verified/install/restart states,
the new signed bundle identity after restart, and unchanged durable Sessions,
configuration, and Workspace authorization receipts.

M83 requires separate invalid-signature and downgrade manifests. Both must
show refusal, leave the installed version executable, preserve data, and avoid
an automatic retry. A unit test or visual fixture cannot replace these package
tests.

The release set retains the previous signed/notarized installer, update archive,
manifest, signature, SBOM, license inventory, checksums, migration evidence,
and rollback instructions for every supported rollback edge.

## Acceptance

1. Default build boots with `updater=false` and no network request.
2. Exact runtime configuration tests reject every insecure or incomplete form.
3. State-machine tests cover every transition, duplicate action, malformed
   version/progress, signature refusal, install failure, and unknown outcome.
4. Tauri capability parity admits only check, download, install, and the
   existing restart command to the `main` window.
5. Production frontend build and full Desktop Rust/frontend tests pass.
6. A signed release remains blocked until M82 and M83 run on a clean Mac.

## See also

- [desktop-work-product.md](desktop-work-product.md) — complete Desktop product contract.
- [desktop-visual-manual-evidence.md](desktop-visual-manual-evidence.md) — screenshot evidence matrix.
- [../../desktop/release/README.md](../../desktop/release/README.md) — package gates and current release boundary.
- [../../desktop/AGENTS.md](../../desktop/AGENTS.md) — Desktop ownership and verification rules.

## Meta

- Owner: Garive Desktop
- Last reviewed: 2026-08-30
- Status: accepted
