# Experimental Kotlin Engine rules

> Experimental semantic implementation. Accepted specs and shared fixtures,
> not Rust source, define the bounded behavior under evaluation.

@AGENTS.md
@.agents/multi-language.md

## Current scope

The Gradle project contains `:config` (D0), `:core` (C0-C3), `:llm` (C1/C1b),
`:tools` (C4-C5 and pure C5b-A), `:ledger` (L0), `:skill` (S0), `:memory` (M0),
`:knowledge` (K0), `:scheduler` (Q0), `:multiagent` (MA0),
`:goal` (G1 portable values and lifecycle),
`:persistence-postgres` (L1),
`:adapter-openai-responses`, `:adapter-anthropic-messages`, `:proto`, and an
experimental `:server-host` composition
fixture. Passing these modules proves only the declared conformance dimension.

```text
experiments/engine-kt/
├── settings.gradle.kts
├── build.gradle.kts
├── gradle.properties
├── gradle/wrapper/gradle-wrapper.properties
├── config/               experimental D0 definition/snapshot contract
├── core/                 experimental C0-C3 domain + shared fixtures
├── llm/                  experimental C1/C1b model contract
├── tools/                experimental C4-C5 plus C5b-A planner
├── ledger/               experimental L0 durable fact semantics
├── skill/                experimental S0 activation contract
├── memory/               experimental M0 value/reduction contract
├── knowledge/            experimental K0 retrieval contract
├── scheduler/            experimental Q0 recurrence contract
├── multiagent/           experimental MA0 delegation contract
├── goal/                 experimental G1 Goal values and lifecycle reducer
├── persistence-postgres/ PostgreSQL portability experiment
├── adapter-openai-responses/    provider-independent Responses protocol
├── adapter-anthropic-messages/  provider-independent Messages protocol
├── proto/                generated admitted wire bindings
└── server-host/          executable experiment fixture, not product Runtime
```

Empty historical module directories are not architectural commitments. Add a
Gradle module only with an admitted experiment boundary and executable test.

## Module admission

Before adding `:<module>`:

1. Link the product/research requirement.
2. Name the accepted Garive behavior being implemented.
3. Define wire, semantic, or capability conformance.
4. Add executable Kotlin tests and a build command.
5. Record its experimental scope and explicit non-claims.

## Build

```text
cd experiments/engine-kt
./gradlew projects
./gradlew build
```

The committed wrapper pins the Gradle distribution. Repository gates must not
depend on whichever Gradle version happens to be installed globally.

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
- Do not claim conformance for a D0/C0-C5/C5b-A/G1 change until both declared fixture suites
  pass; Rust remains free to evolve production-only slices outside that matrix.
- Do not use generated proto values as the entire internal domain model.
- Do not claim production parity, Runtime ownership, or product support from
  this tree.
- Do not substitute an embedded database for PostgreSQL integration evidence.
