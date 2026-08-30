# A-DESKTOP-C — Backend system configuration

## Status

Accepted and implemented contract. This slice makes the Desktop shipping
composition constructible without moving configuration, credentials or model
selection into the frontend, Engine, protocol adapters or Provider profiles.

## Ownership

The Tauri backend owns startup configuration. It receives an explicit OS app
configuration directory from Tauri, reads one versioned Garive document from
that directory and resolves one credential reference through an injected
backend secret resolver. It then constructs the existing P2/R1 values and
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

## Configuration document

The only accepted file is `desktop-v1.json` under the explicit app config
directory. UTF-8 bytes are bounded to 64 KiB. Unknown fields, duplicate JSON
members, unsupported versions, absolute database paths, parent traversal and
empty required strings fail before a Host or HTTP client exists.

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

`profile_id` is an opaque registry identity. The document does not enumerate
vendors or protocol dialects. A backend registry maps an exact installed
profile identity to a constructor which consumes the explicit endpoint,
credential, model and HTTP bounds. Unknown identities fail closed. Adding a
profile does not revise this schema and does not grant hosted capabilities.

V1 is model-only and admits exactly `ModelCapability::Text`. Tool, media,
reasoning, Memory, Knowledge, Scheduler and delegation configuration require
their own Runtime composition increments; their implementation presence does
not advertise them to this model.

Policy strings map exactly to the accepted Core enums. Output limit accepts
`complete_partial`, `retry`, `suspend`, `stop` or `fail`; only `retry` requires
one non-zero `output_limit_max_retries`. Transport/unavailable accept
`suspend`, `stop`, `fail` or `alternate_then_suspend`. Missing usage accepts
`stop` or `estimate`; only `estimate` requires non-zero input and output token
charges. Contradictory optional values fail closed.

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
- profile registry rejects unknown identities and constructs both currently
  installed official profiles without changing the schema;
- temporary SQLite plus loopback protocol server proves configured startup can
  complete one durable Desktop Turn;
- source scan proves no environment lookup and no configuration read IPC; the
  only mutation exception is the exact accepted A-DESKTOP-C2 command set.

## See also

- [`desktop-configuration-onboarding.md`](desktop-configuration-onboarding.md) — safe first-run and rotation amendment.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
