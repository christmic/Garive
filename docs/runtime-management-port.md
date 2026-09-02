# Runtime management port

Date: 2026-09-02

Adds an in-process loopback HTTP surface that lets a headless `garive-host`
be configured (Provider profile + endpoint override + model identity +
deployment + agent definition + API key + runtime identity) before any
H1 Session is created, and an equivalent CLI subcommand for ops / manual
recovery. The slice is intentionally narrow — it does **not** replace
SetupService, does **not** migrate the existing keychain layout, and does
**not** expose Tool catalogue / Workspace binding / Agent installation
management.

## Wire contract

All endpoints are bound on the existing `LiveHostServer` listener
(loopback-only, default `127.0.0.1:8787`), under the `/v1/management`
prefix. The HTTP error shape is `{"code": "<stable_wire_code>",
"message": "<stable_wire_code>"}`.

| Method | Path | Body | Response |
| --- | --- | --- | --- |
| GET | `/v1/management/health` | — | `200` `{schema_version, configured, configuration_revision}` |
| GET | `/v1/management/setup` | — | `200` `ManagementConfigRead` or `404 management_not_configured` |
| POST | `/v1/management/setup` | `ManagementCommitBody` (JSON) | `200` `ManagementConfigReceipt` or `4xx management_*` |
| DELETE | `/v1/management/setup` | — | `204` or `404 management_not_configured` |

### `ManagementCommitBody`

```json
{
  "schema_version": 1,
  "profile_id": "openai.responses.v1",
  "endpoint_override": "https://api.openai.com/v1",
  "model_target_id": "gpt-5.6",
  "model_id": "gpt-5.6",
  "deployment_id": "tok9-flash",
  "definition_id": "desktop.agent.v3",
  "api_key": "<redacted on read>",
  "runtime_id": "runtime-<uuid>"
}
```

Unknown fields are rejected by `#[serde(deny_unknown_fields)]`. The DAO
rejects every commit before persisting:

- empty / whitespace / over-512-byte `api_key`
- whitespace / over-256-byte `endpoint_override`
- any `*_id` field whose characters fall outside `[a-zA-Z0-9.-_]`
- empty / over-128-byte `runtime_id`
- `schema_version` ≠ 1

### `ManagementConfigReceipt`

```json
{
  "schema_version": 1,
  "configuration_revision": 1,
  "configuration_digest": "<64 hex>",
  "restart_required": true,
  "receipt_digest": "<64 hex>"
}
```

`restart_required` is always `true` (matches the existing Setup receipt
contract; hot-swap is out of scope). `configuration_digest` is
SHA-256 of the canonicalized envelope (serde_jcs) keyed by the contract
id `garive.management-config.v1`. `receipt_digest` binds revision,
configuration_digest, and `restart_required` for tamper-evidence.

### Stable wire codes (excerpt)

| Code | Status | Trigger |
| --- | --- | --- |
| `management_not_configured` | 404 | GET / DELETE on empty table |
| `management_profile_unknown` | 400 | `profile_id` not in BuiltinDesktopProfileRegistry |
| `management_definition_unknown` | 400 | `definition_id` not in the two built-in agent revisions |
| `management_endpoint_invalid` | 400 | length / character validation on `endpoint_override` |
| `management_api_key_invalid` | 400 | empty / over-cap `api_key` |
| `management_runtime_id_invalid` | 400 | length / character validation on `runtime_id` |
| `management_identifier_invalid` | 400 | length / character validation on any `*_id` field |
| `management_schema_version_unsupported` | 400 | `schema_version` ≠ 1 |
| `management_storage_failed` | 500 | SQLite constraint / I/O |

## Storage

A new singleton table introduced in SQLite schema v9:

```sql
CREATE TABLE runtime_management_config (
    config_id INTEGER PRIMARY KEY NOT NULL CHECK(config_id = 1),
    profile_id TEXT NOT NULL CHECK(length(profile_id) > 0),
    endpoint_override TEXT,
    model_target_id TEXT NOT NULL CHECK(length(model_target_id) > 0),
    model_id TEXT NOT NULL CHECK(length(model_id) > 0),
    deployment_id TEXT NOT NULL CHECK(length(deployment_id) > 0),
    definition_id TEXT NOT NULL CHECK(length(definition_id) > 0),
    api_key TEXT NOT NULL CHECK(length(api_key) > 0),
    runtime_id TEXT NOT NULL CHECK(length(runtime_id) > 0),
    configuration_revision INTEGER NOT NULL CHECK(configuration_revision > 0),
    configuration_digest TEXT NOT NULL CHECK(length(configuration_digest) = 64),
    committed_at TEXT NOT NULL CHECK(length(committed_at) > 0),
    CHECK(endpoint_override IS NULL OR length(endpoint_override) > 0)
) STRICT;
```

`config_id = 1` enforces exactly one row at the schema layer; a second
insert with any other id fails the CHECK. The DAO wraps `INSERT OR
REPLACE` in an `IMMEDIATE` transaction that reads the previous revision
first, increments it, recomputes the digest, and writes the new row in
a single atomic step.

GET responses deliberately omit `api_key`; the `ManagementConfigRead`
type carries no such field. The on-disk SQLite value is plaintext
(see Risks → R1).

## Validator contract

`ManagementValidator` is a pluggable trait. The runtime default is
`AllowAllValidator`, which accepts every well-formed body. The
`garive-host` binary wires `BuiltinManagementValidator`, which only
accepts:

- profiles: `openai.responses.v1`, `anthropic.messages.v1`
- definitions: `desktop.agent.v3`, `desktop.workspace-agent.v3`

Adding a new profile or agent requires a one-line change to
`desktop/backend/src/management_validator.rs` (the IDs are already
referenced as `pub const` in `system_provider.rs` and `desktop_agent.rs`).

Validation order in `commit_setup`:

1. per-field wire validation in `ManagementConfigStore::commit`
2. `ManagementValidator::validate` (Registry allowlist when wired)
3. SQLite `IMMEDIATE` transaction + insert

## CLI subcommands

The `garive-host` binary gains two subcommands alongside the existing
`serve` / `serve-stdin` / `configure`. Both read the write-only
connection credential from stdin, matching the security posture of
`configure`.

```
garive-host setup-management <config-dir> <profile> <endpoint> <target> <model> <definition> <deployment> <runtime-id>
    setup-management reads the write-only connection credential from stdin
    writes the singleton runtime_management_config row (loopback SQLite path)

garive-host clear-management <config-dir>
```

`setup-management` prints the receipt as JSON and exits 0; unknown
profile or definition ids exit 2 with the stable wire code on stderr.

## Acceptance

All four slices pass their unit, integration and end-to-end tests.

### `cargo fmt --check -p garive-runtime -p garive-desktop`

Clean.

### `cargo clippy --workspace --all-targets -- -D warnings`

Clean for `garive-runtime` and `garive-desktop`. Other workspace
members unchanged.

### Library + integration tests

| Suite | Cases | Result |
| --- | --- | --- |
| `runtime::sqlite_ledger` | 7 | ok |
| `runtime::management` | 13 | ok |
| `runtime::live_host_management` | 9 | ok |
| `desktop::management_validator` | 5 | ok |

### Real binary smoke (`target/debug/garive-host`)

