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
Dim 1 has **three layers**, each catching a different class
of bug.

### Dim 1a — Golden snapshot collection (per provider)

A **golden snapshot** is `(fixed_surface, expected_wire_json)`
per provider. The test runs `assemble` on the surface and
**byte-compares the output to the snapshot**. Any byte
difference is a failure.

```
engine/core/tests/assemble_bench/schema_compliance/golden/
├── anthropic/
│   ├── minimal_chat.json
│   ├── with_tool_use.json
│   ├── with_tool_result.json
│   ├── with_cache_control.json
│   ├── with_system_separated.json
│   ├── with_developer_role.json
│   └── ...
├── openai/
│   ├── minimal_chat.json
│   ├── with_tool_calls.json
│   ├── with_tool_result.json
│   ├── auto_cache_at_1024.json
│   ├── with_developer_role.json
│   └── ...
├── gemini/
│   ├── minimal_chat.json
│   ├── with_function_call.json
│   ├── with_system_instruction.json
│   └── ...
└── local/
    └── minimal_chat.json
```

The snapshots are **hand-curated** by a maintainer who:

1. Builds a small surface (a few entries, a few kinds).
2. Calls `assemble` once to produce the wire JSON.
3. Inspects the output, confirms it's correct, commits it
   as the snapshot.

When the maintainer intentionally changes the serialisation
(e.g. a new `cache_control` field), the snapshot **must** be
updated in the same commit; CI asserts the snapshot and the
code are updated together.

**Why byte-for-byte comparison?** Two equivalent
representations (`{"role":"user","content":"X"}` vs
`{"content":"X","role":"user"}`) might be semantically equal
but **the provider's cache key is byte-order-sensitive** in
some implementations. A byte-exact match catches this.

### Dim 1b — Provider structure validator (executable spec)

A **structural validator** is an executable specification of
the **hard rules** every provider output must satisfy. It
operates on the **payload, not the surface** — the same
shape that the provider's API will see.

The hard rules:

| Rule | Anthropic | OpenAI | Gemini |
|------|-----------|--------|--------|
| **Role alternation** | `user → assistant → user` (no two consecutive same-role messages) | same | same |
| **First message role** | `user` (system goes to top-level `system` array, not as a message) | `user` (system → `developer` role) | `user` (system → top-level `system_instruction`) |
| **Tool call pairing** | `tool_use` block → immediately followed by `user`-role `tool_result` block in the **same turn group** | `tool_calls` field on `assistant` message → immediately followed by `tool` role message in the **same turn group** | `functionCall` block → immediately followed by `functionResponse` block in the **same turn** |
| **System handling** | top-level `system` array; each block can carry `cache_control` | `developer` role message (newer) or top-level `system` (older) | `system_instruction` field |
| **Schema field legality** | every field is one of `messages`, `system`, `tools`, `tool_choice`, `max_tokens`, `temperature`, `cache_control` | every field is one of `messages`, `tools`, `tool_choice`, `max_tokens`, `temperature` | every field is one of `contents`, `system_instruction`, `tools`, `tool_config`, `generation_config` |

The validator is **generated from the same table** — when a
provider adds a field, the validator gets a new rule. The
validator runs against **random surfaces** (fuzzed from
the same synth generator) and asserts the produced payload
passes 100 %. A failure means either the serialisation has a
bug or the rule table is out of date — either way, the fix
is the table.

### Dim 1c — Real API smoke (nightly, live)

The most expensive layer: **POST the payload to the real
provider API** with a test key, assert a 200 response with a
`completion` field. Runs **nightly** because real API
calls cost money and time.

The smoke test uses **minimal requests** — the smallest
payload that exercises the full pipeline. A failed smoke
run is a **P0** (provider integration broken); the bench
captures the response body and the rejection reason for the
bug report.

```
# pseudo
for provider in [anthropic, openai, gemini]:
    payload = assemble(minimal_surface, provider=provider)
    response = http.post(provider.endpoint, payload, key=test_key)
    assert response.status == 200
    assert 'completion' in response.json()
    assert response.json()['usage']['prompt_tokens'] > 0
```

### Dim 1 SLO

- **1a** — 100 % byte-exact match against golden snapshots.
  Any failure is a P1.
- **1b** — 100 % pass against the structural validator, on
  every random surface. Any failure is a P1.
- **1c** — 100 % live API acceptance rate. Any rejection is a P0.

The three layers are **independent** — a payload can pass
the validator (1b) and the golden snapshot (1a) and still be
rejected by the provider (1c) if the provider changes its
spec without the validator being updated. All three must pass.

## Dim 2 — Semantic fidelity (no mapping loss)

Dim 2 answers: "when `assemble` serialises the surface, does
the **information content** survive the round trip?" Two
complementary tests.

### Dim 2a — Round-trip test (decoder ↔ surface)

