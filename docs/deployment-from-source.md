# Deploy Garive from source on a new machine

> This runbook takes an operator from a clean clone to a configured local
> Garive Host and a working CLI, TUI, Web, or Desktop client. It separates the
> reproducible developer deployment from mobile, signing, and hardware release
> gates that require external infrastructure.

## Audience

Operators and contributors installing Garive on a new macOS or Linux machine.
The reader needs a model deployment endpoint and credential, but does not need
prior knowledge of Garive's internal Engine or Ledger.

## Why

Garive is a multi-toolchain repository. The production Agent and Runtime are
Rust, while Web/Desktop, mobile, the Kotlin verification Engine, and Gateway
have independent build chains. A clone is not deployable until the exact
toolchains, explicit Runtime configuration, credential-store boundary, and
loopback Host/client topology are established.

## Quick start: Host plus TUI

This is the smallest production Runtime path. Run every command from the
repository root unless a command changes directory.

### 1. Clone and select the revision

```sh
git clone git@github.com:christmic/Garive.git
cd Garive
git switch master
git pull --ff-only
git status --short --branch
```

For a reproducible deployment, record `git rev-parse HEAD` in the deployment
record and do not build from a dirty tree.

### 2. Install the minimum toolchain

Install Git, a native C/C++ linker, `curl`, `jq`, and `rustup`. Then let the
checked-in Rust toolchain file select the compiler and required components:

```sh
rustup show
rustup show active-toolchain
rustup component add clippy rustfmt
rustc --version
cargo --version
```

The expected Rust version is owned by [`rust-toolchain.toml`](../rust-toolchain.toml),
not this runbook. If that file changes, install its new stable version instead.
On Linux, also install the distribution packages required by crates that use
TLS, SQLite, and desktop keyring integration. On macOS, install the current
Xcode Command Line Tools with `xcode-select --install` when no compiler exists.

### 3. Build the shipping local processes

```sh
cargo build --locked --release -p garive-desktop --bin garive-host
cargo build --locked --release -p garive-cli
cargo build --locked --release -p garive-tui
```

The resulting executables are:

| Process | Path | Responsibility |
|---|---|---|
| Host | `target/release/garive-host` | Runtime, SQLite, model transport, durable Sessions |
| CLI | `target/release/garive` | One command/response interaction with a running Host |
| TUI | `target/release/garive-tui` | Resident terminal client for a running Host |

### 4. Create explicit Runtime configuration

The Host currently installs either an OpenAI Responses-compatible or Anthropic
Messages-compatible profile. The profile names describe wire protocols; they
do not restrict the endpoint to one vendor. Enter the exact values for the
deployment being installed:

```sh
config_dir="$(pwd)/tmp/source-deployment"
mkdir -p "$config_dir"

printf 'Profile (openai.responses.v1 or anthropic.messages.v1): '
read -r profile_id
printf 'Complete protocol endpoint URL: '
read -r endpoint
printf 'Garive model target ID: '
read -r target_id
printf 'Provider model ID: '
read -r model_id
printf 'Installed Agent definition ID: '
read -r definition_id
printf 'Connection credential (input hidden where supported): '
stty -echo 2>/dev/null || true
read -r connection_credential
stty echo 2>/dev/null || true
printf '\n'

printf '%s\n' "$connection_credential" | \
  target/release/garive-host configure "$config_dir" \
    "$profile_id" "$endpoint" "$target_id" "$model_id" "$definition_id"
connection_credential=
```

`configure` writes `desktop-v1.json` and the SQLite database below the explicit
configuration directory. It writes the credential to the operating-system
credential store under service `com.garive.desktop`; the JSON contains only an
opaque reference. Do not edit the JSON by hand or store a credential in it.
Re-running `configure` performs a bounded replacement and requires a Host
restart.

New installations write schema v4. Its non-secret `memory` member freezes the
local namespace, User-scope owner, retrieval policy revisions and all scan,
document and result bounds. Memory content remains in the fact-backed SQLite
repository, never in this JSON. Older schema v1–v3 files retain legacy Agent
meaning and do not acquire Memory until an explicit `configure` replacement.

The endpoint must be `http` or `https`, include a host, and contain no user
information or fragment. Use `http` only for a trusted loopback gateway. Model
target, model, deployment, and definition identities are non-empty explicit
Garive configuration; they are not discovered from environment variables.

