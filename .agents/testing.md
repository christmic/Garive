# Testing

> **The Garive test pyramid.** Eight layers, each with a
> specific scope, language binding, purpose, and tooling.
> Aggregated and enforced by the tier-specific AGENTS.md
> files; this document is the **single source of truth** for
> "what kind of test goes where".

## The Pyramid (bottom-up — most tests at the base)

```
                          ▲
                         E2E                whole app, real runtimes
                        ────
                       Agent / SWE         bench/ harness + official eval
                      ───────────
                     Cross-language        conformance lock + fixtures
                    ───────────────
                   Contract              type consistency, protocol no-drift
                  ────────────────
                 Integration             multi-crate/module, may be stateful
                ────────────────
               Property / Fuzz          invariants + decoder robustness
              ───────────────
             Unit                       pure functions, single behaviour
            ───────────────
           Static                       lint / format / style
          ───────────────
```

The lower the layer, the cheaper it is to run, and the more
of them you should have. The upper layers exist to validate
the whole; the lower layers exist to pin down the parts.

## The Eight Layers

### 1. Static — `style / form`

**Scope:** a file at a time. No runtime.

**Per-language tools:**

| Tier | Tools |
|------|-------|
| Rust | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc --no-deps -- -D warnings` |
| Kotlin | `ktlint`, `detekt`, `kotlinc -Xlint:all` |
| TypeScript / React | `eslint`, `tsc --noEmit`, `prettier --check` |
| Go | `gofmt -l`, `go vet ./...`, `golangci-lint run` |
| Swift | `swift format`, `swiftlint` |

**Cadence:** every CI run, every commit. Fail-fast.

**Anti-pattern:** style rules with auto-fix exceptions. If
the tool has a `--fix` flag, use it; never commit-by-hand
"this one I had to keep."

### 2. Unit — `single behaviour`

**Scope:** one function / type / invariant. No I/O, no
network, no filesystem state outside `tempdir`. Pure where
possible.

**Per-language tools:**

| Tier | Tools |
|------|-------|
| Rust | `cargo test` (built-in); `proptest` for property tests; `rstest` for parameterised |
| Kotlin | `kotlin.test`; `kotest`; `kotest-property` for properties |
| TypeScript | `vitest` / `jest`; `fast-check` for properties |
| Go | built-in `testing`; `testing/quickcheck` |
| Swift | `XCTest`; `swift-check-property` |

**Patterns:**

- **TDD (Red → Green → Refactor).** First commit on a slice
  is a failing test. `.agents/ddd.md` rule.
- **BDD-style naming for behaviour, not implementation:**
  `agent_loop_stops_after_max_turns`, not `test_loop_v2`.
- **One assertion concept per test.** Multiple `assert_eq!`
  on related fields are fine; multiple unrelated behaviours in
  one test are not.
- **Properties for invariants.** Aggregate invariants (e.g.
  "turn count never exceeds `max_turns`") get a `proptest` /
  `kotest-property` / `fast-check` suite — not just examples.

**Cadence:** every slice, every change. The unit suite is
**the** gate before `git rebase origin/master`.

**Anti-pattern:** `#[ignore]`-style skips to make CI green.
Don't skip a failing test; fix the code. If a test is flaky,
quarantine it (move to a `flaky/` dir) and file an issue.

### 3. Property / Fuzz — `invariant robustness`

**Scope:** a single decoder / parser / state machine. **Adversarial
input**, often generated automatically. Catches the class of
bugs example-based unit tests miss: off-by-one boundaries,
empty input, very long input, invalid UTF-8, partial UTF-8,
malformed wire bytes, length-prefix mismatches, adversarial
LLM tool calls.

**Tools per language:**

| Tier | Tools |
|------|-------|
| Rust | `cargo-fuzz` (libFuzzer) for proto decoders; `proptest` for declared invariants |
| Kotlin | `jazzer` (libFuzzer binding); `kotest-property` for declared invariants |
| Go | native `testing.F` (Go 1.18+); `go-fuzz` (legacy) |
| TypeScript | `fast-check` for declared invariants; `jsfuzz` for byte-level fuzzing |

**Mandatory fuzz targets:**

