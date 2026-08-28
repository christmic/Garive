# tests/

> **Cross-tier integration + E2E.** Tier-specific tests live
> with their tier (Rust unit tests in `engine/<crate>/tests/`,
> Kotlin tests in `engine-kt/<module>/src/test/`, etc.). This
> directory is for tests that **span tiers** within one
> language.

## Sub-directories

```
tests/
├── integration/    cross-tier integration, one language (mostly Rust)
├── e2e/            whole-stack smoke, real runtimes
└── conformance/    cross-language lock — owned by `just conformance`,
                    lives in `spec/fixtures/`; this dir is reserved
                    for any tooling scripts the runner needs
```

## What Goes Here

- A Rust test that exercises `engine/core` + `engine/memory`
  + `engine/llm` together.
- A Rust test that talks to a real `runtime/replica`
  process over the wire schema in `spec/proto/`.
- An E2E that boots the desktop backend, points it at a
  local replica + gateway, and checks the IPC commands round-
  trip.

## What Does NOT Go Here

- Unit tests for a single crate — they live in
  `engine/<crate>/tests/`.
- Cross-language conformance — that lives in
  `spec/fixtures/` and runs via `just conformance` from
  `bench/src/conformance.rs` (Rust side) and
  `engine-kt/conformance/` (Kotlin side).
- Per-language UI tests — they live with the tier
  (`mobile/iosApp/`, `desktop/frontend/`).

## Status

Placeholder. Tests land as the slices they exercise land.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
