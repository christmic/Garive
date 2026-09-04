# Runtime headless driver

Date: 2026-09-02

Adds a third Runtime entry point: `garive-headless`, a loopback-only binary
that drives H1 sessions entirely against the SQLite `runtime_management_config`
row committed by the management port. The slice is intentionally narrow — it
does **not** replace `DesktopHost`, does **not** read `desktop-v1.json`, and
does **not** import anything from `garive-desktop`.

This doc covers the recipe and the wire shape; the design rationale lives in
the archived plan under `~/.claude/plans/pure-noodling-crane.md`.

## Wire contract

The `garive-headless` binary serves the **same H1 surface** as
`garive-host serve` (`LiveHostServer` on a loopback-only listener). The only
difference is what loads the model port:

| Binary | Configuration source | Tool catalogue | Use case |
|---|---|---|---|
| `garive-host serve` | `desktop-v1.json` + keychain | Desktop built-ins + workspaces | Tauri Desktop companion |
| `garive-host serve-stdin` | `desktop-v1.json` + stdin credential | Desktop built-ins + workspaces | Tauri-less one-shot |
| `garive-headless` | `<config-dir>/garive-desktop.db::runtime_management_config` | Model-only, or five governed workspace tools when an explicit root is supplied | API-driven sessions |

Argument grammar:

```
garive-headless <config-dir> [127.0.0.1:8787] [workspace-root]
garive-headless setup <config-dir> <profile-id> <endpoint|-> \
  <target-id> <model-id> <definition-id> <deployment-id> <runtime-id>
```

`<config-dir>` must contain `garive-desktop.db` with a committed singleton
row (use `garive-headless setup ...`; the credential is read from stdin). The listen
address defaults to `127.0.0.1:8787` and **must be loopback**. Supplying
`workspace-root` freezes that exact directory capability and enables
`read_text`, `list`, `search_text`, create-only `write_text`, and
digest-bound `apply_patch`. Omitting it keeps the compatibility model-only
surface. The workspace is never inferred from the process directory —
non-loopback addresses are rejected with `listen_address_not_loopback` (exit 2).

Failure modes (exit code 2):

| Code | Trigger |
|---|---|
| `invalid_arguments` | Wrong number of CLI arguments |
| `listen_address_invalid` | `[listen]` does not parse as a socket address |
| `listen_address_not_loopback` | Listener is not `127.0.0.0/8` or `::1` |
| `database_directory_unwritable` | Cannot create the config directory |
| `unsupported_schema` | SQLite file is below the minimum required schema |
| `database_unavailable` | Any other SQLite open error |
| `management_not_configured` | No singleton row has ever been committed |
| `management_storage_failed` | SQLite read failure |
| `management_profile_unknown` | `profile_id` not in the two built-in profile allowlist |
| `management_definition_unknown` | `definition_id` not in the headless allowlist |
| `management_endpoint_invalid` | `api_key` empty after construction |
| `management_profile_rejected` | `build_openai_profile` / `build_anthropic_profile` rejected the connection |
| `management_transport_invalid` | `RuntimeModelHttpTransport::openai` / `::anthropic` failed |
| `management_resolution_failed` | Definition/registry/policy pipeline rejected the inputs |
| `management_installation_invalid` | `RuntimeAgentInstallation::new` rejected the resolved snapshot |
| `worker_construction_failed` | `LocalExecutionWorker::new` returned an error |
| `workspace_construction_failed` | Workspace path, recovery path, catalogue, or governed executor binding was rejected |
| `workspace_recovery_unavailable` | Private patch recovery directory could not be created |
| `host_construction_failed` | `LiveHost::new_with_worker` returned an error |
| `host_bind_failed` | `LiveHostServer::bind` failed |
| `host_serve_failed` | `LiveHostServer::serve` returned an error |

## Definition allowlist

The headless Agent-definition catalogue stays singular. The only accepted
`definition_id` is `desktop.agent.v3`; the headless path installs it as
`headless.agent.v1` so snapshot digests stay unambiguous across the two paths.
Any other `definition_id` is rejected with `management_definition_unknown`
before the SQLite row is re-read or the worker is constructed.

Without `workspace-root`, the capability snapshot is tool-free. With an
explicit root, the snapshot freezes the exact five T1 workspace definitions,
their Prepared-v3 schemas, exact-access resolver and filesystem requirements.
Every Turn receives concrete authority, Safety, sandbox binding, descriptor-
confined executors and effect receipts. The process tool remains excluded:
headless has no configured Podman executable/socket/image lane and must not
silently weaken that boundary.

