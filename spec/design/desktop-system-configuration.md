# A-DESKTOP-C — Backend system configuration

## Status

Accepted and implemented contract. This slice makes the Desktop shipping
composition constructible without moving configuration, credentials or model
selection into the frontend, Engine, protocol adapters or Provider profiles.

## Ownership

The Tauri backend owns startup configuration. It receives an explicit OS app
configuration directory from Tauri and reads backend-owned versioned Garive
documents from that directory. `desktop-v1.json` owns Agent, model and Host
configuration. `runtime-tools-v1.json` separately owns optional machine-level
T1 executor resources. An injected backend secret resolver resolves opaque
credential references. The backend then constructs immutable values and
installs `DesktopHost` exactly once.

The following remain forbidden outside the exact write-only setup/rotation
channel admitted by
[`desktop-configuration-onboarding.md`](desktop-configuration-onboarding.md):

- process environment lookup;
- frontend IPC reading endpoint, model, headers, credential, configuration
  document, credential reference, or database path;
- plaintext credentials in the JSON document;
- adapter/Provider/Runtime configuration-file or credential-store lookup;
- vendor names in the stable document schema;
- silent fallback to a different profile, endpoint, model or Agent definition.

## Agent and Host document

The only accepted file name is `desktop-v1.json` under the explicit app config
directory. Schema v1 is the legacy single-Agent document and v2 adds the
monotonic setup revision. Schema v3 replaces the singular field with
`default_agent_definition_id` plus `installed_agents[]`. Schema v4 requires an
explicit non-secret local Memory binding. UTF-8 bytes are bounded to 64 KiB.
Unknown fields, duplicate JSON members, unsupported versions, absolute database
paths, parent traversal and empty required strings fail before a Host or HTTP
client exists.

```text
DesktopSystemConfigV1 {
  schema_version = 1
  database_file
  installed_agent {
    definition_id, definition_revision, snapshot_digest,
    agent_instance_namespace,
    max_iterations, max_input_tokens?, max_output_tokens?, deadline_budget_ms?
  }
  host { max_command_bytes, event_batch_size, event_poll_interval_ms }
  execution {
    profile_id, credential_ref, endpoint?, model_target_id, model_id,
    deployment_id, recovery_policy_revision,
    max_output_tokens?, max_context_items, max_context_utf8_bytes,
    max_model_attempts, max_context_rebuilds,
    output_limit_action, output_limit_max_retries?,
    transport_action, unavailable_action, missing_usage_policy,
    missing_usage_estimate_input_tokens?, missing_usage_estimate_output_tokens?
  }
  http { connect_timeout_ms, request_timeout_ms, max_response_bytes }
  dispatch_capacity
  execution_lease_duration_ms
}
```

V3 admits 1–16 Agent projections in strictly increasing unique Definition-ID
order. The default identity must resolve inside that exact list. Every entry
freezes its Definition revision, snapshot digest, namespace and Runtime limits;
the backend reconstructs each installed revision and compares every value
before Host creation. V1/v2 singular fields and v3 catalogue fields may never
coexist. A Workspace Agent revision additionally requires the same explicit
machine T1 snapshot used during setup; absence or mismatch fails closed.

V4 retains the v3 catalogue and adds:

```text
memory {
  namespace_id, scope_owner_id
  retriever_revision, source_policy_revision
  max_results, max_total_bytes
  max_repository_records, max_repository_facts
  max_document_bytes, max_content_bytes, max_id_bytes
}
```

Every value is persisted and validated before Runtime construction. The
backend combines it with the immutable built-in descriptor identity to create
one `LocalMemorySystemBinding`; it grants only the configured User scope and
ordinary namespace retrieval. The JSON carries no Memory content. Schema v4
is required for Desktop Agent v2; v1–v3 reconstruct legacy Agent v1 without
Memory and may not contain the v4 member.

`profile_id` is an opaque registry identity. The document does not enumerate
vendors or protocol dialects. A backend registry maps an exact installed
profile identity to a constructor which consumes the explicit endpoint,
credential, model and HTTP bounds. Unknown identities fail closed. Adding a
profile does not revise this schema and does not grant hosted capabilities.

The general built-in Desktop Agent v2 admits text model capability, its exact
governed Workspace write capability and the exact local Memory descriptor. The
Workspace Agent v2 admits Memory plus that Tool and the exact five-tool T1
catalogue. Media, reasoning, Knowledge, Scheduler and delegation still require
explicit Effective Snapshot entries and exact Runtime bindings; implementation
presence never advertises a capability.