| Target | What it does |
|--------|--------------|
| `proto_decode_*` | Random bytes → `Message::decode(&[u8])` → must not panic, must produce a sensible error. |
| `json_canonical_*` | Random JSON → canonical round-trip → diff-able output. |
| `wire_diff_apply` | Random unified diff → `git apply`-style application → must reject malformed without crash. |

**Cadence:** nightly in CI on a self-hosted runner (fuzz
needs wall-clock time). Block-release: any fuzz target that
finds a panic → fix + add a regression test in the unit suite.

**Anti-pattern:** a fuzz target that runs for 5 seconds and
calls it done. Fuzz needs at least minutes-to-hours per run
to be meaningful. Configurable, not arbitrary.

### 4. Contract — `type consistency, protocol no-drift`

**Scope:** the **wire schema** between producer and consumer
across languages / process boundaries.

**Where it lives:**

- Rust: `engine/proto/tests/contract.rs` — round-trip every
  message in `spec/proto/*.proto` (encode → decode →
  re-encode → bytes match). Run as part of `cargo test`.
- Kotlin: `engine-kt/proto/src/test/...` — same round-trip.
- Go: `runtime/gateway/*_test.go` — same round-trip.
- TypeScript: `desktop/frontend/src/ipc/__tests__/...` —
  typed wrappers around `@tauri-apps/api` round-trip with
  generated bindings.

**The contract test gate:** every `.proto` change passes all
round-trip tests in **all** languages, AND `just conformance`
returns empty diff. Either failing means drift.

**Anti-pattern:** "we'll just test the Rust side." That is
not a contract test. The contract test is **the whole reason**
`.agents/multi-language.md` exists — wire shape is language-
agnostic, drift detection is cross-language.

### 5. Cross-language — `conformance lock`

**Scope:** one wire scenario, two implementations, identical
output.

**How it works:** `just conformance` reads every `*.json` in
`spec/fixtures/`, runs each through Rust + Kotlin, writes
canonical JSON, diffs the two outputs. Empty diff = sync held.

This is **the** sync lock between `engine/` and `engine-kt/`.
Failures are not "test flakes" — they are domain drift.

**Anti-pattern:** editing a fixture to make a failing diff
go away. The fixture is the contract.

### 6. Integration — `multi-crate / multi-module, may be stateful`

**Scope:** multiple crates / modules of **the same language**
together. May use a real database (testcontainers), real
filesystem, real clock (frozen), real wire over `127.0.0.1`.

**Where it lives:**

| Tier | Location |
|------|----------|
| Rust | `engine/<crate>/tests-integration/`, or `tests/integration/` for cross-crate flows |
| Kotlin | `engine-kt/<module>/src/test-integration/kotlin/` |
| Go | `runtime/gateway/*_integration_test.go` |
| TypeScript | `desktop/frontend/src/__integration__/` |

**Rules:**

- Tests may be stateful (a real database, a real message
  queue). Use ephemeral resources (testcontainers, tmp dir).
- Tests **must not** be flaky in CI. If a test is flaky, fix
  the underlying race / timing assumption.
