# Garive

Garive is a from-scratch Agent project informed by Sylvander's lessons. The
repository is currently converging the product boundary and recovery model
before implementation slices land.

## Current shape

- `engine/`: the embeddable Agent kernel (`core`, `llm`, `tools`).
- `runtime/`: composition, sessions, durable execution, recovery, and external
  effects. `replica` is the first intended host; `gateway` remains deferred.
- `spec/`: contracts only when a real process, storage, or language boundary
  needs them.
- `docs/architecture/`: active personal design notes and the current system
  map. Start at [`docs/architecture/README.md`](docs/architecture/README.md).
- `experiments/engine-kt/`: optional Kotlin experiment, not a second source of
  truth or a release gate.

Today the Rust workspace contains only the benchmark scaffold. Commands in the
`Justfile` report placeholders honestly where an implementation has not landed.

## Design rule

Agent decides; Runtime executes and persists; adapters translate external
protocols. An external effect is never blindly replayed after an uncertain
crash window.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: architecture convergence; implementation skeleton only
