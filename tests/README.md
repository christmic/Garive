# tests/

> **Cross-tier integration + E2E.** Tier-specific tests live
> with their tier (Rust unit tests in `engine/<crate>/tests/`,
> optional Kotlin experiment tests in `experiments/engine-kt/`). This
> directory is for tests that **span tiers** within one
> language.

## Sub-directories

```
tests/
├── integration/    cross-tier integration, one language (mostly Rust)
├── e2e/            whole-stack smoke, real runtimes
└── conformance/    executable cross-implementation checks when a slice needs them
```

## What Goes Here

- A Rust test that exercises `engine/core` + `engine/llm` + Runtime ports.
- A Rust test that talks to a real `runtime/replica`
  process over the wire schema in `spec/proto/`.
- An E2E that boots the desktop backend, points it at a
  local replica + gateway, and checks the IPC commands round-
  trip.

## What Does NOT Go Here

- Unit tests for a single crate — they live in
  `engine/<crate>/tests/`.
- Unit-only serialization tests for one consumer — keep them beside that
  consumer. Shared conformance belongs here only once executable.
- Per-language UI tests — they live with the tier
  (`mobile/iosApp/`, `desktop/frontend/`).

## Status

Placeholder. Tests land as the slices they exercise land.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: stub — slice not yet landed; content is scaffolding.