## Recipe (token9 on `127.0.0.1:9527`)

```bash
# 1. Commit config via the headless management CLI.
printf '%s\n' 'token9-loopback' | garive-headless setup /tmp/garive-headless-run \
    anthropic.messages.v1 http://127.0.0.1:9527/v1/messages \
    deepseek-v4-flash deepseek-v4-flash desktop.agent.v3 tok9-flash \
    runtime-7e22bcbe-bfa4-4c8f-a0c3-94e07be8f363

# 2. Start the headless driver.
mkdir -p /tmp/garive-workspace
garive-headless /tmp/garive-headless-run 127.0.0.1:8787 /tmp/garive-workspace

# 3. From another shell, drive a session.
SESSION=$(curl -sX POST http://127.0.0.1:8787/v1/sessions \
  -H 'Idempotency-Key: headless-1' \
  -H 'Content-Type: application/json' \
  -d '{"agent_definition_id":"desktop.agent.v3"}' | jq -r .session_id)

curl -sX POST "http://127.0.0.1:8787/v1/sessions/$SESSION/turns" \
  -H 'Idempotency-Key: headless-turn-1' \
  -H 'Content-Type: application/json' \
  -d '{"text":"say hi in one sentence"}'

# 4. Read the model's output.
curl -sN "http://127.0.0.1:8787/v1/turns/$TURN/events?after_position=0"
```

## Why the SQLite row is enough

The `runtime_management_config` row carries everything the headless path
needs:

| Column | Consumed by |
|---|---|
| `profile_id` | Profile selection in `build_headless_model_port` |
| `endpoint_override` | `EndpointSelection::Explicit(...)` |
| `model_target_id` | `ResponsesDeployment.target_id` / `MessagesDeployment.target_id` |
| `model_id` | Deployment `model_id` field |
| `deployment_id` | `LocalExecutionPolicy.deployment_id` |
| `definition_id` | Catalogue entry identity |
| `api_key` | `SecretValue` (only read via `ManagementConfigStore::read_with_credential`) |
| `runtime_id` | Logged on startup; not used for routing |
| `configuration_revision` | Logged on startup; bumped on each `setup-management` |
| `configuration_digest` | Verifies byte-for-byte equality with the committed body |

Hot-swap is intentionally out of scope — every `setup-management` commit
requires a process restart, matching the existing Setup receipt contract
(R4 in `docs/runtime-management-port.md`).

## Why the API key is plaintext

Inherited from the management-port slice (R1 in
`docs/runtime-management-port.md`): the user explicitly chose "API key 明文进
SQLite" over the existing keychain path. `ManagementConfigStore::read` does
**not** expose `api_key`; only `read_with_credential` does, and it is marked
"trusted internal callers only". The H1 `GET /v1/management/setup` endpoint
returns the `ManagementConfigRead` struct, which has no `api_key` field.

## What changed

| File | Change |
|---|---|
| `runtime/replica/src/lib.rs` | `pub mod headless;` + `drive_pending` / `DrivePendingOutcome` exports |
| `runtime/replica/src/headless.rs` (NEW) | `runtime::headless` module — model port, installation, policy, attempt builders, `HeadlessClock`, error taxonomy |
| `runtime/replica/src/live_host/service.rs` | Added `LiveHost::new_with_worker(...)` returning `(LiveHost, LocalTurnDispatcher, LocalDispatchQueue)` |
| `runtime/replica/src/local_worker.rs` | Added `drive_pending(...)` + `DrivePendingOutcome` four-way enum |
| `runtime/replica/src/management/types.rs` | Added `ManagementConfigStateWithCredential` |
| `runtime/replica/src/management/store.rs` | Added `ManagementConfigStore::read_with_credential()` + `SELECT_COLUMNS_WITH_CREDENTIAL` |
| `runtime/replica/src/management/mod.rs` | Re-export `ManagementConfigStateWithCredential` |
| `runtime/replica/src/bin/garive-headless.rs` (NEW) | Loopback-only `garive-headless` binary |
| `runtime/replica/Cargo.toml` | Added `[[bin]] garive-headless`, promoted `garive-provider-profile` from dev-dep to dep |
| `runtime/replica/tests/headless_smoke.rs` (NEW) | 4 end-to-end tests: revision lookup, policy carries capabilities, attempt stamping, full H1 loopback dispatch |