- Tests cover the **wiring** between modules, not the
  business logic in any one of them (that's the unit suite).

**Cadence:** every PR. CI parallel pool, `cargo nextest` /
`gradle test --tests "*"` / `go test ./...` etc.

### 7. E2E — `whole app, real runtimes`

**Scope:** the whole stack. Real replica + gateway + a
desktop/mobile build. Smoke, not coverage.

**Where it lives:** `tests/e2e/` at the repo root. Run via the
release / nightly pipelines (not per-PR — too slow).

**What it covers:**

- `engine` + `runtime/replica` start, accept traffic, persist
  state.
- `runtime/gateway` rate-limits, routes, logs.
- A Tauri desktop build launches and reaches the running
  gateway.
- A Kotlin/mobile build launches and reaches the running
  gateway.

**What it does NOT cover:** business correctness (covered by
unit + integration + bench). E2E is for "does it boot and
talk to itself."

### 8. Agent / SWE — `capability benchmark`

**Scope:** Garive's agent capability, measured objectively.

**How it works:** `bench/` orchestrates SWE-bench (Verified /
Lite / Multimodal / Multilingual) and Terminal-Bench against
the agent under test, using the **official eval scripts**. No
hand-rolled cases. Score history lives in
`bench/tracking/versions/`.

**The agent-level test is also a regression test.** A drop in
score on the same source × env × adapter triple is a regression.

See `bench/AGENTS.md` for the full rules and `bench/` for the
mechanism.

## Layer-to-Language Matrix

| Layer | Rust | Kotlin | Go | TypeScript | Cross-lang |
|-------|------|--------|----|-----------|-----------|
| Static | ✓ | ✓ | ✓ | ✓ | — |
| Unit | ✓ | ✓ | ✓ | ✓ | — |
| Property / Fuzz | ✓ | ✓ | ✓ | ✓ | — |
| Contract | ✓ | ✓ | ✓ | ✓ | — |
| Cross-language | — | — | — | — | ✓ (`just conformance`) |
| Integration | ✓ | ✓ | ✓ | ✓ | — |
| E2E | ✓ | ✓ | ✓ | ✓ | ✓ (`tests/e2e/`) |
| Agent / SWE | ✓ | ✓ | — | — | ✓ (`bench/`) |
| **Derive 4-dim** (see `docs/architecture/core/derive-testing.md`) | ✓ | ✓ | — | — | — |

## CI / Pipeline Gating

| Pipeline | Layers that gate |
|----------|-------------------|
| Per-PR | Static, Unit, Property, Contract, Integration |
| Nightly | Cross-language (`just conformance`) + Fuzz + E2E + Agent (SWE smoke subset) |
| Release | All eight, plus full Agent / SWE on `official` env |

A change to `spec/proto/*` must re-run the **Cross-language**
and **Contract** layers. A change to an engine aggregate
must re-run **Unit** + **Property** + **Integration**. A
release must run **all eight** on a self-hosted runner with
the official bench harness.

## Derive 4-dimension Test Contract (constitutional reference)

`derive` is a **pure function** — `(current_surface, new_entries,
masking_timeline, projection_args) → derived_surface` (see
`loop.md` "Derive in Detail"). Pure functions admit a strong
test contract; the suite must cover **four dimensions**:

1. **Golden snapshot regression** — known ledgers, expected
   surfaces, byte-exact match.
2. **Property tests** — random inputs, invariant assertions;
   includes the **instruction interaction matrix**.
3. **Fact retention rate** — model-in-the-loop pipeline test;
   forces compression + derive + LLM recall.
4. **Cross-language equivalence** — Rust + Kotlin produce
   byte-equal surfaces for the same input.

The **detailed design** (fixtures, harness, probes,
invariants, cadence) lives in the design doc:

> **`docs/architecture/core/derive-testing.md`** — design of
> the four test dimensions.

This file is the **constitutional rule** that the four
dimensions exist; the design doc is the **implementation
plan**. A regression in any dimension is load-bearing.

## What This Means for Each Sub-project

| Sub-project | Tests to write |
|-------------|-----------------|
| `engine/<crate>/` | Unit + Property + Integration (when crate is non-trivial); Contract in `engine/proto/` |
| `engine/proto/` | Contract for every `.proto` message; Fuzz targets for every decoder |
| `engine-kt/<module>/` | Unit + Property + Integration; Contract + Fuzz mirror in `engine-kt/proto/` |
| `runtime/replica/` | Unit + Integration (real wire) |
| `runtime/gateway/` | Unit + Fuzz + Integration; Contract via generated Go bindings |
| `mobile/` | Unit (KMP shared) + UI tests per platform; Snapshot (paparazzi) |
| `desktop/backend/` | Unit + Integration (Tauri commands) |
| `desktop/frontend/` | Unit + Component + E2E (Playwright) |
| `desktop/macos-native/` | Unit + UI test (XCUITest) |
| `bench/` | The whole `bench/` IS the Agent / SWE layer |
| `tests/` (root) | Cross-tier Integration + E2E |

## What NOT to Do

- ❌ Don't skip the unit suite to land a slice. Red first.
- ❌ Don't trust the cross-language conformance diff. Empty
  diff only.
- ❌ Don't put cross-language tests in a per-language unit
  suite. They live in `tests/conformance/` and run via
  `just conformance`.
- ❌ Don't put a fuzz target that "passes" without running
  for any meaningful time. Fuzz needs minutes.
- ❌ Don't write a fixture that depends on the implementation.
  The fixture is the contract; the implementation serves it.
- ❌ Don't add layers. Eight is the number. If something
  doesn't fit one of them, the answer is "fit it" — not
  "add a ninth".