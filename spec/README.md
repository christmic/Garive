# spec/

> **落地规范 + 共享 wire schemas.** Anything that Rust, Kotlin,
> Go, Swift, and TypeScript all need to agree on — and that
> has to be implemented faithfully, not just thought about.
>
> This is the **normative** layer. If it lives in `spec/`, it
> is meant to be implemented; if it lives in `docs/`, it is
> meant to be **discussed, designed, and explored**.

## The Garive Doc Hierarchy

```
design (docs/)   →   spec (here)   →   agents (constitution)   →   tier AGENTS   →   code
  natural lang.       normative contract    rules that apply    tier-specific overrides
  human-edited        machine-checked       to every tier       of the constitution
```

`spec/` is **stage 2**. By the time something lands here, it
has already been through a design doc in `docs/` and survived
review. `spec/` content is what agents read when they implement
a slice.

## Hard Rule: Living Specification

> **Spec only what we are actually about to implement.**

The fastest way to ruin `spec/` is to write down the
"complete future contract" — every possible variant, every
edge case, every future API. It looks thorough; it is rot.

Two rules:

1. **No speculative spec.** A spec doc lands when there is a
   slice scheduled to implement it. If the slice is not on
   a worktree branch within ~30 days, the spec is deleted
   or downgraded back to `docs/`.
2. **Spec deepens as it lands.** The first cut is the
   "what + a sketch of how." Type references and invariants
   fill in as the slice progresses. Wire fields pin to
   `.proto` tags once they're encoded. Don't pre-write what
   won't be needed yet.

## What Goes Here

| Subdir / file | Role |
|---|---|
| `proto/` | Wire schemas. Single source of truth for all generated bindings (Rust + Kotlin). |
| `fixtures/` | Test data consumed by the cross-language conformance suite. |
| `design/` | Cross-language protocol specs and invariants — **prose** that names a contract and points at the `.proto` field that enforces it. |

A spec document is **short** — one slice, one concern. It does
not duplicate `.proto` (which is the type source) and does not
duplicate `.agents/ddd.md` (which is the domain methodology).
It references both.

## Spec-Doc Checklist

Every spec doc must answer these, in order. Each answer is
short. If a section is empty, the slice probably isn't ready
to be speced yet.

```
# <Slice name>

## Responsibility (one line)

> What this slice owns. What it does NOT own.

## Does / Does NOT

- **Does:** 2–4 bullets naming the capabilities in plain
  English.
- **Does NOT:** 2–4 bullets naming the things that look
  related but live elsewhere. These are the cross-cutting
  contracts — when in doubt, name them here so the boundary
  is explicit.

## Interface (sketch)

The protocol-level interface — not function signatures,
but the verbs the slice exposes and what flows across them.
For wire-touching slices, this section names the `.proto`
methods / messages involved.

## Types (by reference)

> Type definitions live in `spec/proto/*.proto` (single source).

This section **names** the messages / value types the slice
uses. No type definitions here — just names, with the
`.proto` path. The implementation MUST consume the generated
bindings.

## Invariants (must hold)

The properties the slice guarantees. Examples:
- "Total turn count never exceeds `max_turns`."
- "Each domain event has exactly one producer."
- "`AgentState` transitions are total — every state has at
  least one outgoing edge."

Invariants must be **testable**. Each invariant here MUST
have at least one property test in the relevant tier (see
`.agents/testing.md`).

## Architecture (record)

A short paragraph recording how this slice fits into the
system. What does it consume? What consumes it? Why does it
live in this tier? Cross-link the relevant design doc in
`docs/` if one exists.

## Error boundaries

> How this slice fails — and what it does NOT do when it
> fails.

- The errors this slice surfaces (named types, message
  conventions, exit codes).
- The errors it **propagates from below** without
  re-wrapping.
- The errors it never sees because they are caught by a
  higher layer.

## Acceptance

> What must be true for the slice to count as "done".

A short checklist of observable conditions:
- [ ] `spec/proto/*.proto` round-trips through every tier
      (`just conformance` empty diff).
- [ ] Unit + property tests for invariants land in
      `<slice>/tests/` (Rust) / `<slice>/src/test/` (Kotlin) /
      `<slice>/*_test.go` (Go).
- [ ] Integration test exercising the slice's wiring with at
      least one neighbour lands in `<slice>/tests-integration/`.
- [ ] Docs that referenced this slice get a forward pointer
      if behaviour changed.
- [ ] `git log -- spec/proto/ <slice>/` is one coherent
      story (small, atomic commits; no "oh I forgot the tests"
      follow-up commits).
```

A spec is **complete** when every section above is filled in
short. If any section needs paragraphs of explanation, push
the explanation back to a `docs/` design doc and link it.

## What Does NOT Go Here

- Free-form thinking, exploratory sketches, ADRs-in-progress.
  Those belong in `docs/`.
- Per-feature designs. Those belong in `docs/architecture/`.
- API references or tutorials. Those belong in `docs/`.
- **Anything that does not have a slice scheduled to implement
  it.** Spec is for slices that are about to be built.

## Cross-language Sync Lock

- `proto/` is the **source**. Rust types are generated into
  `engine/proto/` via `build.rs` + `prost-build`; Kotlin types
  are generated into `engine-kt/proto/` and `mobile/` via the
  Gradle protobuf plugin.
- `fixtures/` drives the conformance target. Both languages
  consume the same fixtures; `just conformance` diffs the
  outputs. An empty diff = the wire shape has not drifted.
- Hand-edits to generated code in either language are forbidden —
  change the `.proto`, regenerate.

## Convention

- English for all technical writing.
- Reference, don't duplicate. Type definitions are in `.proto`;
  domain methodology is in `.agents/ddd.md`; per-tier rules are
  in `<tier>/AGENTS.md`. A spec doc that restates any of those
  is doing it wrong.
- When the implementation changes behaviour that isn't yet
  captured in spec — add it to spec, don't leave it implicit.