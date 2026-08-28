# spec/AGENTS.md

> **落地规范 + 共享 wire schemas.** Anything that Rust, Kotlin,
> Go, Swift, and TypeScript all need to agree on lives here.
>
> This file applies to everything under `spec/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Single Source of Truth

- **`spec/proto/*.proto`** is the **only** place wire types are
  defined. Rust and Kotlin (and any future Go / Swift / TS)
  bindings are generated from these files.
- Hand-written request / response / message structs that mirror
  a `.proto` field are forbidden in any tier. If a hand-written
  type shadows a generated one, delete it.

## Schema Discipline

- All new packages go under a versioned namespace
  (`garive.v1`, `garive.v2`, …). Bumping a major version is a
  breaking change to the wire — coordinate with `engine/`,
  `runtime/`, `mobile/`, `desktop/`, `experiments/kotlin/`.
- Do not remove a field once shipped. Mark it deprecated and
  reserve the tag.
- New `enum` values are additive — never renumber.

## Codegen Workflow

- Rust: `engine/proto/build.rs` calls `prost-build` over
  `spec/proto/`. Output lands in `OUT_DIR` and is pulled in via
  `include!` from `engine/proto/src/lib.rs`.
- Kotlin: Gradle protobuf plugin generates Kotlin bindings into
  `experiments/kotlin/` and `mobile/`.
- Go: `buf generate` with the Go plugin (gateway).
- Regenerate on every `.proto` change; CI fails if generated
  files drift from source.

## Fixtures (`spec/fixtures/`)

- JSON / YAML inputs consumed by the cross-language conformance
  suite (`just conformance`).
- One fixture per scenario; names are stable across languages.
- Treat fixtures as part of the contract — changing one is a
  semantic change, not a refactor.

## Cross-language Sync Lock

- `just conformance` is the **only** arbiter of cross-language
  parity. Both implementations must produce byte-identical
  canonical JSON for every fixture.
- An empty `diff -u` output is the gate. Empty diff = sync held.
- If conformance fails, fix the implementation; do not edit the
  fixture to make the diff go away.