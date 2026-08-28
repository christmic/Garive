# Rust/Kotlin Agent conformance contract

## Status

Kotlin is admitted as a supported server implementation for portable C0-C3.
Other Engine capabilities remain unsupported until admitted by an amended
matrix.

Rust remains the production-first implementation. Neither language defines
shared behavior alone: accepted specs plus shared fixtures define it.

## Purpose

Ensure Rust and Kotlin can evolve the same portable Agent semantics without
forcing identical source structure, arbitrary JSON byte equality, or protobuf
domain models.

## Support matrix

| Slice | Rust | Kotlin | Required conformance |
|---|---|---|---|
| C0 execution control | supported | supported | semantic fixtures + native unit tests |
| C1 usage/model outcome | supported | supported | semantic fixtures + native unit tests |
| C1b model request/stream | supported | supported | semantic fixtures + stream invariant tests |
| C2 context derive | supported | supported | semantic fixtures + property tests |
| C3 model-only turn | supported | supported | capability scenarios |
| C4-C7 | Rust planned | unsupported | no parity claim until explicitly admitted |

`unsupported` is a valid explicit capability result. It must not silently fall
back to behavior with different safety semantics.

## Shared source of behavior

Precedence is:

1. `agent-architecture.md`;
2. the focused accepted slice spec;
3. shared fixtures under `spec/fixtures/agent/`;
4. implementation-native tests and types.

If a fixture and spec disagree, implementation stops and the contradiction is
fixed in the spec/fixture together. A fixture is never edited only to make one
implementation green.

## Fixture protocol

Fixtures are UTF-8 JSON documents used as data, not public product wire DTOs.
Every file contains:

```json
{
  "schema_version": 1,
  "contract": "execution-control",
  "cases": [
    {
      "name": "stable-case-name",
      "input": {},
      "operations": [],
      "expected": {}
    }
  ]
}
```

Rules:

- case names are stable and unique within a contract;
- numbers use contract-declared integer bounds; no floating-point comparison;
- maps are semantically compared unless canonical ordering is explicitly part
  of a digest/cache contract;
- unknown enum/operation values fail loudly with case name and path;
- secret/provider raw payloads never appear in fixtures;
- fixture schema changes increment `schema_version` and update both readers in
  the same slice.

## C0 fixture behavior

An execution-control case supplies:

- a non-empty Turn ID and Execution ID;
- starting completed-iteration count;
- non-zero maximum iterations;
- ordered operations (`begin`, `close:<outcome-kind>`).

Expected output records each operation result plus final count/status. There is
no `resume` operation. Continuation is a separate case with the same Turn ID,
new Execution ID, and Runtime-reconstructed starting count.

## C1 fixture behavior

A usage/outcome case supplies known/unknown counts and one model fact envelope.
Expected output records normalized outcome kind, success flag, partial flag,
and checked total (`known value`, `unknown`, or `overflow`). Text/reasoning/tool
items are preserved in order. Cache breakdowns are never double-counted.

## Implementation independence

- Rust uses idiomatic enums/newtypes and Kotlin uses sealed interfaces/data
  classes/value classes.
- Implementations are authored from specs and fixtures, not translated line by
  line from each other.
- Protobuf is used only when the value crosses an admitted wire/persistence
  boundary. C0-C3 semantic fixtures do not require generated domain types.
- Both implementations validate inputs and make invalid states unrepresentable
  where practical.

## Toolchains

- Rust follows the workspace `rust-version` and strict fmt/Clippy/rustdoc gates.
- Kotlin uses the repository Gradle wrapper/plugins. The build toolchain may run
  on JDK 21 while compiling JVM bytecode target 17; both values are explicit.
- Native unit tests and shared-fixture tests run in each language independently.

## Conformance commands

The root gate is `just conformance`:

1. Rust native tests including all shared fixtures;
2. Kotlin `:agent-core:test` and `:llm-contract:test` including the same fixture files;
3. a fixture coverage check proving both runners consumed every declared case.

The gate reports Rust and Kotlin results separately. Success requires both;
matching failures are not conformance.

## Joint iteration rule

For an admitted shared slice, one change set contains:

1. accepted spec change;
2. shared fixture change/addition;
3. Rust implementation/tests;
4. Kotlin implementation/tests;
5. successful root conformance.

A language may temporarily be red while developing on a worktree, but the
slice cannot merge with only one implementation updated. Breaking capability
or semantic changes require a versioned migration or an explicit support-matrix
change reviewed before code.

## Non-conformance

The following do not prove parity:

- both projects compiling;
- matching enum names or directory trees;
- generated protobuf bindings existing;
- byte-equal arbitrary JSON;
- one language executing tests produced from the other language's output;
- a placeholder command returning success.

## Acceptance

C0-C3 are jointly supported only when native Rust and Kotlin tests pass, both
consume the same complete fixture set, `just conformance` invokes both, and the
support matrix matches executable reality.
