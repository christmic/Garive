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

## Layout

`engine-kt/` is a **Gradle multi-module project**, not a flat
copy of `engine/`. Each sub-directory is its own Gradle module
with its own `build.gradle.kts`.

```
engine-kt/
├── AGENTS.md                  this file
├── settings.gradle.kts        module list (`:engine:*`, `:runtime:*`)
├── build.gradle.kts           root build (plugin versions, group / version)
├── gradle.properties          repo-wide Gradle / Kotlin defaults
├── gradle/wrapper/
│   └── gradle-wrapper.properties   (run `gradle wrapper` once to generate gradlew/jar)
│
├── engine/                    Gradle multi-module "engine" group
│   ├── build.gradle.kts       applies Kotlin JVM to every :engine:* subproject
│   ├── core/                  Agent loop, runtime primitives
│   │   ├── build.gradle.kts
│   │   └── src/main/kotlin/com/garive/eng/kt/core/
│   ├── ledger/                Append-only event log
│   ├── llm/                   Language-model abstraction (provider-agnostic)
│   ├── tools/                 Tool registry
│   ├── memory/                Short- and long-term memory
│   ├── knowledge/             Knowledge store + retrieval
│   ├── skill/                 Skill packaging
│   ├── multiagent/            Multi-agent coordination
│   ├── scheduler/             Task scheduling
│   ├── creativity/            Value discovery
│   ├── eval/                  Eval harness
│   ├── observability/         Tracing, metrics, logs
│   ├── config/                Config schema + loaders
│   └── proto/                 Generated Kotlin bindings from spec/proto/
│       ├── build.gradle.kts   protobuf Gradle plugin
│       └── src/main/proto/    linked source for spec/proto/
│
└── runtime/
    └── replica/               Kotlin mirror of runtime/replica (Gradle application module)
        ├── build.gradle.kts
        └── src/main/kotlin/com/garive/eng/kt/replica/
```

## Why Gradle Multi-module (not a 1:1 Rust Copy)

Rust uses a Cargo workspace — one repo with N crates, each a
flat sub-directory. Kotlin uses Gradle — one repo with N
**modules**, each its own sub-project with its own
`build.gradle.kts`. The two layouts look superficially
similar but:

- Gradle modules share JVM and Kotlin toolchain configuration
  through the `subprojects { ... }` block in
  `engine/build.gradle.kts`.
- A Kotlin module declares its **own dependencies** in its own
  `build.gradle.kts` (`implementation(project(":engine:proto"))`,
  etc.), so module boundaries are explicit and enforceable.
- The protobuf module (`engine/proto/`) is a real Gradle module
  because it owns the protobuf plugin and generated sources —
  there is no "Cargo build script" analogue.
- Each module has the standard `src/main/kotlin/` and
  `src/test/kotlin/` source sets; tests live next to the code
  they exercise.

## Naming

| Rust crate | Gradle module | Kotlin package |
|------------|---------------|----------------|
| `garive-core` | `:engine:core` | `com.garive.eng.kt.core` |
| `garive-ledger` | `:engine:ledger` | `com.garive.eng.kt.ledger` |
| `garive-1lm` (Rust numeric-prefix workaround) | `:engine:llm` | `com.garive.eng.kt.llm` |
| `garive-tools` | `:engine:tools` | `com.garive.eng.kt.tools` |
| … | … | … |
| `garive-replica` | `:runtime:replica` | `com.garive.eng.kt.replica` |

The numeric-prefix crate name `garive-1lm` is a Rust identifier
constraint; the Kotlin mirror uses the natural `llm`. The
ubiquitous-language mapping in `.agents/ddd.md` applies.

## Adding a New Module

1. Create `engine/<name>/build.gradle.kts` (start from the
   placeholder comment already there) and
   `engine/<name>/src/main/kotlin/com/garive/eng/kt/<name>/`.
2. Add `include(":engine:<name>")` to `settings.gradle.kts`.
3. If the module depends on other engine modules, add
   `implementation(project(":engine:other"))` to its
   `build.gradle.kts`.

## Build

```
cd experiments/engine-kt
gradle build                          # builds every module + tests
gradle :engine:proto:generateProto    # regenerate Kotlin bindings from spec/proto/
```

Run `gradle wrapper` once in `engine-kt/` to generate
`gradlew`, `gradlew.bat`, and the wrapper jar.

## Wire Contracts

- Generated Kotlin bindings are produced by the protobuf Gradle
  plugin (`engine/proto/build.gradle.kts`) from `spec/proto/`.
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