```text
$ rm -rf /tmp/garive-smoke && mkdir -p /tmp/garive-smoke
$ echo "sk-test-1234567890" | garive-host setup-management /tmp/garive-smoke \
    openai.responses.v1 https://api.openai.com/v1 gpt-5.6 gpt-5.6 \
    desktop.agent.v3 tok9-flash runtime-7e22bcbe-bfa4-4c8f-a0c3-94e07be8f363
{"configuration_digest":"1d27cddc16cc1be7e3a19423b8bbfd92432e8ce8cb1227bb4fc341fe460d12cb",
 "configuration_revision":1,
 "receipt_digest":"b40bfded9221aca67d6f2b277e183a1f267b0c7819244d52d529dfce8b263179",
 "restart_required":true,"schema_version":1}

$ sqlite3 /tmp/garive-smoke/garive-desktop.db \
    "SELECT profile_id, definition_id, runtime_id, configuration_revision, length(api_key) FROM runtime_management_config;"
openai.responses.v1|desktop.agent.v3|runtime-7e22bcbe-bfa4-4c8f-a0c3-94e07be8f363|1|18

$ echo "sk-test" | garive-host setup-management /tmp/garive-smoke openai.unknown.v9 ...
garive-host: management_validation_failed

$ echo "sk-test-9876543210" | garive-host setup-management /tmp/garive-smoke \
    anthropic.messages.v1 https://api.anthropic.com/v1 claude-opus claude-opus \
    desktop.agent.v3 tok9-flash runtime-aaaa-bbbb-cccc
{"configuration_digest":"42ae0c5480d2974957488216c81c90620a4335dd41098e802fd8cee2193cb0ef",
 "configuration_revision":2,
 "receipt_digest":"b194cbe5bcd18583ec5d39067222744706c11d5a88109f46df4c472099f0b17b",
 "restart_required":true,"schema_version":1}

$ sqlite3 /tmp/garive-smoke/garive-desktop.db "SELECT configuration_revision FROM runtime_management_config;"
2

$ garive-host clear-management /tmp/garive-smoke
$ sqlite3 /tmp/garive-smoke/garive-desktop.db "SELECT count(*) FROM runtime_management_config;"
0
```

The headless loopback path is the same SQLite database file the runtime
will read on restart (`<config-dir>/garive-desktop.db`). The H1 path
itself does not depend on this row — only the next cold start sees it
when constructing `RuntimeModelHttpTransport`.

## Driven runtime (runtime crate headless binary)

A companion slice ships a third Runtime entry point that actually consumes the
committed row at startup, in the **runtime** crate (no Desktop source involved):

```text
runtime/replica/src/bin/garive-headless.rs   <- loopback-only binary
runtime/replica/src/headless.rs              <- runtime::headless helpers
runtime/replica/tests/headless_smoke.rs      <- end-to-end wiring test
```

`garive-headless <config-dir> [127.0.0.1:8787]` reads the singleton row via
`ManagementConfigStore::read_with_credential()`, constructs the `ModelPort`
directly from the provider crates (no `BuiltinDesktopProfileRegistry`
involvement), builds a tool-free headless `RuntimeAgentInstallation`, wires
a `LocalExecutionWorker`, and serves H1 on a loopback listener.

Full wire contract, end-to-end recipe against the local `token9` gateway,
catalogue policy and risks are documented in
[`docs/runtime-headless.md`](runtime-headless.md).

The Desktop path (`FileDesktopConfigurationProvider::load`) is unchanged and
still reads only `desktop-v1.json`; merging the SQLite row into the Desktop
loader remains the separate R3 follow-up.

## Turn-mode wire contract (queue + steer)

The H1 surface offers two complementary turn-mode shapes, both implemented
in the runtime crate:

| Mode     | Path                                                    | Purpose                                                              |
| -------- | ------------------------------------------------------- | -------------------------------------------------------------------- |
| Queue    | `POST /v1/sessions/:session_id/turns`                   | Submit a new Turn; rejected with `session_busy` while one is Open.   |
| Steer    | `POST /v1/sessions/:session_id/turns/:turn_id/steer`    | Inject new user input into the next derive iteration of an Open Turn. |

Queue mode is the only path that creates a new Turn. Steer mode is purely
ledger-driven: it commits a `turn.steered` fact under the targeted `turn_id`,
position-ordered naturally with whatever `plan.*` events the worker emits
around it. No abort, no in-memory inbox.

### Steer mode

Request body (`deny_unknown_fields`):