`garive-desktop` was **not touched**. The `desktop-v1.json` path is still
loaded by `FileDesktopConfigurationProvider`; `FileDesktopConfigurationProvider`
still reads only `desktop-v1.json` (no merge with the SQLite row). This is
intentional — merging the two paths is a separate, larger slice (R3 in
`docs/runtime-management-port.md`).

## Verification

```bash
cargo fmt --check -p garive-runtime
cargo clippy -p garive-runtime --lib --tests --bins -- -D warnings
cargo test -p garive-runtime --lib
cargo test -p garive-runtime --test live_host_management
cargo test -p garive-runtime --test headless_smoke
```

The current real-provider/tool evidence and exact commands are recorded in
[`evidence/headless-agent-tools-real-api-2026-09-03.md`](evidence/headless-agent-tools-real-api-2026-09-03.md).
The same-Session ten-peer and non-blocking delegation run is recorded in
[`evidence/headless-multi-agent-real-api-2026-09-03.md`](evidence/headless-multi-agent-real-api-2026-09-03.md).

## Session collaboration API

One H1 Session may contain at most ten equal named Agent instances. Creation
accepts an optional `agent_name`; additional peers join through
`POST /v1/sessions/{session}/agents`, and the roster is read from the matching
GET route. Roster order and the founding-member marker do not grant authority.

Durable peer messages use `GET/POST
/v1/sessions/{session}/agent-messages`. A non-null
`to_agent_instance_id` addresses one roster member; null broadcasts to all
other members. Runtime rejects senders and recipients outside the Session.
Addressed and broadcast messages are projected into the recipient's next
model context with exact sender identity.

`POST /v1/sessions/{session}/delegations` currently admits only the
non-blocking `notify` policy and exactly three assignee selectors:

```json
{"kind":"named","agent_instance_id":"agent-..."}
{"kind":"anonymous","agent_definition_id":"desktop.agent.v3"}
{"kind":"fork_self"}
```

Runtime allocates a real assignee Turn/Execution, dispatches it without
suspending the dispatcher, and publishes the terminal result as a durable
addressed message. `await_before_final` and `suspend_execution` fail closed
until their barrier/continuation implementations satisfy MA1 and MA0.

## Risks

### R1 — API key plaintext in SQLite (carried)

Same posture as `docs/runtime-management-port.md`: the SQLite file is
unencrypted. The H1 wire never returns it. The headless binary is the only
caller of `read_with_credential()`.

### R2 — No auth on the management port (carried)

Same posture: loopback-only binding, no extra token.

### R6 — Headless process execution remains disabled

Workspace mode deliberately installs no `garive.process.run` definition.
Enabling it requires explicit immutable Podman lane configuration and
preflight, not reuse of the Runtime host process.

The headless catalogue cannot run tool-bearing agents. A `definition_id`
other than `desktop.agent.v3` is rejected at startup with
`management_definition_unknown`. Adding tool-bearing agents requires:

1. Wiring `LocalGovernedExecutionFactory` into `LocalExecutionWorker::new_governed`
   (already in the runtime API).
2. Allowing the new `definition_id` in `headless_revision_for`.
3. Wiring the `LocalKnowledgeSystemBinding` + `LocalMemorySystemBinding`
   parameters on `CatalogueCapabilityPreparationFactory::new`.

Deferred until the SQLite row carries a binding namespace.

### R7 — Definition id allowlist mirrors the Desktop value (new)

The headless path accepts `desktop.agent.v3` (the same id the Tauri Desktop
Setup flow uses today). Renaming the Desktop id requires a coordinated
change in both paths. Documented here so the next agent who touches
`system_provider.rs` is aware.

## Out of scope (deferred)

- Merging the SQLite row into `FileDesktopConfigurationProvider::load` (R3)
- Tool-bearing agents in headless mode (R6)
- `live_output` SSE on the headless path (the binary does not install a
  `LiveOutputHub`; a future slice can wire one)
- Workspace bookmarks, memory binding, knowledge binding
- Hot-swap of committed config (R4)
- Agent-callable `message_agent`, `delegate`, `collect_delegations`, and
  `fork_self` capability tools. H1 currently exposes the governed operations;
  provider-facing tool definitions are not yet installed.
- `AwaitBeforeFinal`, explicit `SuspendExecution`, bounded concurrent fork
  collection, and crash recovery of the result-delivery supervisor
