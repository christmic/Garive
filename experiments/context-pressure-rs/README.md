# Context-pressure evidence experiment

This Rust workspace member implements the C7-A evidence runner and C7-B exact
provider-counter composition. It loads the versioned C3/C6 reference corpus and
creates non-overwriting context-pressure evidence. It does not implement
compression or run inside Runtime.

Run the checked-in development evidence from the repository root after building
both binaries:

```text
cargo build -p garive-context-pressure --bins
target/debug/garive-context-pressure run \
  experiments/context-pressure-rs/config.reference-v1.json
```

The checked-in `reference-v1.json` uses
`garive-fixture-utf8-ceil4`. That deterministic counter validates the complete
pipeline but is permanently non-publishable and is not a provider tokenizer.
Its pressure values cannot select C7 thresholds. A new evidence path, clean
Garive revision and admitted exact provider counter are required for the
publication-grade run that may unlock the focused C7 behavior Spec.

For an exact Messages-compatible count, composition code constructs
`AnthropicProviderCounter` from an explicit `MessagesDeployment`, an already
resolved `AnthropicTokenCountProfile`, and an injected `TokenCountExchangePort`.
The route is Core assembly → normal P2-C mapping → P2-VX-ATC count projection →
one bounded exchange. No environment or configuration lookup occurs in this
module. Fake and loopback ports must be non-publishable; an eligible production
port and externally resolved credential are required for publication evidence.

`config.provider-reference-v1.json` is the strict non-secret publication
template. Replace its clean full Git revision and exact supported model ID,
then install the credential under OS credential-store service
`com.garive.context-pressure` with account/reference
`anthropic-context-pressure`. Do not add a credential field or environment
entry to the document. The runner independently verifies the configured HEAD
and empty porcelain status before resolving the credential or opening HTTP.

```text
target/debug/garive-context-pressure run \
  experiments/context-pressure-rs/config.provider-reference-v1.json
```

Provider publication uses only the in-process exact descriptor. The legacy
`command` counter remains useful for deterministic development evidence but is
permanently non-publishable regardless of its executable identity.

All executable, argv, cwd, environment and resource limits are explicit in the
configuration. The counter child inherits no environment. Evidence contains
only identities, digests and numeric measurements; it excludes context content,
environment values, credential references, secrets and stderr.