```json
{ "text": "additional context for the running Turn" }
```

Success (`200 OK`):

```json
{
  "session_id": "<sid>",
  "turn_id":    "<tid>",
  "execution_id": "<eid-or-null>",
  "committed_position": 7
}
```

Failure wire codes:

| Status | code                    | When                                                       |
| ------ | ----------------------- | ---------------------------------------------------------- |
| `400`  | `invalid_request`       | body missing, malformed, or exceeds `max_command_bytes`.   |
| `404`  | `not_found`             | session or turn does not exist.                            |
| `409`  | `command_conflict`      | same idempotency-key replay but the inline text drifted.  |
| `412`  | `precondition_failed`   | the targeted Turn is no longer Open (Suspended/Completed/Stopped/Failed). |

### Ordering guarantee under one `turn_id`

Steer commits share the same `turn_id` as the active Turn. Because every
fact is assigned a strictly increasing Session-local position at commit
time, a sequence of steers interleaves with the worker's own facts in
the order the runtime receives them. The next derive iteration of the
worker observes the new fact at the start of its scan and surfaces the
inline text as a user message on the next model call.

Replay safety: a steer command with the same `Idempotency-Key` is detected
before planning; the original `TurnCommandResponse` is returned without
writing a second fact. A different `Idempotency-Key` against the same
Open Turn produces a fresh `turn.steered` fact.

## Risks (carried verbatim from the design plan)

### R1 — API key plaintext in SQLite (user-chosen)

The user explicitly chose "API key 明文进 SQLite" over the existing
keychain path. SQLite file `garive-desktop.db` is unencrypted; any
process with read permission on the loopback user's home directory can
extract the API key. The rest of the project continues to use
keychain; this slice is the documented exception.

Rolling this back later is a migration:

1. Add a new SQLite column / table tracking opaque OS credential refs.
2. Read the existing row, push the key to the keychain service
   `com.garive.desktop`, replace the column with the reference.
3. Update `BuiltinManagementValidator`'s callers to resolve the
   reference via `SystemDesktopSecretResolver` before constructing the
   transport.

### R2 — No auth on the management port (user-chosen)

Same posture as the existing H1 routes: loopback-only binding, no
extra token. A local user with the same UID can drive the port. Adding
Bearer-token / mTLS / Unix-socket is out of scope.

### R3 — SetupService JSON path is untouched

Tauri Desktop still writes `desktop-v1.json` and pushes the key to the
keychain via `DesktopSetupService`. `garive-host` writes the singleton
row directly through `ManagementConfigStore`. After this slice, two
parallel configuration sources exist:

- `<config-dir>/desktop-v1.json` + keychain (Tauri main process)
- `<config-dir>/garive-desktop.db::runtime_management_config`
  (`garive-host` CLI / management port)

The Tauri `FileDesktopConfigurationProvider::load` does not read the
SQLite row. Configuring through `garive-host setup-management` is
therefore valid only on the headless binary path; the Tauri Desktop UI
must still complete Setup flow. Document this constraint in any UI
that ever bridges the two sources.

### R4 — Hot-swap disabled

Every receipt advertises `restart_required: true`. The runtime does not
swap a committed configuration in place; it requires a process restart,
matching the existing Setup receipt contract.

### R5 — Digest includes the credential

`configuration_digest` is SHA-256 of the canonicalized commit envelope,
which intentionally includes `api_key`. Restart safety: when the
process reopens the SQLite row, it can verify digest equality before
constructing the transport. A drifted `api_key` (different byte-for-byte
content) yields a different digest; mismatched rev or runtime_id fail
explicitly rather than silently reusing the prior key.

## Out of scope (deferred)

- Tool catalogue mutation
- Agent definition CRUD beyond the two built-ins
- Workspace binding management
- SetupService migration to SQLite-backed storage
- Bearer-token / mTLS / Unix-socket auth
- Hot-swap of committed configuration
- Provider profile registration from external sources
