# Garive

Garive is a from-scratch Agent project informed by Sylvander's lessons. The
repository is currently converging the product boundary and recovery model
before implementation slices land.

## Current shape

- `engine/`: buildable Rust crates for the Agent kernel and planned capability
  modules. Domain policy/ports live here; concrete I/O stays in Runtime.
- `runtime/`: composition, sessions, durable execution, recovery, and external
  effects. `replica` is the first host; `gateway` is the planned Go edge.
- `spec/`: contracts only when a real process, storage, or language boundary
  needs them.
- `docs/architecture/`: active personal design notes and the current system
  map. Start at [`docs/architecture/README.md`](docs/architecture/README.md).
- `runtime/server-kt/`: supported Kotlin C0-C3 server implementation and
  native adapters; accepted specs and shared fixtures remain the source of truth.

The Rust workspace and Kotlin server currently implement portable C0-C3 from
shared fixtures. Other crates and product surfaces remain explicit skeletons.
Commands in the `Justfile` report unimplemented paths without presenting them
as successful gates.

## Design rule

Agent decides; Runtime executes and persists; adapters translate external
protocols. An external effect is never blindly replayed after an uncertain
crash window.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: architecture convergence; implementation skeleton only