### 5. Start and verify the Host

Start the Host in terminal A. It accepts loopback listeners only:

```sh
target/release/garive-host serve "$config_dir" 127.0.0.1:8787
```

Expected startup text contains:

```text
Garive Host listening on http://127.0.0.1:8787
```

In terminal B, verify the public Host read model:

```sh
curl --fail --silent http://127.0.0.1:8787/v1/agent-definitions | jq .
curl --fail --silent 'http://127.0.0.1:8787/v1/sessions?limit=10' | jq .
```

The first response must contain the configured definition. A non-2xx response,
invalid JSON, or a missing definition means the deployment is not admitted.

On a locked/headless machine without an available OS credential service, start
the same stored configuration with a write-only stdin credential:

```sh
printf 'Connection credential: '
stty -echo 2>/dev/null || true
read -r connection_credential
stty echo 2>/dev/null || true
printf '\n'
printf '%s\n' "$connection_credential" | \
  target/release/garive-host serve-stdin "$config_dir" 127.0.0.1:8787
connection_credential=
```

Do not place the credential in argv, a tracked file, a shell profile, or an
`.env` file. A production service manager should feed stdin from its protected
secret facility and restrict access to the configuration/database directory.

### 6. Run a client

For the TUI:

```sh
target/release/garive-tui --host http://127.0.0.1:8787/
```

For one CLI Turn in a new Session:

```sh
printf 'Installed Agent definition ID: '
read -r definition_id
target/release/garive http://127.0.0.1:8787/ "$definition_id" \
  'Reply with the exact text: Garive is running.'
```

The CLI exits `0` on completion, `3` on suspension, `4` when stopped, and `5`
on Agent failure. Invalid arguments and client/transport failures exit `2`.
See the complete TUI options in the
[`TUI user guide`](manual/tui-user-guide.md).

## Full build and verification

Install `just`, `jq`, JDK 21, and the platform tools needed by the selected
clients. The Gradle Wrapper, Cargo lock, pnpm locks, Go module, and Swift
package manifests are the dependency reproduction sources.

```sh
just --list
just build
just verify
```

`just build` compiles the locked Rust workspace and experimental Kotlin Engine.
`just verify` additionally runs architecture boundaries, Rust/Kotlin semantic
conformance, protocol/provider tests, Runtime recovery, clients, strict Clippy,
and rustdoc. It is a release-sized gate rather than a quick smoke test.

| Toolchain | Repository owner | Required host capability |
|---|---|---|
| Rust | `rust-toolchain.toml`, `Cargo.lock` | native linker |
| Kotlin/Android | Gradle Wrapper and Kotlin/Android plugin manifests | JDK 21; Android SDK 36 for Android |
| TypeScript | package manifests and `pnpm-lock.yaml` | Node satisfying locked package engine constraints; pnpm lock v9 support |
| Go Gateway | `runtime/gateway/go.mod` | exact stable Go line declared there |
| Swift/iOS/macOS | `Package.swift`, Xcode project settings | current compatible stable Xcode/SDK |

Do not regenerate lockfiles during deployment. Dependency upgrades follow
[`dependency-versions.md`](../.agents/dependency-versions.md) and require native
verification before admission.

## Client builds

### Web

```sh
cd web
pnpm install --frozen-lockfile
pnpm test
pnpm build
pnpm dev
```

Open `http://127.0.0.1:1430/`. Development Vite proxies `/v1` to the configured
loopback Host. A deployed static bundle requires an equivalent same-origin
`/v1` reverse proxy; do not expose the Runtime listener beyond loopback.

### Desktop

Install the current Tauri 2 operating-system prerequisites, then run:

```sh
cd desktop/frontend
pnpm install --frozen-lockfile
pnpm test
cd ../backend
../frontend/node_modules/.bin/tauri dev
```

The Desktop setup screen owns its app-config directory and writes the same
backend configuration contract through typed IPC. Use that screen for normal
Desktop installation. A local macOS DMG is built and audited by the commands in
[`desktop/release/README.md`](../desktop/release/README.md); it is not a public
release until signing, notarization, Universal architecture, updater, and clean
machine gates pass.

### Android and iOS

```sh
just mobile-android
just mobile-ios
```

