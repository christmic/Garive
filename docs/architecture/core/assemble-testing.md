# Assemble 5-dimension Test Contract — design

> `assemble(surface, projection, last_seen_seq?, provider)` is
> the **dialect serialiser** — it takes a `Surface` that
> `derive` already produced and emits the exact bytes a
> specific LLM provider expects to see (see `loop.md`
> "Assemble (per-projection reshape)"). The five
> responsibilities — role mapping, provider translation, layout,
> budget, cache marker — are exercised by **five test
> dimensions** that this document specifies.
>
> The constitutional rule (assemble has 5 dims) lives in
> `.agents/testing.md`. The detailed design is here.

## TL;DR

| Dim | Question it answers | Cadence |
|-----|----------------------|---------|
| 1. **Schema compliance** | Does the assembled payload pass the provider's API without rejection? | Nightly (live API) |
| 2. **Role mapping correctness** | Is every kind → the right provider role, with `tool.call`/`tool.result` paired correctly? | Per-PR + Nightly |
| 3. **Provider dialect** | Do Anthropic / OpenAI / Gemini-specific markers land where they should? | Per-PR |
| 4. **Budget enforcement** | Does the real-token count + tail-drop + output reserve match the budget? | Per-PR |
| 5. **Cache marker byte-stability** | Is the `cache_control` placed on a byte-stable prefix? | Per-PR + Nightly |

The five dimensions **mirror the five responsibilities**.
Each test exercises one responsibility in isolation; together
they cover the full pipeline.

## Dim 1 — Schema compliance (provider accepts the payload)

**The single most important test**: does the assembled
payload actually pass the provider's API without rejection?

The test runs against a real provider API (test key, nightly
cadence). For each `(provider, projection, ledger_fixture)`
triple:

1. Build the surface via `derive`.
2. Call `assemble` to produce the request payload.
3. **POST it to the provider** (test mode / mock / live).
4. Assert the response is `200 OK` and contains a
   `completion` (or the provider's equivalent) field.

A failure here is **load-bearing** — a payload that the
provider rejects is unusable, even if every local dim 2-5
test passes. The bench also captures the rejection reason
(`400 invalid_role`, `413 too_many_tokens`, etc.) for the
bug report.

**Test corpus** — the 1000+ ledgers from
`engine/core/tests/derive_bench/fixtures/generators.py`,
parametrised over `size × branch_density × compression_count`,
run for each provider (`anthropic`, `openai`, `gemini`,
`local`). **1000+ live API calls per nightly run** is
expensive; in CI, the bench uses a **provider stub** that
parses the payload against the provider's OpenAPI schema
(no network). The nightly run uses live API.

**SLO:** 100 % payload acceptance rate (every payload that
passes the local schema check must also pass the live
provider's check). Any rejection is a P1.

## Dim 2 — Role mapping correctness

`assemble` step 1 is the role map. The test asserts:

- `text.user` → provider's `user` role.
- `text.assistant` → `assistant` (Anthropic, OpenAI, Gemini).
- `harness.feature{feature:"skills_catalog"}` →
  Anthropic's `system` array (top-level), OpenAI's
  `developer` role (newer models) or top-level `system` (older
  models), Gemini's `system_instruction`.
- `assistant.tool_call` + `tool.result` → `assistant`
  `tool_use` block (Anthropic) / `assistant` `tool_calls`
  field (OpenAI) / `functionCall` (Gemini) — and the
  `tool.result` is the `user`-role `tool_result` block that
  **immediately follows** the `tool_use` in the same turn
  group.
- `compaction.summary`, `compaction.rewrite`,
  `privacy.redact` — **never** appear in the assembled
  payload (they're masking-instruction rows; `derive` already
  collapsed them into the surface).

The test enumerates every kind × every projection × every
provider and asserts the role is **exactly** the expected one.
A miscount is a **semantic bug** — the model would see
something the runtime didn't intend to send.

**Test corpus** — a fixed table of `(kind, projection,
provider) → expected role`. Every entry checked. Coverage is
the **kind catalog** in `ledger.md` × **the 5 projections**
× **the supported providers**.

**SLO:** 100 % role correctness. Any mismatch is a P1.

## Dim 3 — Provider dialect (Anthropic / OpenAI / Gemini)

Each provider has its own shape. `assemble` carries a
`provider` argument and dispatches:

- **Anthropic** uses a separate top-level `system` array and
  `cache_control` markers on specific blocks. The
  `tools` field is at the top level, not inside the
  message.
- **OpenAI** (newer) folds system into a `developer` role
  inside `messages`. The `tools` field is at the top level.
  Auto-cache is implicit (the first ~1024 tokens).
- **Gemini** uses `system_instruction` and `contents`
  (not `messages`). Function calls are in `functionCall`
  blocks.

The test asserts the **dialect-specific markers** are placed
correctly:

- Anthropic output has `system` array at the top, with each
  system block carrying `cache_control: {type: "ephemeral"}`
  on the **last** system block.
- OpenAI output has `messages[0].role == "developer"` for the
  system content.
- Gemini output has `system_instruction` and `contents` (not
  `messages`).

A failure here is "we sent the wrong shape to the wrong
provider" — the model may still respond (because the API is
lenient), but we're paying for cache misses we shouldn't be.

**SLO:** 100 % dialect correctness per provider. Mismatch is
a P1.

## Dim 4 — Budget enforcement (real-token count + tail drop)

`assemble` step 4 uses the **provider's real tokenizer** to
count the assembled prompt, compares to the budget, and drops
the tail of the `new` part if the real count exceeds the
budget. The output budget is reserved upfront as
`max_tokens` on the request.

The test asserts:

- **Counting accuracy** — assemble's count is within 1 % of
  the provider's billed count (via the response's
  `usage.prompt_tokens`). This is **tighter** than derive's
  T4 (which estimates; assemble must be exact because it
  gates the budget cut).
