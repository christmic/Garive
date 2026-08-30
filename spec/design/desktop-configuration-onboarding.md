# A-DESKTOP-C2 — Secure Desktop configuration onboarding

> This Spec adds a one-way, backend-governed setup and reconfiguration flow to
> the Desktop app. The frontend may submit bounded user choices and a credential
> once; only the backend validates profiles, stores secrets, writes configuration,
> and constructs Runtime. No secret or effective configuration is readable back.

## Audience

Desktop backend/frontend and release engineers replacing manual JSON/keychain
provisioning without moving configuration authority into React.

## Why

A-DESKTOP-C proves strict startup from a pre-provisioned document and OS
credential reference, but forbids configuration IPC. That is safe yet not a
usable first-run product. C2 adds a narrow write-only setup channel with staged
recovery. It does not expose a generic settings object, environment fallback,
credential read API, or Provider construction in the frontend.

## Relationship to C1

C2 amends the C1 prohibition only for the exact commands below. Normal Agent
IPC still carries no endpoint, model, profile, header, credential, or database
path. C1 remains the stored document, resolver, registry, startup, and Runtime
construction contract.

The backend exposes a redacted public setup catalogue generated from installed
profile constructors:

```text
DesktopSetupCatalogueV1 {
  schema_version: 1
  profiles: SetupProfileV1[]
  limits: SetupInputLimitsV1
}
SetupProfileV1 {
  profile_id, display_name_key
  endpoint_mode: fixed | optional_override
  model_mode: exact_id
  credential_label_key
  supported_capabilities[]
}
```

Display keys are localizable presentation metadata. The stable setup wire has
opaque profile identities and neutral capabilities; it does not branch on a
vendor enum. Hosted special capabilities require their own admitted Specs.

## Typed IPC

```text
get_setup_state() -> NotConfigured | Configured {restart_required} |
                     InvalidConfiguration {code} | SetupRecovering
get_setup_catalogue() -> DesktopSetupCatalogueV1
prepare_setup(input: DesktopSetupInputV1) -> DesktopSetupPlanV1
commit_setup(plan_digest, credential) -> DesktopSetupReceiptV1
cancel_setup(plan_digest) -> Cancelled | AlreadyCommitted
```

`DesktopSetupInputV1` contains exact `profile_id`, optional endpoint override,
model target/model/deployment identities, installed Agent identity, non-secret
Runtime bounds/policies, and a caller nonce. It contains no credential.
`prepare_setup` validates all non-secret values, resolves the immutable profile
constructor, generates a fresh opaque credential reference and setup identity,
and returns a bounded redacted summary plus SHA-256/JCS plan digest.

`commit_setup` accepts the exact unexpired plan and one bounded secret byte
buffer. The IPC layer marks the credential parameter sensitive: Debug,
serialization errors, tracing, analytics, panic hooks, and frontend state never
retain or echo it. The response contains setup identity, plan digest,
configuration revision, and `restart_required`; it contains no input values or
credential reference.

## Staged commit and recovery

Backend commits in this order:

1. Revalidate plan identity, digest, expiry, current configuration revision,
   installed registry revision, and all bounds.
2. Store the new credential under the fresh reference in the OS credential
   service. Never overwrite the current reference.
3. Write and fsync a new strict C1 document to a same-directory temporary file.
4. Atomically rename it to `desktop-v1.json` and fsync the directory.
5. Persist a non-secret setup receipt and mark the new revision committed.
6. On a later successful Runtime start, delete an obsolete credential reference
   best-effort; failure becomes bounded cleanup work, not rollback.

A crash before rename leaves the old configuration authoritative and startup
removes an uncommitted new credential/temp file from the non-secret recovery
journal. A crash after rename treats the new document as committed; startup
validates it, repairs the receipt if needed, and never restores old config from
memory. Recovery is bounded and completes before Agent IPC admission.

V1 does not hot-swap Runtime. Initial setup or reconfiguration returns
`restart_required`; the app offers an explicit restart action. The current
Runtime remains immutable until process exit.

## Frontend interaction

- First run shows one setup route, not raw JSON.
- Profile and model are explicit selections/text under catalogue limits;
  optional endpoint override is hidden behind an advanced disclosure.
- Credential input uses a native secure field, is never copied into preference
  storage, and is cleared on navigation, failure, commit, and unmount.
- The review screen displays only the redacted plan summary. Commit requires an
  explicit action; reconfiguration warns that restart is required.
- Invalid stored configuration offers reconfigure and open-diagnostics actions;
  diagnostics contain only stable codes and file identity, never file content.

## Bounds, authority, and failures

All text/count/secret bytes, plan count, plan lifetime, recovery entries, and
diagnostics have non-zero backend limits. Only the main Desktop window under
the installed Tauri capability may call setup IPC. Web/mobile/XPC extensions
cannot call it.

| Code | Meaning |
|---|---|
| `setup_not_allowed` | Caller/window/capability lacks setup authority. |
| `setup_input_invalid` | Unknown field, profile, model, endpoint, policy, or bound. |
| `setup_plan_stale` | Plan expired or registry/configuration revision changed. |
| `setup_plan_conflict` | Digest/identity was reused with different semantics. |
| `setup_credential_rejected` | Secret is empty, oversized, or OS storage refused it. |
| `setup_persistence_failed` | Document/receipt could not commit durably. |
| `setup_recovery_failed` | Startup cannot safely classify a staged setup. |

Errors and UI messages never reveal whether a submitted credential is valid at
the provider; setup performs no network request or model attempt.

## Acceptance evidence

- strict shared fixture covers catalogue, every input/profile mode, plan digest,
  bounds, stale/conflict/cancel, and redacted receipts;
- backend tests inject registry, credential store, filesystem crash points,
  clock and identities; every stage restarts into exactly old or new config;
- typed IPC tests prove credential bytes cross only `commit_setup`, are cleared,
  never serialized in result/error/log/debug, and cannot be retrieved;
- configured startup after explicit restart completes one real loopback durable
  Turn with the new revision;
- React tests cover first-run, invalid-config, review, commit, restart-required,
  reconfigure, keyboard/focus and secret-field clearing;
- source scans prove no environment loader, generic config read/write IPC,
  frontend secret persistence, Provider construction, or setup network call.

## See also

- [`desktop-system-configuration.md`](desktop-system-configuration.md) — C1
  strict stored document and backend startup.
- [`client-product-experience.md`](client-product-experience.md) — A-UX1 shell
  state and presentation rules.
- [`vendor-connection-profiles.md`](vendor-connection-profiles.md) — installed
  explicit profile constructors and secret values.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
