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
template. Copy it outside the attested worktree, use absolute corpus, evidence
and repository paths, then replace its clean full Git revision and exact
supported model ID. Do not edit the tracked template: publication requires an
empty Git status, including untracked files. Install the credential under OS
credential-store service
`com.garive.context-pressure` with account/reference
`anthropic-context-pressure`. Do not add a credential field or environment
entry to the document. The runner independently verifies the configured HEAD
and empty porcelain status before resolving the credential or opening HTTP.

```text
target/debug/garive-context-pressure run \
  /absolute/path/outside/worktree/context-pressure-publication.json
```

Provider publication uses only the in-process exact descriptor. The legacy
`command` counter remains useful for deterministic development evidence but is
permanently non-publishable regardless of its executable identity.
Publication evidence is schema v2 and binds SHA-256 digests of the actual Git
executable and its non-secret attestation configuration; development evidence
remains schema v1.

The output destination is exclusively reserved before any counter process,
credential lookup or HTTP request. A pre-existing path therefore fails without
spending a provider call, and any later failure removes the empty reservation.

All executable, argv, cwd, environment and resource limits are explicit in the
configuration. The counter child inherits no environment. Evidence contains
only identities, digests and numeric measurements; it excludes context content,
environment values, credential references, secrets and stderr.
