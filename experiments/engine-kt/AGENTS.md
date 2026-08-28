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

## Naming

`engine-kt` follows the **Rust crate-name convention** (think
`garive-1lm` — Rust crate names can't start with a digit, so
`1lm` is the workaround). `engine-kt` is the Kotlin mirror's
sibling — same shape, different language. The `kt` suffix marks
it as the Kotlin port; the rest of the name lines up with the
Rust side (`engine-core` ↔ `engine-kt/core`, etc.).

## Layout

```
engine-kt/
├── engine/                    Kotlin mirror of engine/*
│   ├── core/                  Agent loop, runtime primitives
│   ├── ledger/                Append-only event log
│   ├── llm/                   Language-model abstraction
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
│   └── proto/                 Generated Kotlin bindings (from spec/proto)
└── runtime/
    └── replica/               Kotlin mirror of runtime/replica
```

## Mapping

| Rust crate | Kotlin module |
|------------|---------------|
| `garive-core` | `com.garive.eng.kt.core` |
| `garive-ledger` | `com.garive.eng.kt.ledger` |
| `garive-1lm` (Rust numeric-prefix workaround) | `com.garive.eng.kt.llm` (Kotlin prefers readable names) |
| `garive-tools` | `com.garive.eng.kt.tools` |
| … | … |

The numeric-prefix crate name `garive-1lm` is a Rust identifier
constraint. The Kotlin mirror uses the natural `llm`. The
ubiquitous-language mapping in `.agents/ddd.md` applies.

## When to Use This vs. `engine/`

- **Production today**: Rust under `engine/`.
- **Cross-language conformance**: `engine-kt` is the second
  implementation that keeps `engine/` honest — `.agents/ddd.md`
  treats them as a single domain.
- **JVM-side services**: anything that wants to embed the
  agent runtime without a Rust toolchain.

## Build

```
cd experiments/engine-kt
gradle build                            # builds all Kotlin modules + tests
gradle :engine:proto:generateProto      # regenerate Kotlin bindings from spec/proto/
```

This is an **independent Gradle build** — not part of any
mobile/desktop workspace. Reuse is via Maven coordinates once
the mirror is published.

## Wire Contracts

- Generated Kotlin bindings are produced by the Gradle protobuf
  plugin from `spec/proto/`.
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
- ❌ Don't put domain types here that have no counterpart in
  `engine/`. The mirror mirrors; it doesn't extend the domain.
- ❌ Don't hand-write types that mirror `.proto` fields. Use
  generated bindings.
- ❌ Don't depend on crates from `engine/` directly. `engine-kt`
  is a separate codebase.
- ❌ Don't add Kotlin code under `engine/` (the Rust side). The
  two stay in separate trees so a future contributor can build
  one without the other.