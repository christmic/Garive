# Testing and evidence

> Test the cheapest boundary that can disprove a claim. Numeric gates become
> constitutional only after a reproducible baseline exists.

## Evidence maturity

| Level | Meaning | May block landing? |
|---|---|---|
| Proposed | Test idea or target with no executable harness. | No. |
| Executable | Runs locally and fails on a demonstrated defect. | Yes, for its scoped slice. |
| Baseline | Repeated on pinned inputs/environment with stored results. | Yes, after variance is known. |
| Gate | Threshold and regression policy accepted from baseline evidence. | Yes. |

Design documents may contain proposed metrics. They must label them as
provisional until the harness reaches Baseline.

## Test categories

| Category | Purpose | Typical cadence |
|---|---|---|
| Static | Format, lint, docs, dependency and architecture checks. | Every change. |
| Unit | One behavior or invariant with controlled dependencies. | Every change. |
| Property/fuzz | State-machine invariants and hostile decoder/tool input. | Property per change; fuzz scheduled. |
| Contract | One admitted API, persistence, provider, or execution boundary. | Every boundary change. |
| Integration | Multiple owned modules with real ephemeral infrastructure where useful. | Every relevant change. |
| Conformance | Declared wire/semantic/capability equivalence across implementations. | Only when multiple implementations are admitted. |
| End-to-end | Product boot and one representative workflow through real boundaries. | Nightly/release after a runnable product exists. |
| Capability benchmark | Agent outcome quality on pinned public or licensed corpora. | Baseline and release programs. |

This is a taxonomy, not a requirement to create every directory before code
exists.

## Current repository gate

The current executable Rust scaffold must pass:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The Kotlin experiment currently proves configuration separately:

```text
cd experiments/engine-kt
gradle projects
```

`gradle build` becomes a repository gate after a complete toolchain/wrapper and
an executable Kotlin slice are present.

## Unit and property rules

- Write the test before implementation and observe a real failure.
- Commit green states; do not preserve uncompilable TDD checkpoints in trunk
  history.
- Name behavior, not implementation detail.
- One assertion concept per test; related field assertions may stay together.
- Use properties for declared invariants, not to generate random examples with
  no oracle.
- A failing test is fixed or intentionally removed with the contract change;
  it is not skipped to make CI green.

## Boundary contract rules

| Boundary | Required evidence |
|---|---|
| Provider adapter | Pinned official schema/SDK paths, captured safe fixtures, parser/stream terminals, usage normalization. |
| Public API | Schema compatibility, validation, redaction, negotiation, and unknown-field/version policy. |
| Persistence | Migration, crash boundary, transaction atomicity, backup/restore, and corruption behavior. |
| External effect | Stable invocation identity, authorization binding, receipt durability, retry class, and uncertain-state terminal. |
| Cross-language wire | Decode/encode compatibility; byte equality only for declared canonical bytes. |

Protobuf decode/re-encode byte equality is not a universal compatibility
oracle. Unknown fields, map ordering, and language encoders require explicit
policies. Prefer semantic equality unless canonical bytes are the contract.

## Crash and recovery testing

Durable behavior requires fault injection at each boundary:

```text
fact prepared
fact committed
external operation started
external operation committed
receipt persisted
terminal persisted
client terminal published
```

For every interruption point, assert whether recovery may retry, must recover
from a receipt, or must require operator reconciliation. Never use “unpaired
call” alone as evidence that an external effect did not happen.

## Derive and assemble research

The detailed designs remain in:

- `docs/architecture/core/derive-testing.md`
- `docs/architecture/core/assemble-testing.md`

Their golden, property, retention, provider-smoke, and prefix-stability ideas
are useful. Their current latency, ratio, corpus-size, and retention numbers
are Proposed. Promote a number to Gate only after recording:

1. pinned hardware/runtime/provider coordinates;
2. input corpus identity and generator revision;
3. repeated baseline distribution;
4. acceptable variance and regression margin;
5. cost and cadence.

## Capability benchmarks

Public Agent benchmarks measure one axis, not total product correctness.

- Pin source dataset, environment, Agent revision, model coordinates, adapter,
  runner revision, and budgets.
- Keep infrastructure failure separate from Agent underperformance.
- Compare Agent and model changes as separate axes.
- Preserve failure-bearing runs; do not publish only successful samples.
- Add `bench/` implementation only as its executable adapter lands.

## Fixtures

- A fixture is authoritative only for the contract that names it.
- Provider fixtures come from verified official shapes with secrets removed.
- Behavioral fixtures describe input, expected semantic output, and failure
  terminal without importing implementation details.
- Updating a fixture and implementation together requires explaining which
  accepted behavior changed.

## Anti-patterns

- A test command that only prints “not wired” but is reported as passing.
- A numeric SLO invented before a baseline.
- Requiring every test category for an empty placeholder module.
- Byte-diffing non-canonical data.
- Treating two equally wrong implementations as conformance evidence.
- Blindly replaying an uncertain external effect in a recovery test.

## Reference

- `.agents/engineering-rules.md` — truth and verification requirements.
- `.agents/ddd.md` — test-first, green-commit workflow.
- `.agents/multi-language.md` — conformance levels.
- `docs/architecture/core/README.md` — active design maturity.