- **Tail-drop correct** — when the prompt is over budget,
  the dropped entries are the **oldest** of the `new` part
  (the model has already digested the head / middle from
  cache).
- **Output budget reserved** — `max_tokens` on the request is
  ≥ the configured `output_reserve`.

A failure here means: either we're **sending too much**
(over-budget payload → 413 from provider), or we're
**under-spending the cache** (dropped the wrong entries →
model loses context it had).

**SLO:** MAE ≤ 1 %; tail-drop order = `new[-N:]` first;
`max_tokens ≥ output_reserve`. Any violation is a P1.

## Dim 5 — Cache marker byte-stability

`assemble` step 5 places the `cache_control` marker on the
**stable prefix** and asserts the prefix bytes are
byte-equal to the previous round's. The test is the
**assemble-side counterpart** of derive's CF1:

- For each pair of adjacent rounds, take the assembled
  payload's prefix region (`pinned + seen`).
- Byte-diff the prefixes.
- Assert stable.

The difference from CF1: CF1 measures the **Surface
prefix** (the `derive` output). This dim measures the
**assembled payload prefix** (the bytes that go over the
wire). They should match — but the assertion is
**independent**: a bug in `assemble`'s serialisation can
break the wire-level cache even when the surface cache is
fine.

The test also asserts the **marker position** is on a
byte-stable boundary. Anthropic's `cache_control` must be on
the **last** system block; OpenAI's auto-cache must kick in
at the right offset. A misplaced marker means the cache
**doesn't hit** even when the prefix is stable.

**SLO:** ≥ 99 % adjacent-round prefix stability (tighter
than CF1's 90 %, because the marker should make the wire
cache *more* stable than the surface cache, not less).
Marker position correct 100 % of the time. Any regression is
a P1.

## Bench Directory Layout

The 5-dim assemble contract lands as **5 sub-directories**
under `engine/core/tests/assemble_bench/`:

```
engine/core/tests/assemble_bench/
├── schema_compliance/      Dim 1 — POST payloads to provider (or stub)
│   ├── live/                nightly — real API, test key
│   └── stub/                per-PR — parses against OpenAPI schema
├── role_mapping/           Dim 2 — (kind, projection, provider) → role
├── dialect/                Dim 3 — Anthropic / OpenAI / Gemini markers
├── budget/                 Dim 4 — count accuracy, tail-drop, reserve
├── cache_marker/           Dim 5 — wire-level prefix stability
└── README.md               how to add a test
```

Same **fixture** directory as `derive_bench/fixtures/` is
**reused** — the synthesised-ledger generator is the input
engine for *both* benches. No duplication.

## Acceptance Gate

The 5 dims **must all pass** before any `assemble` change
lands. The gate is constitutional (it lives in
`.agents/testing.md`):

| Gate | What it asserts | Threshold |
|------|----------------|-----------|
| **Provider accepts** (Dim 1) | Live API rate 100 %; stub-parses 100 % | 100 % |
| **Role correctness** (Dim 2) | (kind × projection × provider) table 100 % match | 100 % |
| **Dialect correctness** (Dim 3) | Provider-specific markers correct 100 % | 100 % |
| **Budget accuracy** (Dim 4) | MAE ≤ 1 %; tail-drop order correct; `max_tokens ≥ reserve` | 1 % / correct / ≥ |
| **Cache marker stability** (Dim 5) | Adjacent-round wire prefix ≥ 99 % stable; marker position 100 % correct | 99 % / 100 % |

A regression that breaks **two** gates is two bugs.
**Provider accepts + Role correctness** is the most common
pair — a payload that the API rejects and the role map
disagrees about.

## How the 5 dims relate to the 4-dim derive test

The 4-dim **derive** test asserts *what should be in the
surface*. The 5-dim **assemble** test asserts *how that
surface becomes bytes for a specific provider*. The two
contracts are **complementary** — a `derive` regression
surfaces a wrong entry; an `assemble` regression surfaces a
right entry in a wrong shape. Together they cover the
end-to-end fidelity of "what the model sees".

If a regression lands in the **shared input** (the surface
or the ledger), both contracts may fail. The shared fixture
corpus means the diagnostic is consistent: the same
`golden__multiple_instructions_stacked/` snapshot is checked
in both benches; the failure report includes both
"derive-output" and "assemble-output" diffs, side by side.

## See also

- `loop.md` "Assemble (per-projection reshape)" — the
  function being tested.
- `loop.md` "Change-locality matrix" — assemble vs derive
  boundary; dim 5 (cache marker) is on the assemble side,
  dim 1–4 (content choice) are on the derive side.
- `docs/architecture/core/derive-testing.md` — the derive
  contract; the 5 assemble dims complement the 4 derive
  dims.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: **draft (possible mechanism)** — the 5-dim
  structure and the gate are settled; the specific test
  corpus and the live-vs-stub cadence land with the slice.
  No final code.