Write a **decoder** that takes the assembled request
payload and reconstructs a **canonical `entries[]` list**
(of the same shape `derive` consumes). The test then asserts
that:

- The decoded entries are **isomorphic** to the input
  surface's entries.
- The **kinds** match.
- The **pair_ref** graph is preserved.
- The **pinned / branch_path / surface_visible** flags are
  preserved.
- The **body text** is preserved byte-for-byte (modulo
  provider's own escaping — the round trip is "is the
  information there, even if the bytes are escaped").

```
# pseudo
def round_trip_test(surface, provider):
    payload = assemble(surface, provider=provider)
    decoded = decode(payload, provider=provider)
    # decoded.entries is the canonical surface shape
    assert canonicalise(decoded.entries) == canonicalise(surface.entries)
    # pair_ref graph preserved
    assert decoded_pair_ref_graph == surface_pair_ref_graph
    # kinds preserved
    assert set(decoded.kind) == set(surface.kind)
```

The decoder is **provider-specific** (each provider has its
own shape to reverse-engineer). The test for each provider
uses that provider's decoder. The test passes when the
decoder reproduces the input surface.

A failure means `assemble` **dropped information** — the
model sees something different from what `derive` produced.
That's a **mapping loss** bug, caught here.

### Dim 2b — Role mapping matrix (table-driven unit test)

A **matrix** of `(kind × projection × provider) → expected
role / block type / position`. The test loads the matrix and
asserts **every cell** matches the expected mapping.

```
                | anthropic          | openai               | gemini
text.user       | role=user          | role=user            | role=user
text.assistant  | role=assistant     | role=assistant       | role=model
tool.call       | tool_use block     | tool_calls field     | functionCall
tool.result     | role=user,         | role=tool,            | role=functionResponse,
                |   tool_result_id    |   tool_call_id       |   functionCall.id
harness.feature | system[]           | role=developer       | system_instruction
goal.declare    | role=user (instr.)  | role=user (instr.)    | role=user (instr.)
                | (the goal text     | (the goal text        | (the goal text
                |  rides in user role |  rides in user role  |  rides in user role
                |  in Anthropic)     |  in OpenAI)          |  in Gemini)
```

(That last row is a subtle point: a `goal.declare` doesn't
have a dedicated role — it rides in the `user` role as
instruction text. The bench catches that.)

The matrix lives in `assemble_bench/role_mapping/matrix.json`
and is **generated** from the kind catalog + provider specs.
When a new kind is added, the matrix gets a new row; the
test fails until the new row's expected mapping is filled in.

**SLO:** 100 % role correctness, 100 % matrix coverage.
Any mismatch is a P1.

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

## Dim 4 — Token accuracy (counting + reserve)

Dim 4 has two layers: the **counting** must be exact, and
the **reserve** must be correct.

### Dim 4a — Counting accuracy (assemble ↔ provider)

`assemble` uses the **provider's real tokenizer** to count the
assembled prompt. The bench verifies that count against the
provider's **billed** count.

```
# pseudo — nightly, with test key
for provider in [anthropic, openai, gemini]:
    for surface in synth_corpus:
        payload = assemble(surface, provider=provider)
        response = http.post(provider.endpoint, payload, key=test_key)
        billed_tokens = response.json()['usage']['prompt_tokens']
        assemble_estimate = payload['usage']['estimated_prompt_tokens']
        error = abs(assemble_estimate - billed_tokens) / billed_tokens
        assert error < 0.01   # MAE ≤ 1 %
```

**MAE ≤ 1 %** is the target — `derive`'s T4 is an *estimate*
(may be loose); `assemble`'s count **must be exact** because
it gates the budget cut. A 1 % miscount on a 10k-token prompt
is 100 tokens; if the budget gate sees 10100 tokens but the
real count is 9900, it would over-truncate and lose context.

The **T4 → Dim 4a progression**:

| Stage | Layer | Tolerance |
|-------|-------|-----------|
| `derive` T4 | surface estimate | ≤ 5 % |
| `assemble` 4a | wire count, real | ≤ 1 % |

Tighter because `assemble` actually calls the provider's
tokenizer (not a generic estimator).

### Dim 4b — Output reserve (`max_tokens` + window slack)

The output budget is reserved upfront as `max_tokens` on the
request. The bench asserts:

- `max_tokens ≥ output_reserve` (configured in the budget).
- `max_tokens + prompt_tokens ≤ model.context_window` (the
  model won't reject for context overflow).
- `max_tokens ≤ model.max_output` (the model's hard cap).

```
# pseudo
response = http.post(...)
assert response.usage.completion_tokens <= max_tokens
# and the request itself was within bounds
assert max_tokens >= state.output_reserve
assert max_tokens + payload.estimated_prompt_tokens <= model.context_window
```

A failure here is **load-bearing** — the request gets
rejected with a 400 / 413, or the model truncates mid-reply.

**SLO:** 100 % pass on the reserve rules. Any violation is a
P1.

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