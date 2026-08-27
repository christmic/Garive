# spec/

Shared, language-agnostic content used across all parts of the
project. Anything that Rust, Kotlin, Go, Swift, and TypeScript
all need to agree on lives here.

## Sub-directories

| Path | Role |
|------|------|
| `proto/` | Wire schemas. Single source of truth for all generated bindings. |
| `fixtures/` | Test fixtures (JSON, YAML) read by the conformance suite across languages. |
| `design/` | Cross-language design write-ups (protocol specs, invariants). |

## Cross-language sync lock

- **`proto/`** is the source. Rust types are generated into
  `engine/proto/` via `build.rs` + `prost-build`; Kotlin types are
  generated into `experiments/kotlin/` and `mobile/` via the
  Gradle protobuf plugin.
- **`fixtures/`** drives the conformance target. Both languages
  consume the same fixtures; `just conformance` diffs the outputs.
  An empty diff = the wire shape has not drifted.
- **`design/`** captures shared invariants and protocol contracts
  in plain prose.