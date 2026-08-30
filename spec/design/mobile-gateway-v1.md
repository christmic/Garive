# G0-R — Authenticated mobile Gateway v1

> Defines the narrow HTTPS edge that pairs a native device, issues a
> revocable grant, and forwards only admitted Garive Host routes to a
> loopback Runtime.

## Boundary

The Gateway owns edge TLS, device authentication, route admission, request
bounds, revocation, and transparent streaming. Runtime remains the sole owner
of Sessions, command idempotency, durable positions, Agent execution, recovery,
and response bodies.

The first deployment slice is one process beside one Runtime. Its grant store
is intentionally in memory: a Gateway restart revokes all devices and requires
a new pairing ceremony. This is fail-closed behavior, not durable availability.
A later shared grant adapter may replace it without changing these routes.

## Composition

```text
native app -- TLS 1.3 --> Gateway -- loopback HTTP --> LiveHostServer
```

The Gateway refuses startup unless the Runtime is a bare loopback HTTP origin,
TLS certificate and key are configured, the one-time code is 6–128 characters,
and the administrative token has at least 20 characters. It never enables a
remote Runtime listener, follows redirects, discovers a proxy, forwards
cookies, accepts URL credentials, or logs tokens, bodies, paths, or Host IDs.

## Pairing

`POST /v1/mobile/pair` is the only unauthenticated route. It accepts exactly:

```json
{
  "api_version": "v1",
  "code": "single-use operator code",
  "device_name": "public device label",
  "platform": "ios",
  "device_public_key": "base64url-without-padding"
}
```

`platform` is `ios` or `android`. `device_name` is 1–100 characters. The
decoded EC public key is 32–2048 bytes. Unknown fields, duplicate/trailing JSON,
oversized bodies, or wrong versions reject the request. Code comparison is
constant-time and a valid code is consumed once.

The server obtains 32 random grant bytes and 16 random device-identity bytes
from the OS CSPRNG. Only `SHA-256(grant)` is retained. Success is `201`:

```json
{
  "api_version": "v1",
  "access_grant": "opaque base64url",
  "device_id": "opaque base64url",
  "expires_at": "RFC3339 UTC"
}
```

Native code creates its device key first, retains the private key in
Keychain/Android Keystore, and stores the grant only in protected storage. The
current grant is device-scoped and independently revocable; per-request proof
of possession is reserved for a compatible protocol revision.

An expiring `garive://pair` link carries `origin`, `code`, `exp`, and `name`
exactly once. Native clients require `now < exp <= now + 600 seconds`, show the
service label and HTTPS origin, then require confirmation. A QR code contains
this link and uses the OS camera/link handoff, so the app needs no camera access.

## Authentication and admission

Every Host request carries one `Authorization: Bearer <grant>`. The Gateway
hashes it and requires a non-revoked, unexpired grant. Every failure is the same
`401 {"code":"authentication_required"}`. Responses add `Cache-Control:
no-store` and `X-Content-Type-Options: nosniff`.

Only these method families route:

```text
GET  /v1/agent-definitions
GET  /v1/sessions
GET  /v1/sessions/{id}
GET  /v1/sessions/{id}/timeline
GET  /v1/sessions/{id}/events
POST /v1/sessions
POST /v1/sessions/{id}/turns
POST /v1/turns/{id}:cancel
POST /v1/turns/{id}:continue
```

IDs contain only ASCII letters, digits, `_`, and `-`. Other paths/methods
return `404 route_not_admitted` before upstream I/O. Query validation remains
with Host.

The proxy creates a new request and forwards only `Accept`, `Content-Type`,
`Idempotency-Key`, and `Last-Event-ID`; never Authorization. It preserves the
method, admitted path, query, bounded body, Host status, `Content-Type`, and
`Retry-After`. SSE is copied incrementally and flushed. Gateway EOF is not a
terminal. Upstream failure is `503 runtime_unavailable`; mutations never retry.

## Revocation

After clearing local storage, a client best-effort calls:

```text
POST /v1/mobile/grants/self:revoke
Authorization: Bearer <captured prior grant>
```

An active grant becomes revoked and returns `204`; failure cannot restore local
credentials. An operator may use `POST /v1/mobile/devices/{device_id}:revoke`
with the admin Bearer token. The admin token is digest-compared in constant time
and is never accepted on Host routes.

## Push registration and automatic wake relay

An authenticated device registers exactly one provider address with:

```text
POST /v1/mobile/push/registrations
{"api_version":"v1","transport":"apns|fcm","registration_id":"..."}
```

The grant's paired platform fixes the transport: iOS accepts only APNs and
Android only FCM. `DELETE /v1/mobile/push/registrations/self` removes it before
sign-out revocation. Provider addresses remain in volatile Gateway memory and
never enter Runtime, logs, diagnostics, or notification content.

Runtime exposes the loopback-only, non-proxied
`GET /internal/mobile/wake-snapshot?limit=N[&before=ID]`. It projects only
Session ID, latest durable position, and an optional `attention`, `completed`,
or `failed` wake category. Gateway pages the whole projection, suppresses the
initial historical snapshot, and emits only category transitions. Runtime
remains the owner of the category; Gateway retains only an ephemeral observed
position/category map and cannot turn a hint into durable truth.

APNs uses ES256 provider authentication and an alert/background payload with
generic lock-screen text. FCM uses service-account OAuth2, Firebase Installation
IDs, and a data message. The provider-specific envelope contains only:

```json
{"schema_version":1,"route_token":"opaque","category":"attention","collapse_key":"attention"}
```

Each device receives its own random 10-minute route token. The app must resolve
it once through `POST /v1/mobile/wake/{token}:resolve` with the same active
grant. Resolution returns only `destination`, `session_id`, and `category`;
the app then refreshes Runtime truth before rendering or authorizing anything.
Provider failure removes the unresolved token and never becomes Agent failure.

## Stable edge failures

| HTTP | Code | Meaning |
|---:|---|---|
| 400 | `invalid_json`, `invalid_pairing_request`, `invalid_push_registration`, `invalid_wake_hint`, `invalid_request` | Correct the request. |
| 401 | `pairing_rejected`, `authentication_required` | Pair or re-pair. |
| 404 | `route_not_admitted`, `device_not_found`, `wake_hint_not_found` | Nothing was routed. |
| 413 | `request_too_large` | Reduce input. |
| 503 | `entropy_unavailable`, `runtime_unavailable`, `push_unavailable` | Reads may back off; mutations retain identity. |

Host errors/success bodies pass through. Edge errors never contain request
values or upstream exception text.

## Operations and verification

TLS terminates in this process for this slice, certificate DNS matches the
public origin, and Runtime remains loopback. A secret manager supplies pairing
and admin secrets.

Tests cover invalid composition, strict single-use pairing, credential
stripping, path/body/query preservation, expiry, self/admin revocation,
provider payload privacy and authentication, strict registration, route-token
binding/expiry/single use, snapshot paging, transition deduplication, startup
history suppression, traversal/method rejection, bounds, stable errors, and
the race detector.
A-MOBILE-R additionally requires physical iOS and Android evidence through a
disposable certificate, Gateway, and Runtime.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted; single-process implementation verified
