# C4 — Tool resolution and Prepared Call

> Contract for tool/Core implementers and reviewers defining how untrusted
> model tool output becomes a validated, immutable, authority-free Prepared
> Call with a cross-language stable digest.

## Audience

Engineers implementing `engine/tools`, the Kotlin experiment, Core intent
reduction, Runtime authorization input, or conformance fixtures.

## Why

The accepted architecture fixed the ownership boundary but left schema
coverage, canonical numbers, digest fields and stable failures undecided. Those
details must not emerge independently in Rust and Kotlin implementations.

## Status

Accepted implementation contract for C4.

## Boundary

A model `ToolIntent` is untrusted correlation plus proposed JSON. C4 resolves
one exact admitted definition, validates and normalizes the arguments, and
returns an immutable `PreparedToolCall`. It performs no authorization,
invocation-ID allocation, persistence, execution, retry, or recovery.

Producer: Core/tool preparation. Consumers: Core reduction and Runtime C5.
Rust is production-first; Kotlin is an experimental independent implementation
of the same admitted semantics after this Spec is accepted.

## Inputs

```text
ToolIntent {
  model_call_id: ModelCallId
  tool_name: ToolName
  arguments_json: UTF-8 JSON text
}

ToolDefinition {
  name: ToolName
  revision: ToolRevision
  description: non-empty text
  input_schema: PortableToolSchema
  requirements: ExecutionRequirements
  replay_class: ReplayClass
}
```

The execution snapshot supplies a catalog with one definition per tool name.
Name and revision are non-empty opaque strings. Definition revisions are
immutable: the registry rejects reuse of `(name, revision)` for different
schema, requirements, or replay class.

The v1 root schema declares the single type `object`, declares `properties`,
and explicitly declares `additionalProperties` as `false` or as a schema.
Tool arguments are therefore always JSON objects. Nested schemas may use the
admitted value types below. This matches the neutral `ToolIntent` boundary and
prevents an omitted root policy from silently admitting misspelled arguments.

`model_call_id` is non-empty but untrusted. It correlates the later observation
to model output; it is not an idempotency, authorization, or recovery identity.

## Portable Tool Schema v1

V1 is a deterministic JSON Schema 2020-12 subset:

- value keywords: single-string `type`, `enum`, `const`;
- object keywords: `properties`, `required`, `additionalProperties`;
- array keywords: `items`, `minItems`, `maxItems`;
- string keywords: `minLength`, `maxLength`;
- number keywords: `minimum`, `maximum`, `exclusiveMinimum`,
  `exclusiveMaximum`, `multipleOf`;
- composition: `allOf`, `anyOf`, `oneOf`, `not`;
- annotations: `$schema`, `$id`, `title`, `description`, `default`, `examples`,
  `deprecated`, `readOnly`, `writeOnly` (no validation effect).

Admitted `type` strings are `object`, `array`, `string`, `number`, `integer`,
`boolean`, and `null`. `$schema`, when present, is exactly the JSON Schema
2020-12 dialect URI. Schema JSON itself rejects duplicate keys and must satisfy
the same I-JSON numeric/string surface as arguments.

Local/remote `$ref`, `$dynamicRef`, `pattern`, `patternProperties`,
conditionals, unevaluated keywords, content/media assertions, custom format
assertions and unknown assertion keywords are unsupported in v1. A definition
containing one is rejected at catalog construction; it is not ignored.
`format` may be retained as an annotation only and never changes validation
success. Regex validation is deferred until one portable grammar and validator
matrix is separately admitted.

Schemas and arguments must satisfy I-JSON. Numbers are finite IEEE 754 binary64
values and integers used as identities/counts remain in the interoperable
range `[-9007199254740991, 9007199254740991]`, as required by the RFC 8785
canonical surface. An object with omitted
`additionalProperties` permits additional properties as JSON Schema requires;
safety-sensitive definitions should set it to `false`. Defaults are never
inserted. Strings are counted by Unicode scalar values. Numeric assertions use
the JSON mathematical value before RFC 8785 serialization, not a native
integer overflow or locale-sensitive decimal representation.

## Execution requirements

