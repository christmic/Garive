# engine/AGENTS.md

> **Core Agent implementation tier.** Primary implementation is
> Rust (Cargo workspace). `experiments/engine-kt/` may validate selected
> semantics after the Rust boundary is executable; it is not a mirror gate.
>
> This file applies to everything under `engine/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md
@.agents/multi-language.md

## Crate Layout

- Add a crate only for an owned boundary under `core`, `llm`, or `tools`, named
  with the `garive-` prefix (for example `garive-core`).
- Each crate's `src/lib.rs` is the entry point. Sub-modules live
  under `src/<module>.rs`.
- Public API is re-exported from `lib.rs`; deeper paths are not
  considered part of the contract.

## Dependencies

- Crates within `engine/` depend only on lower-level `engine/` contracts and
  wire bindings required by a real external boundary. They **must not** depend
  on `runtime/replica`, `runtime/gateway`,
  or on crates in `mobile/`, `desktop/`, `experiments/engine-kt/`.
- Third-party dependencies are added to the root
  `[workspace.dependencies]` table so versions stay aligned.
- Avoid heavy I/O or framework dependencies; keep the core
  runtime testable and embeddable.

## Naming

- Modules: `snake_case`.
- Types / traits: `PascalCase`.
- Functions / variables: `snake_case`.
- Constants: `SCREAMING_SNAKE_CASE`.
- Crate names: `garive-<dir>` (kebab-case, no abbreviation
  unless the directory name is itself abbreviated, e.g. `llm`).

## Module-level Imports

- All `use` declarations live at module scope, after module-level
  docs and attributes, before type / constant / / impl / fn
  declarations.
- No indented local imports inside fn / impl / match / test
  bodies.
- Local imports are allowed only when a documented compiler,
  macro-expansion, or cfg constraint makes a module-level import
  impossible. Tag with `// Local import required: <reason>`.

## Verification

Each crate in `engine/` must pass:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test`
- `cargo doc --no-deps -- -D warnings`

A crate that fails any of these is not merge-ready.

## Testing

This tier follows the test pyramid in `.agents/testing.md`.
For `engine/`, the relevant layers:

| Layer | Where | What |
|-------|-------|------|
| Static | root + per crate | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` |
| Unit | `engine/<crate>/tests/` | one test per behaviour; TDD-first per `.agents/ddd.md` |
| Property | `engine/<crate>/tests/` | `proptest` for aggregate invariants |
| Integration | `engine/<crate>/tests-integration/` + `tests/integration/` | multi-crate flows |
| Contract | beside the owning boundary | round-trip only shipped wire contracts |
| Cross-language | executable conformance harness | compare selected semantics when a second implementation exists |

Add fuzzing where parser risk and evidence justify it; a schema file alone does
not create a blanket fuzz-target requirement.