Android requires JDK 21 and Android SDK 36. iOS requires macOS, Xcode, and an
iOS Simulator. These commands create development candidates. Distribution
requires real signing identities, provisioning, public CA TLS, notification
credentials, and the physical-device admission described in
[`mobile/androidApp/README.md`](../mobile/androidApp/README.md) and
[`mobile/iosApp/README.md`](../mobile/iosApp/README.md).

### Optional mobile Gateway

The Gateway is required only when native mobile clients connect from outside
the Host machine. Build and test it independently:

```sh
cd runtime/gateway
go test -race ./...
go build ./cmd/garive-gateway
```

Its TLS, pairing, admin, Runtime-origin, APNs, and FCM operator inputs are
documented in [`runtime/gateway/README.md`](../runtime/gateway/README.md).
Keep the Runtime on loopback and store Gateway secrets in a protected service
manager or orchestrator, never in the repository.

## Configuration and data ownership

| Data | Owner | Migration rule |
|---|---|---|
| `desktop-v1.json` | Runtime/Desktop backend | Copy only with its database; validate on startup; never hand-edit |
| `garive-desktop.db` | Runtime SQLite Ledger | Stop Host before backup or restore; preserve file permissions |
| model credential | OS credential store or protected stdin injector | Re-enter/rebind on a new machine; it is not contained in Git or JSON |
| TUI preferences/history | TUI `--state-dir` or platform local state | Optional presentation data; may be omitted with `--ephemeral` |
| Web preferences | browser storage | Presentation-only; never Runtime truth |
| mobile grants | Keychain/Keystore | Pair each new installation; do not copy grants between devices |

For a machine migration, stop the old Host, copy the configuration directory
through an encrypted channel with owner-only permissions, then run `configure`
on the new machine to create a new local credential-store binding. Keep an
offline backup until the new Host passes both read-model checks and one real
Turn.

## Capability deployment status

| Capability | Source state | Deployment meaning |
|---|---|---|
| Agent/Runtime/CLI/TUI/Web | executable | Available through the Host flow above |
| Tauri Desktop | local candidate | Runnable locally; public macOS release gates remain external |
| Android/iOS | installable development clients | public TLS, push credentials, signing, and physical devices remain external |
| Managed Browser | native adapter increments | No general-purpose shipping browser launcher is admitted yet |
| Attached Browser | framing/config adapter foundation | Extension, native-host registration, and Runtime grant lifecycle are not complete |
| macOS Computer Use | verified Swift package slices | Packaged XPC service, pixel/pointer/scroll coverage, and permission-granted app evidence remain open |

Do not enable an incomplete capability by editing configuration. Its accepted
Spec, shipping composition, and platform evidence must land first.

## Failure reference

| Symptom | Cause to check | Action |
|---|---|---|
| `configuration_missing` | no `desktop-v1.json` in the supplied directory | run `garive-host configure` against that exact directory |
| `configuration_load_failed` | malformed/stale document, missing database path, or unavailable credential | preserve files, inspect permissions, then reconfigure instead of editing JSON |
| `listen_address_not_loopback` | Host was given a public/interface-wide listener | bind `127.0.0.1` or `::1`; use Gateway for remote clients |
| `host_bind_failed` | port occupied or denied | stop the conflicting process or select another loopback port and update clients |
| CLI exit `3` | Agent requires approval or external input | resume in TUI/Desktop and resolve the exact suspension |
| TUI `invalid arguments; use --help` | invalid Host URL, relative state path, or malformed option | run `garive-tui --help`; use a credential-free loopback root URL |
| keyring failure on headless Linux | no unlocked Secret Service session | use protected `serve-stdin` or provision the system credential service |
| pnpm engine error | Node does not satisfy a locked dependency | install a compatible stable Node release; keep the lockfile unchanged |

## See also

- [`README.md`](../README.md) — repository map and implemented foundation.
- [`Justfile`](../Justfile) — canonical build and verification recipes.
- [`local-runtime-composition.md`](../spec/design/local-runtime-composition.md) — explicit Runtime composition contract.
- [`desktop-system-configuration.md`](../spec/design/desktop-system-configuration.md) — configuration and secret boundary.
- [`AGENTS.md`](../AGENTS.md) — repository constitution entry point.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: active source-deployment runbook
