# Multi-language admission and conformance

> Rust is Garive's initial implementation. Another language is admitted for a
> concrete product or research need, not to prove every abstraction twice.

## Current position

Kotlin is an experimental semantic implementation for admitted portable slices
D0 and C0-C5. It evaluates portability through accepted specs and shared
fixtures; it is not a supported product server, a Runtime, or a promise that
every Rust module has a Kotlin counterpart.

The sources of truth are boundary-specific:

| Concern | Source |
|---|---|
| Product/module ownership | `docs/architecture/system.md` |
| Internal Rust behavior | Rust types, tests, and accepted design |
| Public/cross-process wire shape | the admitted file under `spec/` |
| Provider wire shape | pinned official documentation/SDK evidence |
| Cross-language behavior | shared acceptance fixture for the admitted slice |

## Admission gate

A second implementation needs all of:

1. A user or deployment requirement that Rust alone cannot satisfy well.
2. A bounded behavior surface small enough to specify independently.
3. Shared acceptance examples that do not depend on either implementation.
4. An owner and build pipeline for the new implementation.
5. A decision on whether it is experimental, supported, or production.

Until then, keep the idea in docs and do not create placeholder modules.

## What is shared

| Artifact | Sharing rule |
|---|---|
| Public wire schema | Generated per language from one admitted schema. |
| Canonical wire fixture | Same bytes or canonical JSON when byte identity is part of the contract. |
| Behavioral fixture | Same inputs and semantic outputs; language representation may differ. |
| Internal domain types | Authored per implementation; no forced proto mirror. |
| Algorithms | Re-authored from accepted behavior, not transcribed line by line. |
| Platform integration | Native to the platform. |

## Conformance levels

| Level | Assertion | Use |
|---|---|---|
| Wire | Decode/encode compatibility and unknown-field policy. | Cross-process schema. |
| Canonical representation | Byte-equal canonical output. | Hashes, signatures, cache keys, golden wire fixtures. |
| Semantic | Equivalent domain result after normalization. | Independent Agent or client implementations. |
| Capability | Both satisfy the same end-to-end scenario and failure contract. | Production support claim. |

Do not require byte equality for values whose map ordering, serialization, or
language representation is not itself a contract.

## Kotlin implementation

The Gradle tree contains experimental `:config` (D0), `:core` (C0-C3), `:llm`
(C1/C1b), and `:tools` (C4-C5) modules plus `:proto`. The exact conformance matrix
lives in `spec/design/cross-language-agent-contract.md`.

For every admitted joint slice:

1. Read the accepted design and fixtures before reading Rust implementation
   details.
2. Generate boundary bindings where a schema exists.
3. Implement Kotlin-idiomatic domain values behind the boundary.
4. Run the declared conformance level.
5. Report Rust and Kotlin evidence separately; matching failures are still two
   failures, not proof of correctness.

## Generated code policy

- Generated bindings are build artifacts unless a release/distribution reason
  explicitly requires committing them.
- CI regenerates and compares committed artifacts only when the project has
  chosen to track generated output.
- Handwritten types may map to generated wire values; they must not silently
  redefine an admitted public schema.

## Anti-patterns

- Calling two implementations “two sources of truth.”
- Blocking an initial Rust slice on a Kotlin placeholder.
- Using proto for every internal value to avoid writing mappings.
- Comparing arbitrary JSON bytes when semantic equivalence is the contract.
- Editing a shared fixture to match one implementation without revisiting the
  accepted behavior.
- Adding a language because its directory already exists.

## Conformance merge gate

When a change claims Rust/Kotlin conformance for an admitted slice, it includes
the spec, shared fixtures, both implementations/tests, and green `just
conformance`. Production Rust changes outside the admitted experimental matrix
do not wait on Kotlin placeholders.

## Reference

- `docs/architecture/system.md` — technology admission and ownership.
- `.agents/ddd.md` — domain/wire separation.
- `.agents/testing.md` — conformance cadence and evidence maturity.
- `experiments/engine-kt/AGENTS.md` — Kotlin experiment rules.
- `spec/design/cross-language-agent-contract.md` — support matrix and fixtures.
