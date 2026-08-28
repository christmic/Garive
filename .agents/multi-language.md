# Multi-Language Isomorphic Design

> How Garive keeps Rust + Kotlin (and future languages) in
> lock-step **without "writing two sides"**.
>
> The mechanism: **single source + code generation +
> consistency tests**. There is no second author; there is one
> author of intent, and languages consume the same artifact.

## The Core Principle

| Layer | Authoring model |
|-------|-----------------|
| **Design** (architecture, mechanisms, invariants) | One doc. Both languages read it. |
| **Data types** (messages, value objects) | One schema. Generated into both languages. |
| **Protocol** (wire shape, error codes, sequence) | One proto. Generated into both languages. |
| **Test fixtures** (`spec/fixtures/*`) | One file. The conformance runner reads it from both. |
| **Cross-language conformance** | One script (`just conformance`). Diff must be empty. |
| **Core logic** (agent loop, scheduler, dispatch) | **Re-written per language.** Different framework idioms. |
| **Framework integration** (FFI, SKIE bridge, env handling) | Per language. |
| **Per-platform UI** | Per platform (Compose, SwiftUI). |

The first six rows are **not** written twice. The last three
**must** be — because the frameworks are different. We
**never** transcribe the Rust core to Kotlin, or vice versa.
We re-author from the same source of intent in each language's
idioms.

## What "Mirror" Means (and Doesn't)

When we say `experiments/engine-kt/` is a Kotlin mirror of
`engine/`, we mean **the semantics are identical**, not that
the code is. Three things are kept identical:

1. **Domain semantics.** Same aggregate boundaries, same
   invariants, same domain rules. These come from
   `.agents/ddd.md` + `spec/proto/`.
2. **Wire shape.** Same proto, same generated types.
3. **Test expectations.** Same fixtures, same canonical JSON
   output under conformance.

Everything else is **idiomatic to the language**:

| Concern | Rust | Kotlin |
|---------|------|--------|
| Async | `tokio::future`, `async fn` | `suspend fun`, `Flow` |
| Errors | `thiserror` enums | sealed classes / `Result` |
| Type wrappers | newtype structs | value classes |
| Module organisation | one crate per sub-dir | one Gradle module per sub-dir |
| Test framework | `#[test]`, `proptest` | `kotlin.test`, `kotest-property` |

A new contributor to `engine-kt/` should:

1. Read `engine/AGENTS.md` and `experiments/engine-kt/AGENTS.md`.
2. Read the relevant `.agents/` rule files.
3. **Not** read the Rust code first. Read the proto + fixtures
   + design doc, then implement from those in Kotlin idioms.

If the Kotlin code starts looking like a Rust translation,
that's a smell — push back and rewrite in Kotlin idioms.

## The Mechanism (Three Guarantees)

```
                  ┌────────────────────────┐
                  │  spec/proto/*.proto    │   ← single source
                  └────────────┬───────────┘
                               │
         ┌─────────────────────┼─────────────────────┐
         ▼                     ▼                     ▼
   Rust (prost-build)   Kotlin (gradle)      Go (buf gen)   ← codegen
         │                     │                     │
         ▼                     ▼                     ▼
    engine/proto/       engine-kt/proto/       runtime/gateway/   ← generated
         │                     │                     │
         ▼                     ▼                     ▼
    Rust types          Kotlin types            Go types
         │                     │                     │
         └──────────┬──────────┴──────────┬──────────┘
                    │                     │
                    ▼                     ▼
             spec/fixtures/*        spec/fixtures/*    ← shared test data
                    │                     │
                    └──────────┬──────────┘
                               ▼
                  just conformance        ← consistency check
                  diff must be empty
```

Three guarantees keep this honest:

1. **Single source.** Anything that's wire-shaped lives in
   `spec/proto/`. Hand-written parallel types are banned in
   `engine/`, `engine-kt/`, `mobile/`, `runtime/gateway/`,
   `desktop/`. See `spec/AGENTS.md`.
2. **Generated, not transcribed.** Code generation happens at
   build time. CI fails if generated files drift from source
   (`.agents/git-workflow.md` Before-Rebase Checklist).
3. **Conformance gates sync.** `just conformance` reads the
   same fixture from both languages and diffs the canonical
   JSON. Empty diff = sync held.

## The Sync Loop

When a slice changes in `engine/` (Rust):

```
1. Update spec/proto/*.proto   if the wire shape moves
2. Update spec/fixtures/*       if the inputs / expected outputs move
3. Update Rust impl + Rust-specific tests
4. Run just conformance — confirm the Rust side alone is green
5. Sync the Kotlin mirror:
   a. Pull the new proto + fixture
   b. Re-author the Kotlin side from those (NOT a port of Rust code)
   c. Re-run conformance — confirm both sides agree
6. Both sides commit in lock-step (or two adjacent commits;
   not "Rust first, Kotlin later, oh wait we forgot")
```

The same loop applies in reverse when `engine-kt/` leads.
The Rust side stays as the canonical implementer for
performance-sensitive paths; the Kotlin mirror exists to
prove the abstractions are language-agnostic and to serve
the JVM-side runtime when needed.

## What This Means for Future Languages

Adding a new wire language (Swift, TypeScript, Python, …)
**does not** mean writing Garive again. It means:

1. Wire up `protoc-gen-<lang>` in the codegen step.
2. Add the generated bindings to the language's package
   manager.
3. The conformance runner gains a third participant.
4. New language's harness slots into the driver loop (no
   driver changes).

The "platform-specific code" (loop logic, scheduler, …)
still has to be authored per language — but the **contract**
is the one source.

## What NOT to Do

- ❌ Don't hand-write types in `engine/`, `engine-kt/`,
  `mobile/`, `runtime/gateway/` that mirror `.proto` fields.
  Generate or import.
- ❌ Don't transcribe Rust code to Kotlin. Re-author in Kotlin
  idioms from the spec / fixtures / design doc.
- ❌ Don't let one side drift. If conformance fails, fix
  the implementation; never edit the fixture to make the
  diff go away.
- ❌ Don't skip conformance. `just conformance` is the gate
  for any commit touching `spec/proto/`, `engine/proto/`,
  `engine-kt/proto/`, or `mobile/`.
- ❌ Don't re-design semantics separately per language.
  Aggregate boundaries, invariants, domain events all come
  from `.agents/ddd.md` and apply uniformly.