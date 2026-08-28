# Kotlin server implementation rules

> Production server modules. Accepted specs and shared fixtures, not Rust
> source, define jointly admitted behavior.

@AGENTS.md
@.agents/multi-language.md

## Current scope

The Gradle project contains `:agent-core` (C0-C3), `:llm-contract` (C1/C1b),
`:ledger-contract` (L0), `:persistence-postgres` (L1), and `:proto`. Provider
and host modules are added only with their implementation slice and native
boundary tests.

```text
runtime/server-kt/
├── settings.gradle.kts
├── build.gradle.kts
├── gradle.properties
├── gradle/wrapper/gradle-wrapper.properties
├── agent-core/        supported C0-C3 domain + shared fixtures
├── llm-contract/      supported C1/C1b model contract
├── ledger-contract/   supported L0 durable fact semantics
├── persistence-postgres/ supported L1 PostgreSQL transactions + recovery
└── proto/             generated admitted wire bindings
```

Empty historical module directories are not architectural commitments. Add a
Gradle module only with an admitted production boundary and executable test.

## Module admission

Before adding `:<module>`:

1. Link the product/research requirement.
2. Name the accepted Garive behavior being implemented.
3. Define wire, semantic, or capability conformance.
4. Add executable Kotlin tests and a build command.
5. Mark the module supported or explicitly incomplete.

## Build

```text
cd runtime/server-kt
./gradlew projects
./gradlew build
```

The committed wrapper pins the Gradle distribution. Repository gates must not
depend on whichever Gradle version happens to be installed globally.

## Language rules

- Packages use `com.garive.runtime.server.<module>`.
- Kotlin domain values are idiomatic and may map explicitly to generated wire
  values.
- Do not transcribe Rust control flow line by line.
- Do not depend on Rust crates or host paths.
- Generated code is not handwritten or edited.

## Conformance

Use the minimum conformance level required by the admitted boundary:

| Level | Kotlin evidence |
|---|---|
| Wire | Generated binding round-trip and compatibility tests. |
| Canonical | Byte/canonical JSON equality for declared canonical values. |
| Semantic | Same normalized behavior over shared fixtures. |
| Capability | Same end-to-end scenario and terminal/failure contract. |

An empty diff proves only the dimension being compared.

## What not to do

- Do not add placeholder modules for the Rust directory list.
- Do not merge an admitted C0-C3 semantic change with only one language updated.
- Do not use generated proto values as the entire internal domain model.
- Do not claim production parity from schema generation alone.
- Do not substitute an embedded database for PostgreSQL integration evidence.
