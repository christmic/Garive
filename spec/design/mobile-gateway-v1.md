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

## Stable edge failures

| HTTP | Code | Meaning |
|---:|---|---|
| 400 | `invalid_json`, `invalid_pairing_request`, `invalid_request` | Correct the request. |
| 401 | `pairing_rejected`, `authentication_required` | Pair or re-pair. |
| 404 | `route_not_admitted`, `device_not_found` | Nothing was routed. |
| 413 | `request_too_large` | Reduce input. |
| 503 | `entropy_unavailable`, `runtime_unavailable` | Reads may back off; mutations retain identity. |

Host errors/success bodies pass through. Edge errors never contain request
values or upstream exception text.

## Operations and verification

TLS terminates in this process for this slice, certificate DNS matches the
public origin, and Runtime remains loopback. A secret manager supplies pairing
and admin secrets.

Tests cover invalid composition, strict single-use pairing, credential
stripping, path/body/query preservation, expiry, self/admin revocation,
traversal/method rejection, bounds, stable errors, and the race detector.
A-MOBILE-R additionally requires physical iOS and Android evidence through a
disposable certificate, Gateway, and Runtime.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted; single-process implementation verified
