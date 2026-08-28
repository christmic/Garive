# experiments/engine-kt/AGENTS.md

> **Kotlin mirror of the Rust engine + runtime.** `engine-kt` —
> the Kotlin half of the engine, mirroring `engine/` and
> `runtime/replica/`. Lives under `experiments/` because the
> canonical implementation is Rust under `engine/`; this
> directory exists to **prove the engine's abstractions are
> language-agnostic** and to give the JVM side a Kotlin agent
> runtime when `mobile/` and `desktop/` need one.
>
> This file applies to everything under `experiments/engine-kt/`.
> It overrides the root `AGENTS.md` where the two disagree.

@AGENTS.md
@.agents/multi-language.md

## Layout

`engine-kt/` is a **standard Gradle multi-module project**.
Every sub-directory is a top-level Gradle module — there is
**no** intermediate `engine/` or `runtime/` grouping (those
names describe Rust crates, not Gradle modules).

```
engine-kt/
├── AGENTS.md                 this file
├── settings.gradle.kts       Gradle module list (:proto, ...)
├── build.gradle.kts          root build (plugin versions, group/version, repos)
├── gradle.properties         Gradle / Kotlin defaults
├── gradle/wrapper/
│   └── gradle-wrapper.properties   (run `gradle wrapper` once to generate gradlew/jar)
│
├── proto/                    :proto  — generated Kotlin bindings from spec/proto/
│   ├── build.gradle.kts
│   └── src/main/proto/       protobuf Gradle plugin source dir
│
├── core/         placeholders — `include(":core")` when the slice lands
├── ledger/
├── llm/
├── tools/
├── memory/
├── knowledge/
├── skill/
├── multiagent/
├── scheduler/
├── creativity/
├── eval/
├── observability/
└── config/
```

The placeholder sub-directories (`core/`, `llm/`, `tools/`,
`memory/`, `knowledge/`, `skill/`, `multiagent/`, `scheduler/`,
`creativity/`, `eval/`, `observability/`, `config/`, `ledger/`)
are **not** Gradle modules today — they are reserved for the
slices that mirror the Rust `engine/*` crates. Add a module to
`settings.gradle.kts` and drop a `build.gradle.kts` into the
sub-directory when its slice starts landing code.

## Why Standard Gradle Multi-module (no engine/runtime layer)

The Rust side uses a Cargo workspace with a flat list of
crates — `engine/core`, `engine/llm`, `runtime/replica`, etc.
Kotlin / Gradle doesn't need a mirror of that nesting. Each
Gradle module is just a directory with its own
`build.gradle.kts`; the module's name (`include(":proto")`,
`include(":replica")`, ...) is the only thing that needs to match
the directory. There is no need for `engine/` and `runtime/`
intermediate directories that only exist to mirror Rust's
crate-name convention. A Kotlin replica module will be added only after the
Kotlin Agent experiment has an executable product requirement.

When the Kotlin mirror grows, each slice becomes its own
top-level Gradle module. That's the standard layout.

## Naming

| Rust crate | Gradle module | Kotlin package |
|------------|---------------|----------------|
| `garive-core` | `:core` | `com.garive.eng.kt.core` |
| `garive-ledger` | `:ledger` | `com.garive.eng.kt.ledger` |
| `garive-llm` | `:llm` | `com.garive.eng.kt.llm` |
| `garive-tools` | `:tools` | `com.garive.eng.kt.tools` |
| … | … | … |
| `garive-replica` | `:replica` | `com.garive.eng.kt.replica` |

Both implementations use the natural `llm` name. The ubiquitous-language
mapping in `.agents/ddd.md` applies.

## Adding a New Module

1. Create `<module>/build.gradle.kts` (typically a 4-line file
   that adds module-specific deps on top of the shared
   `beforeProject` config in the root `build.gradle.kts`).
2. Create `<module>/src/main/kotlin/com/garive/eng/kt/<module>/`
   for the source code.
3. Add `include(":<module>")` to `settings.gradle.kts`.
4. If the module depends on other engine modules, add
   `implementation(project(":<other>"))` to its
   `build.gradle.kts`.

## Build

```
cd experiments/engine-kt
gradle build                          # builds every module + tests
gradle :proto:generateProto           # regenerate Kotlin bindings from spec/proto/
```

Run `gradle wrapper` once in `engine-kt/` to generate
`gradlew`, `gradlew.bat`, and the wrapper jar.

## Wire Contracts

- Generated Kotlin bindings are produced by the protobuf Gradle
  plugin (`proto/build.gradle.kts`) from `spec/proto/`.
- Hand-written request / response types mirroring `.proto`
  fields are **forbidden**.
- Bumping a `.proto` package version requires regenerating
  Rust + Kotlin in lock-step.

## Conformance Lock

- `just conformance` reads `spec/fixtures/`, runs the canonical
  implementation in **both** Rust and Kotlin, and diffs the
  canonical JSON output.
- An empty diff is the gate. Empty diff = the two implementations
  agree on the wire.
- If conformance fails, fix the implementation. Do not edit
  the fixture to make the diff go away.

## What NOT to Do

- ❌ Don't let `engine-kt` drift semantically from `engine/`.
  Mirror changes are paired with the Rust change in the same
  PR (or in lock-step across two).
- ❌ Don't put domain types in a module that have no counterpart
  in `engine/`. The mirror mirrors; it doesn't extend the domain.
- ❌ Don't hand-write types that mirror `.proto` fields. Use
  generated bindings.
- ❌ Don't depend on crates from `engine/` directly. `engine-kt`
  is a separate codebase.
- ❌ Don't add Kotlin code under `engine/` (the Rust side). The
  two stay in separate trees so a future contributor can build
  one without the other.
- ❌ Don't reintroduce an `engine/` or `runtime/` intermediate
  directory. Each Gradle module sits directly under `engine-kt/`.

## Testing

This tier follows the test pyramid in `.agents/testing.md`.
For `engine-kt/`, the relevant layers:

| Layer | Where | What |
|-------|-------|------|
| Static | per module | `ktlint`, `detekt` |
| Unit | `engine-kt/<module>/src/test/kotlin/` | one test per behaviour; TDD-first |
| Property | `engine-kt/<module>/src/test/kotlin/` | `kotest-property` for aggregate invariants |
| Integration | `engine-kt/<module>/src/test-integration/kotlin/` + `tests/integration/` | multi-module flows |
| Contract | `engine-kt/proto/src/test/` | round-trip every `.proto` message |
| Cross-language | driven from `just conformance` (Rust ↔ Kotlin) | sync lock |

Add a fuzz target in `engine-kt/proto/fuzz/` per message.
Same contract as the Rust side: every wire message has a fuzz
target, full stop.