Policy strings map exactly to the accepted Core enums. Output limit accepts
`complete_partial`, `retry`, `suspend`, `stop` or `fail`; only `retry` requires
one non-zero `output_limit_max_retries`. Transport/unavailable accept
`suspend`, `stop`, `fail` or `alternate_then_suspend`. Missing usage accepts
`stop` or `estimate`; only `estimate` requires non-zero input and output token
charges. Contradictory optional values fail closed.

## Machine T1 document

The optional `runtime-tools-v1.json` stores only persistent machine-level
executor resources. It is bounded by the same 64 KiB limit and uses the same
duplicate-member and unknown-field rejection rules.

```text
DesktopT1ConfigV1 {
  schema_version = 1
  policy_revision
  executor_revision
  patch_recovery
  process_recovery
  podman { executable, socket_uri, image, control_timeout_ms }
  process_lanes[] {
    name
    executables[] { alias, path }
    environment { key: { literal } | { credential_ref } }
  }
}
```

The Podman executable and lane executable paths are explicit absolute paths;
there is no `PATH` or process-environment discovery. The image is pinned by an
exact lowercase SHA-256 digest. Recovery names are single relative components
created below the app configuration directory and must remain owned by the
current user with no group/other permissions. Literal environment values are
non-secret fixed configuration; every secret uses `credential_ref` and is
resolved in the backend. Diagnostics expose environment keys but never values.

The document deliberately contains no Workspace path. An opaque authorized
Desktop Workspace capability supplies the canonical root for a Turn. Only then
may the backend bind `T1HostSystemConfig` into `T1RuntimeSystemConfig` and build
the five exact definitions, preparation port and executor router described by
[`basic-tools.md`](basic-tools.md). Construction still does not enable T1: the
definitions must exactly match the installed Effective Agent Snapshot before
Core starts. Missing `runtime-tools-v1.json` means T1 is not configured;
malformed or unsafe content fails closed without fallback.

## Secret boundary

```text
DesktopSecretResolver.resolve(credential_ref) -> SecretValue
DesktopProfileRegistry.construct(profile_id, explicit values) -> ModelPort
DesktopConfigurationProvider.load() -> NotPresent | DesktopHostConfig
```

The resolver accepts only the opaque reference and returns the redacting P2-V0
`SecretValue`. It must not return a secret in an error or debug value. The
shipping macOS resolver uses the OS credential store service
`com.garive.desktop`; tests use an injected resolver. Other platform resolvers
must be explicit implementations, never environment fallbacks.

Missing document means `not_configured`. A present but malformed document,
missing credential, unknown profile or invalid constructed value means
`invalid_configuration`; startup does not replace it with defaults.

## Operational identities and clocks

The backend constructs wall-clock RFC 3339 timestamps and cryptographically
random command, worker and lease identities. Configuration contains durations
and bounds, never generated identities or current time. One provider instance
owns one immutable parsed snapshot; live file/keychain changes take effect only
after an explicit application restart.

## Startup and failure behavior

Tauri `setup` resolves the app config directory, calls the configuration
provider once, installs a constructed `DesktopHost` before accepting IPC and
manages the resulting `DesktopState`. No document leaves the state
unconfigured. Any other configuration failure aborts startup with only a
stable error code.

For a configured Runtime, `run_agent_turn` remains the conversation surface.
A-DESKTOP-C2 adds only setup state/catalogue and write-only staged mutation; it
cannot inspect a credential, credential reference, or persisted configuration.

## Verification

- shared fixture parses into the exact non-secret construction snapshot;
- unknown/duplicate/oversized/traversal/zero-bound documents fail closed;
- missing file is distinct from malformed or missing-secret configuration;
- injected resolver proves only `credential_ref` crosses the secret boundary;
- diagnostics and serialized frontend values contain no fixture secret;
- absent T1 configuration does not enable tools; a valid document builds
  exactly five definitions only after explicit Workspace binding;
- duplicate/unknown T1 members, traversal recovery names, mutable images,
  broad recovery permissions and missing credential references fail closed;
- profile registry rejects unknown identities and constructs both currently
  installed official profiles without changing the schema;
- temporary SQLite plus loopback protocol server proves configured startup can
  complete one durable Desktop Turn and commit Memory retrieval before model
  dispatch;
- source scan proves no environment lookup and no configuration read IPC; the
  only mutation exception is the exact accepted A-DESKTOP-C2 command set.

## See also

- [`desktop-configuration-onboarding.md`](desktop-configuration-onboarding.md) — safe first-run and rotation amendment.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: accepted
