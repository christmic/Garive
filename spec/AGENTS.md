# spec/AGENTS.md

> Normative contracts that cross a real process, storage, or language
> boundary. Internal domain types do not belong here by default.

This file applies to everything under `spec/` and refines the root rules.

@AGENTS.md
@.agents/multi-language.md

## Ownership

- `spec/proto/` owns protobuf wire schemas for boundaries that are actually
  implemented by more than one component or persisted independently.
- `spec/fixtures/` owns shared, versioned conformance inputs once an executable
  conformance harness exists.
- `spec/design/` may hold concise normative decisions. Exploratory reasoning
  remains in `docs/`.
- An internal Rust struct is owned by its Rust module. Do not introduce proto
  merely to make the repository look language-neutral.

## Schema discipline

- Use a versioned package such as `garive.v1` for a shipped wire boundary.
- Once a field/tag has shipped, do not reuse its tag. Reserve removed tags and
  document compatibility behavior.
- Generated bindings are outputs; change the schema and generator, not the
  generated file.
- State the producer, consumer, compatibility promise, and canonicalization
  rules beside each contract.

## Evidence and conformance

Conformance level is chosen per boundary:

1. wire: each consumer can decode/encode the contract;
2. canonical: byte identity only when the encoding is specified as canonical;
3. semantic: implementations produce equivalent normalized outcomes;
4. capability: unsupported features are declared explicitly.

`just conformance` is not a gate until it runs a real harness. A schema change
must run the generator and tests that exist for its current consumers; do not
claim Rust/Kotlin/Go parity before those consumers exist.

## Verification

- Validate protobuf syntax with the generator/toolchain pinned by the slice.
- Run round-trip and compatibility tests beside each real consumer.
- Add property or fuzz testing based on parser risk, not one target per message
  as a directory convention.
