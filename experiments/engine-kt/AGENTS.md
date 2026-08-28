# Kotlin Agent implementation rules

> Supported C0/C1 semantic implementation plus focused experiments. Accepted
> specs and shared fixtures, not Rust source, define joint behavior.

@AGENTS.md
@.agents/multi-language.md

## Current scope

The Gradle project contains `:core` (C0 execution control), `:llm` (C1 model
facts), and `:proto`. C0/C1 are supported at semantic conformance level; no
Kotlin Runtime is claimed.

```text
experiments/engine-kt/
├── settings.gradle.kts
├── build.gradle.kts
├── gradle.properties
├── gradle/wrapper/gradle-wrapper.properties
├── core/              supported C0 domain + shared fixtures
├── llm/               supported C1 domain + shared fixtures
└── proto/             generated admitted wire bindings
```

Empty historical module directories are not architectural commitments. Remove
them when encountered; add a Gradle module only with an admitted experiment.

## Module admission

Before adding `:<module>`:

1. Link the product/research requirement.
2. Name the accepted Garive behavior being tested.
3. Define wire, semantic, or capability conformance.
4. Add executable Kotlin tests and a build command.
5. Mark the module experimental or supported.

## Build

```text
cd experiments/engine-kt
gradle projects
gradle build
```

The repository currently tracks wrapper properties only. Use the installed
Gradle executable until a complete wrapper is intentionally generated and
committed.

## Language rules

- Packages use `com.garive.eng.kt.<module>`.
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
- Do not merge an admitted C0/C1 semantic change with only one language updated.
- Do not use generated proto values as the entire internal domain model.
- Do not claim production parity from schema generation alone.
- Do not add a Kotlin replica until a JVM/on-device Runtime requirement exists.
