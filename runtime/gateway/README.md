# Garive mobile Gateway

This service is the authenticated HTTPS edge for the native mobile clients. It
keeps the Runtime on loopback, admits only the versioned H1/H2/H3 mobile route
family, streams SSE without buffering, and never becomes a second Session
store.

## Security model

- The listener requires TLS 1.3 and an operator-provided certificate.
- The upstream Runtime must be a bare loopback HTTP origin.
- A bootstrap pairing code is single-use for this process lifetime.
- Pairing creates a random per-device grant stored only as a SHA-256 digest.
- Grants expire after 30 days and an administrator can revoke a device.
- Incoming credentials are removed before Runtime forwarding.
- Redirects, arbitrary paths, cookies, and general reverse-proxy behavior are
  absent by construction.

The initial slice retains grants in memory. Restart intentionally revokes every
device and creates a new pairing ceremony. A durable multi-instance grant store
can replace this map without changing the mobile or Runtime routes.

## Run

Build with the Go version declared in `go.mod`, then set:

```text
GARIVE_RUNTIME_ORIGIN=http://127.0.0.1:4317
GARIVE_GATEWAY_LISTEN=:8443
GARIVE_TLS_CERT=/absolute/path/to/fullchain.pem
GARIVE_TLS_KEY=/absolute/path/to/private-key.pem
GARIVE_PAIRING_CODE=<single-use operator code>
GARIVE_ADMIN_TOKEN=<at least 20 random characters>
GARIVE_WAKE_POLL_INTERVAL=3s
```

For iOS delivery also set `GARIVE_APNS_TEAM_ID`, `GARIVE_APNS_KEY_ID`,
`GARIVE_APNS_TOPIC`, and `GARIVE_APNS_KEY_FILE`; set
`GARIVE_APNS_SANDBOX=true` only for development device builds. For Android set
`GARIVE_FCM_CREDENTIALS` to a service-account JSON file with FCM send authority.
Provider secrets stay outside the repository and command-line arguments.

Start `go run ./cmd/garive-gateway`. Production DNS and the certificate must
match the HTTPS service origin entered on mobile. Do not expose the Runtime
port outside loopback.

## Pairing request

`POST /v1/mobile/pair` accepts one strict JSON object:

```json
{
  "api_version": "v1",
  "code": "single-use-code",
  "device_name": "Chris's iPhone",
  "platform": "ios",
  "device_public_key": "base64url-without-padding"
}
```

The success response contains `access_grant`, `device_id`, and `expires_at`.
The native app stores the grant in Keychain or Android Keystore and uses it as
a Bearer credential. Unknown fields and malformed keys fail closed.

## Revocation

An operator revokes a device with:

```text
POST /v1/mobile/devices/{device_id}:revoke
Authorization: Bearer {GARIVE_ADMIN_TOKEN}
```

The route returns `204` and subsequent requests with that grant return the
stable `authentication_required` error. Operator tooling must keep the admin
token outside command history and application logs.

Native sign-out first clears protected local storage, then best-effort calls
`POST /v1/mobile/grants/self:revoke` with the captured prior grant. This also
returns `204`; network failure never restores the local credential.

## Verify

Run `go test -race ./...`. Tests cover strict one-time pairing, authorization,
expiry, revocation, route admission, header stripping, body preservation, and
loopback-only Runtime composition, plus APNs/FCM payload privacy, FID targeting,
strict wake resolution, automatic durable-transition relay and deduplication.
