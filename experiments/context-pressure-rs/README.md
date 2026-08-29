# Context-pressure evidence experiment

This Rust workspace member implements C7-A only. It loads the versioned C3/C6
reference corpus, invokes one explicit token-counter command and creates
non-overwriting context-pressure evidence. It does not implement compression or
run inside Runtime.

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

All executable, argv, cwd, environment and resource limits are explicit in the
configuration. The counter child inherits no environment. Evidence contains
only identities, digests and numeric measurements; it excludes context content,
environment values and stderr.
