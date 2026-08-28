# engine/AGENTS.md

> **Core Agent implementation tier.** Primary implementation is
> Rust (Cargo workspace). The Kotlin mirror lives in
> `experiments/engine-kt/` and tracks this tree semantically.
>
> This file applies to everything under `engine/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Crate Layout

- One crate per sub-directory, named with the `garive-` prefix
  (e.g. `garive-core`, `garive-ledger`, `garive-1lm`).
- Each crate's `src/lib.rs` is the entry point. Sub-modules live
  under `src/<module>.rs`.
- Public API is re-exported from `lib.rs`; deeper paths are not
  considered part of the contract.

## Dependencies

- Crates within `engine/` depend only on other `engine/` crates,
  `runtime/replica`, and `spec/proto`-generated bindings. They
  **must not** depend on `runtime/gateway` (different language)
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
  unless the directory name is itself abbreviated, e.g. `1lm`).

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