```text
ExecutionRequirements {
  capabilities: unique set canonically ordered as
    FilesystemRead | FilesystemWrite | Process | Network
  max_duration_ms: non-zero u64
  max_output_bytes: non-zero u64
}

ReplayClass = ReadOnly | Idempotent | ReceiptRecoverable | NeverReplay
```

Requirements declare what an executor must prove; they grant nothing. Runtime
may impose stricter limits. A replay class is a claim that Runtime validates
against the selected executor before dispatch. `ReadOnly` requires no write,
process, or network capability. Other cross-field policies are checked when
the catalog is built, not after model output arrives.

## Preparation algorithm

For each intent, in input order:

1. reject an empty model call ID or tool name;
2. resolve exactly one definition by the snapshot's admitted tool name;
3. parse one complete JSON value, rejecting duplicate keys and trailing bytes;
4. validate it against Portable Tool Schema v1, collecting deterministic
   failures sorted by instance JSON Pointer, schema JSON Pointer, then keyword;
5. normalize only representation: recursively apply RFC 8785 property ordering
   (lexicographic UTF-16 code units), preserve array order and values, and
   insert no defaults;
6. build the versioned digest preimage and compute its digest;
7. return the immutable Prepared Call.

No failed preparation reaches an authorization port. Multiple intents are
prepared independently and retain model order; later C5 policy decides whether
one failure becomes an observation or ends/suspends the execution.

## Output and digest

```text
PreparedToolCall {
  model_call_id
  tool_name
  tool_revision
  normalized_arguments
  input_digest
  requirements
  replay_class
}
```

`input_digest` is lowercase SHA-256 over UTF-8 RFC 8785 canonical bytes of:

```json
{
  "contract": "garive.prepared-tool-call",
  "version": 1,
  "tool_name": "...",
  "tool_revision": "...",
  "arguments": {},
  "requirements": {
    "capabilities": [],
    "max_duration_ms": 1,
    "max_output_bytes": 1
  },
  "replay_class": "never_replay"
}
```

`model_call_id` is deliberately excluded: correlation changes must not change
the executable intent. Description, annotations and registry location are also
excluded. Tool name/revision bind their immutable definition. Any executable
argument, requirement, limit, replay class, contract or version change changes
the digest.

## Stable failures

| Code | Meaning |
|---|---|
| `invalid_model_call_id` | Correlation identity is empty. |
| `invalid_tool_name` | Proposed name is empty or malformed. |
| `tool_not_admitted` | Snapshot contains no exact admitted name. |
| `invalid_arguments_json` | JSON is incomplete, duplicate-keyed or malformed. |
| `arguments_schema_mismatch` | One or more schema assertions failed. |
| `invalid_tool_definition` | Definition/schema/cross-field invariant failed. |
| `unsupported_schema_keyword` | Definition uses a non-v1 assertion. |
| `non_canonical_value` | A valid value cannot satisfy canonical digest rules. |

Schema failures expose stable keyword, instance JSON Pointer and schema JSON
Pointer. Messages are diagnostic and are not compatibility keys. Secret/raw
provider data is never included.

## Immutability and compatibility

- Prepared values are not mutable through public APIs.
- Serialization for logs or persistence is not implied by the internal type.
- A replacement proposed by C5 is a new C4 input and produces a new digest.
- Unknown replay classes, requirement capabilities, canonical versions, or
  schema versions fail closed.

## Required acceptance evidence after approval

- shared Rust/Kotlin fixture for unknown tools, malformed/duplicate JSON,
  every schema keyword, deterministic error paths and unsupported keywords;
- canonical vectors proving key-order independence and array-order/value/
  revision/requirement sensitivity;
- property tests for determinism, immutability and “invalid never authorizes”;
- native API/documentation gates and a dependency test proving `tools` imports
  no Runtime, adapter, SQL, HTTP, environment, or executor module.

## See also

- [`agent-definition-snapshot.md`](agent-definition-snapshot.md) — catalog
  admission and immutable revision binding.
- [`governed-effects.md`](governed-effects.md) — authorization and execution
  after successful preparation.
- [`agent-execution-contract.md`](agent-execution-contract.md) — accepted
  Kernel boundary and outcome semantics.
- [`durable-ledger.md`](durable-ledger.md) — separate integer-only durable
  canonical payload contract.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
