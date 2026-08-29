# Creativity baseline CR-A

This package implements the accepted CR-A prerequisite in
`spec/design/creativity-baseline.md`. It runs the same strict corpus through a
one-candidate control arm and a bounded-alternatives arm, then writes exact
paired evidence from an arm-blind evaluator.

The command ports clear inherited environment, use only constructor-supplied
configuration, enforce process and output bounds, and never retry. CR-A command
runs are permanently non-publishable. The included generator and evaluator are
deterministic fixtures that verify the harness; their output is not evidence of
model creativity.

Build and run the fixture route from the repository root:

```sh
cargo build -p garive-creativity-baseline --bins
cargo run -p garive-creativity-baseline -- run \
  experiments/creativity-baseline-rs/config.reference-v1.json
```

Choose a new `evidence_path` before rerunning. Evidence creation is
non-overwriting and excludes prompts, rubrics, candidates, environment values
and selection rationale. CR-B remains responsible for external model/evaluator
coordinates, clean-revision attestation and publication eligibility.

CR-B is implemented by `garive-creativity-publication`. It accepts only two
explicit compatible protocol dialects, resolves opaque credential references
from the `com.garive.creativity` OS credential-store service, uses the normal
Provider/adapter/Runtime transport path, and writes evidence v2 only for two
public-HTTPS endpoints plus exact clean Git attestation. Copy
`config.publication-reference-v1.json` outside the attested worktree, use
absolute corpus, evidence and repository paths, then replace every placeholder
with reviewed deployment and revision coordinates. Editing the tracked template
or writing the output into the worktree would violate the required empty Git
status. A generated v2 document still requires human review before it can admit
production Creativity behavior.
