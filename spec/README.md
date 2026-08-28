# spec/

> **落地规范 + 共享 wire schemas.** Anything that Rust, Kotlin, Go,
> Swift, and TypeScript all need to agree on — and that has to be
> implemented faithfully, not just thought about.

This directory is the **concrete, normative** layer of the project.
If it lives in `spec/`, it is meant to be implemented; if it lives
in `docs/`, it is meant to be **discussed, designed, and explored**.
Do not move content between the two lightly.

## What Goes Here

| Subdir / file | Role |
|---|---|
| `proto/` | Wire schemas — single source of truth for all generated bindings (Rust + Kotlin). |
| `fixtures/` | Test data consumed by the cross-language conformance suite. |
| `design/` | Short, normative cross-language protocol specs and invariants — prose that names a contract and points at the `.proto` field that enforces it. |

## What Does NOT Go Here

- Free-form thinking, exploratory sketches, ADRs-in-progress.
  Those belong in `docs/`.
- Per-feature designs. Those belong in `docs/architecture/`.
- API references or tutorials. Those belong in `docs/`.

## Cross-language Sync Lock

- `proto/` is the **source**. Rust types are generated into
  `engine/proto/` via `build.rs` + `prost-build`; Kotlin types are
  generated into `experiments/kotlin/` and `mobile/` via the
  Gradle protobuf plugin.
- `fixtures/` drives the conformance target. Both languages
  consume the same fixtures; `just conformance` diffs the outputs.
  An empty diff = the wire shape has not drifted.
- Hand-edits to generated code in either language are forbidden —
  change the `.proto`, regenerate.