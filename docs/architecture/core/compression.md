# Compression — adaptive design

> Compression is one of four masking instructions
> (`compaction.rewrite`); its **mechanism** is in `loop.md`
> ("Derive in Detail (common path)" — step 3 of the six-step
> pipeline). Its **implementation** (`summarize(prefix)` LLM
> call) and its **policy** (when / how often to call it) are
> here. **Adaptive** because four layers — mathematical,
> self-learning, overflow, configuration — work together
> without code changes.

This doc and `loop.md` are split on purpose. `loop.md`
describes compression as a masking instruction (the
mechanism). This doc describes **how** to decide whether to
fire the LLM summariser, and **what to learn** from past
fires.

## TL;DR

Five-step algorithm, run at the end of each `derive`:

```
1. derive → surface, heuristic estimate E_now
2. find anchor: most recent model.usage with same model_id
3. projection = anchor.send_true + E_now - anchor.receipt.heuristic
4. if projection > trigger_ratio × window_size:  compress
5. on return: Runtime appends normalized usage and calibration evidence;
              verified overflow signals update per-model limit evidence
```

Four adaptive layers stacked on top:

| Layer | What it does | Tunable? |
|-------|-------------|----------|
| **Mathematical** | Projection formula + trigger structure | Versioned policy |
| **Self-learning** | EWMA calibration ratio = actual / estimated | Per-round automatic |
| **Context-limit evidence** | verified per-provider/model rejection evidence | Recorded with provenance and expiry |
| **Configuration** | trigger ratio, preserved headroom, calibration policy | Runtime config |

## Why this design

The earlier design (in `loop.md` "Budget Projection
(anchored to `model.usage`)") already introduced the
**anchor + delta** structure for budget projection. Adaptive
compression is its **trigger** — the projection tells the
loop *what the cost looks like*, and the trigger decides
*when to compress* to keep it under control.

Three failure modes the design defends against:

1. **Long-round estimator drift** — pure heuristic estimates
   drift as the surface shape changes (more code, more
   tokeniser surprises). Anchor + delta **cancels the
   systematic bias** between rounds of the same model.
2. **Provider ceiling false advertising** — a provider
   advertises a 200k window but rejects at 195k. The trigger
   must use the *real* ceiling, not the advertised one.
   verified overflow evidence can reduce the effective ceiling. One failure
   does not by itself prove an exact model limit.
3. **Tuning drift across rounds** — a fixed `trigger_ratio`
   can't adapt to a changing mix of long vs short rounds. The
   EWMA calibration ratio lets the **estimate** get better
   over time without code changes.

## The 5-step algorithm

### Step 1 — derive produces surface + heuristic `E_now`

`derive(surface, projection)` returns a heuristic token
estimate `E_now`. The estimate is a `derive`-internal
calculation; the result is carried forward as
`surface.heuristic_estimate`.

The estimate is **biased but consistent**: same model +
same surface shape → same bias, every round. That's the
property the anchor + delta exploits.

### Step 2 — find the anchor

The anchor is the most recent `model.usage` row whose
`model_id` equals the current run's `model_id`.

| Anchor present? | Path |
|-----------------|------|
| **Yes** (same `model_id`) | Step 3 — projection via anchor + delta |
| **No** (first round / model swap / malformed row) | Step 4 — pure `E_now` |

The `model_id` check is **load-bearing**. `tokenizer` is
never cross-model — a `claude-3-5` tokeniser is not a
`gpt-4o` tokeniser; an anchor from a different model would
introduce an unrecoverable bias. The fallback path is the
right answer for "anchor unavailable"; it's **not** a
degraded mode of the anchor path.

### Step 3 — projection via anchor + delta

```
projection =
    anchor.normalized_input_count # context contribution, not billing cost
  + E_now                        # this round, heuristic
  - anchor.receipt.heuristic     # last round, heuristic
```

The two `heuristic` terms cancel the systematic bias
(both estimates are made by the same estimator on the same
surface shape). The remaining noise is bounded by **twice
the per-round estimator error** — tracked by the EWMA
calibration ratio.

### Step 4 — trigger decision

```
if projection > trigger_ratio × effective_window:
    compress
```

Where `effective_window` is:

```
effective_window = min(
    configured_window_size,            # model spec, e.g. 200k
    observed_limit × safety_ratio          # scoped limit evidence
)
```

The trigger fires when the projection reaches a configured fraction of the
effective window, leaving measured headroom for output, tool calls, and
assembly overhead. Scoped limit evidence shrinks the effective window when a
verified adapter outcome indicates a lower accepted limit than configured.

### Step 5 — record + learn

After the model call returns:

1. Append a `model.usage` row with `tokens`, `model_id`,
   `model_reported`, `ref → loop.receipt`.
2. Runtime's accounting/calibration component updates the **EWMA calibration
   ratio** after the call. `derive` only reads an immutable calibration snapshot:
   ```
   calibration_ratio =
       0.9 × calibration_ratio            # old
     + 0.1 × (actual / estimated)         # new round
   ```
3. If the adapter returns a verified `Rejected(ContextOverflow)`, record the
   normalized input size, provider/model identity, sanitized classification
   evidence, and expiry.

The calibration ratio is what `derive` uses to convert
heuristic estimates into better estimates:

```
E_now_corrected = E_now_heuristic × calibration_ratio
```

This is the **self-tuning layer** — the estimator improves
over time, automatically.

## The 4-layer adaptation

### Layer 1 — Mathematical (versioned policy)

- The projection formula is a versioned policy with property tests around
  monotonicity and safety margins.
- Coefficients are hypotheses until workload evidence promotes them to gates.

### Layer 2 — Self-learning (EWMA calibration)

```
EWMA_alpha = configured_alpha
calibration_ratio[t+1] =
    (1 - alpha) × calibration_ratio[t]
  +     alpha  × (actual[t] / estimated[t])
```

The ratio converges over many rounds; one outlier doesn't
move it. The initial convergence rate is provisional and must be measured.

The **calibration_ratio** is persisted by Runtime as calibration evidence and
passed to `derive` as part of its immutable input:

```
E_now_corrected = E_now_heuristic × calibration_ratio
```

### Layer 3 — context-limit evidence (`overserved_max`)

A verified provider-specific context-overflow rejection is evidence that the submitted
context exceeded an accepted limit under that request shape. Runtime records
the normalized input size and its provenance:

```
overserved_max = request_size × overserved_ratio   # default 0.95
```

The evidence is scoped at least by provider, model, endpoint/request shape,
and policy version. Expiry and safety margin are configuration hypotheses;
after expiry Runtime falls back to its configured model capability.

### Layer 4 — Configuration (runtime-tunable knobs)

| Knob | Default | Meaning |
|------|---------|---------|
| `trigger_ratio` | provisional | Compress when projection reaches this fraction of the effective window. |
| `min_preserve` | model-specific | Minimum headroom preserved after compression. |
| `EWMA_alpha` | provisional | Calibration learning rate. |
| `overserved_ratio` | provisional | Safety margin applied to verified overflow evidence. |
| `overserved_max_ttl` | provisional | How long provider/model limit evidence is remembered. |

All knobs are Runtime policy. Initial values belong in configuration and must
be accompanied by benchmarks or production observations before becoming
release gates.

## Worked example — a case study (anchor vs no-anchor)

The following walks a single round sequence through the
algorithm. Numbers are concrete; the design is the shape,
but the walk shows what each layer does in practice.

```
Setup: window = 200k, trigger_ratio = 0.85
        → trigger threshold = 170k
```

**Turn 5** (no anchor — first round or model swap):

```
derive → E_now = 92k                       # heuristic
anchor: none                               # first round
projection = E_now = 92k                  # pure fallback
trigger  : 92k < 170k                     # don't compress
call:    normalized input reported = 88k   # context accounting
         assembly count = 87.5k            # real tokeniser
record:  model.usage{tokens.in = 88k,
                       sent_true = 87.5k,
                       heuristic = 92k}
ratio:   88 / 92 = 0.957                   # heuristic over-est by 4.3 %
```

**Turn 6** (new tool result):

```
derive → E_now = 104k                      # surface grew
anchor : Turn 5
projection = anchor.sent_true + (E_now − anchor.heuristic)
          = 87.5k + (104k − 92k)
          = 99.5k
trigger  : 99.5k < 170k                    # don't compress

Note: pure heuristic would have estimated 104k; the anchor
method sees the real level is 99.5k. The systematic bias
(heuristic over-estimates) is **subtracted out**.
```

**Turn 20** (long round, heuristic drift accumulated):

```
Pure heuristic:  estimated 180k            # drift over 15 rounds
                  trigger fires (180 > 170) # premature compression
Anchor method:   projection = 165k        # real base + small delta
                  165 < 170                # don't fire
```

**Without the anchor, the loop would have compressed
prematurely at round 20** — wasting tokens on a summariser
call when there was still headroom. With the anchor, the
trigger holds until the **real** budget is reached.

**One day** (provider's advertised window is wrong):

```
A request for 195k normalized tokens receives a provider-specific signal that
the adapter classifies as `Rejected(ContextOverflow)`.
overserved_max  = 195k × 0.95 = 185.25k
effective_window = min(200k, 185.25k) = 185.25k
trigger        = 185.25k × 0.85 = 157.5k

The example tightens the trigger from 170k to 157.5k. In a real policy, the
evidence is scoped, carries a configured margin, and expires; the values above
are illustrative rather than defaults.
```

The four layers work in concert:

- **Math** — the formula's shape (anchor + delta + trigger)
  is constant.
- **Self-learning** — EWMA converges the estimator's
  bias over many rounds.
- **Context-limit evidence** — `overserved_max` corrects the trigger when
  the provider's spec is wrong.
- **Configuration** — `trigger_ratio`, `min_preserve`, and
  the EWMA coefficients are runtime knobs.

## Recording each round

Each round appends a `model.usage` row:

```python
class ModelUsage:
    tokens:         Tokens              # in / out / cache_read / cache_write / total
    model_reported: bool                # provider-reported vs locally estimated
    model_id:       string              # model used for the request
    ref:            {session, uid}      # → loop.receipt entry for this round
    calibration:    float               # EWMA at the moment of this round
    heuristic:      int                 # what derive estimated (for delta math)
```

The `ref` points at the `loop.receipt` entry that described
this round's surface. An audit query joins them:

```sql
-- "Show me what the model saw and what it cost."
SELECT r.notes AS saw,
       u.tokens AS paid,
       u.calibration AS cal,
       u.heuristic AS est,
       u.tokens.in - u.heuristic AS bias
  FROM entry r
  JOIN entry u ON u.ref == r.uid
 WHERE u.kind == 'model.usage'
   AND r.kind == 'loop.receipt'
 ORDER BY u.wall_ts DESC
 LIMIT 50;
```

## Failure modes

| Failure | Layer that catches it | Recovery |
|---------|----------------------|----------|
| Provider rejects the submitted context as too large | **Context-limit evidence** | Normalize the provider-specific signal, record scoped evidence, and tighten conservatively |
| Estimator drifts over many rounds | **Self-learning** (EWMA) | Ratio converges; trigger stops firing prematurely |
| First round / model swap | **Mathematical fallback** | No anchor → pure `E_now`; the next round gets a fresh anchor |
| Calibration ratio out-of-bounds (e.g. 2.0) | **Self-learning alert** | Ratio clamped at runtime; out-of-bounds value recorded in `ops_log` |
| Anchor has wrong `model_id` | **Mathematical** (anchor selection) | Anchor discarded; pure `E_now`; the next round gets a fresh anchor |

## Pending decisions (recorded, not resolved)

These are open for later iteration; the design above
**records** them but does not commit to a choice:

- **P1. EWMA alpha** — an initial candidate is 0.1. The right value depends on
  how stable the provider is; a noisy provider wants a
  smaller alpha (slower convergence, smoother), a stable
  provider wants a larger alpha (faster convergence, less
  memory). *Resolution deferred.*
- **P2. `overserved_ratio`** — an initial candidate is 0.95. Safety margin on
  verified overflow evidence. A more conservative margin (e.g.
  0.85) is safer but compresses earlier. *Resolution deferred.*
- **P3. `overserved_max_ttl`** — an initial candidate is 7 days. How long to
  trust limit evidence. A provider classification or capability can change;
  expiry prevents stale evidence from becoming permanent truth.
  *Resolution deferred.*
- **P4. Anchor selection at multi-model boundaries** — when a
  single round emits multiple `model.usage` rows (e.g. router
  → primary → fallback), which row is the anchor? *Resolution
  deferred.*
- **P5. Calibration ratio clamping** — the ratio should be
  bounded (e.g. `[0.5, 2.0]`) to prevent runaway from bad
  data. *Resolution deferred.*

**These decisions do not block the design** — each is
resolvable in its own follow-up commit. The design above
is the **stable** structure: 5-step algorithm + 4-layer
adaptation. The five items above are the **policy** layer
on top.

## Integration — where each change lands (the "onion" view)

The above adaptive compression design lands **entirely in
the existing turn-loop skeleton** without changing it. The
"onion" layering of the loop (`ledger` at the bottom,
`derive` → `assemble` → `model` → `governance` →
`executor` → `record` on top) absorbs every change as either
a **signature** extension, an **append to the ledger**, or a
new **internal step** inside an existing layer. **Zero
skeleton rewrites.**

### Five change points — each at the right layer

| # | Change | Lands in | Skeleton touched? |
|---|--------|-----------|-------------------|
| **1. Receipt writing + ledger append** | `derive` returns `(surface, receipt)`; the loop already appends `loop.receipt` to the ledger via the existing `append` machinery | `derive` signature + the loop's existing `append` step | **No** — the receipt is just another entry the loop appends |
| **2. Anchor accounting (projection / EWMA)** | Runtime reads normalized prior usage, updates calibration after a call, and supplies a snapshot; `derive` computes a projection without writes | Runtime accounting + derive input | **No** — ownership follows the existing Runtime/Agent boundary |
| **3. Anchor source (read prior round)** | The previous round's `model.usage` is the **anchor**; `derive` reads it via the ledger port | `ledger.usage` port + `derive` reads | **No** — pure read |
| **4. observed-limit learning** | The adapter classifies provider-specific overflow; Runtime records scoped evidence | provider adapter + Runtime accounting | **No** — classified outcomes already flow through Runtime |
| **5. Provider cache marker** | The provider adapter writes the `cache_control` marker on the byte-stable prefix; `assemble` asserts byte stability | `assemble` + provider adapter | **No** — assemble already knows prefix bytes from `derive` |

### The "onion" picture

```
       turn_loop skeleton
       (zero changes)
       ───────────────────────────
                ↓ call
       ───────────────────────────
       derive                          (signature: +purpose)
         ├── purpose projection       (5)
         ├── inject dedup / layout    (unchanged)
         ├── derive 6-step pipeline   (unchanged)
         ├── read prior usage         (3)
         ├── compute projection       (2)
         ├── read calibration snapshot (2)
         ├── return (surface, receipt) (1)
       ───────────────────────────
       assemble                        (zero changes here)
       ───────────────────────────
       model adapter
         ├── assemble output → POST
         ├── classify overflow signal (4)
         └── emit overserved_max      (4)
       ───────────────────────────
       ledger port
         └── append(receipt, usage)   (1, 4)
       ───────────────────────────
```

The **skeleton** (`turn_loop`) is the outer ring. Every
change is **inside** the onion — `derive`'s signature, the
ledger port's read, the model adapter's error handler.
The skeleton's control flow (`derive → assemble → invoke →
governance → record`) doesn't change.

### Why this works

The onion absorbs the change because each layer's
**contract** is already what the change needs:

- **Ledger port** already exposes `usage` rows; `derive`
  reading the anchor is one method call (`ledger.usage.latest(model_id)`).
- **`append` is append-only**; a new kind (`model.usage`,
  `loop.receipt`) is one more entry — no schema migration.
- **`model` adapter** already normalizes provider responses; overflow is a
  provider-specific classification, not a universal HTTP branch.
- **`assemble` already knows** the byte-stable prefix from
  `derive`'s split-at-`last_seen_seq`; the provider marker
  is one more write.

The **zero skeleton change** is the design's
**invariant**. If a future change requires touching
`turn_loop` itself, that's a smell — either the change is
out-of-scope for this design, or the onion wasn't layered
right to begin with.

## See also

- `loop.md` "Derive in Detail (common path)" — compression
  as one of the six derive steps. **This is the only
  place compression lives as a masking instruction.**
- `loop.md` "Compression scope split" — which concerns
  belong here vs in `loop.md`.
- `loop.md` "Budget Projection (anchored to `model.usage`)"
  — the projection formula this builds on; adaptive
  compression is its **trigger**.
- `ledger.md` "compaction.* family" — the entry kinds this
  design produces.
- `docs/architecture/core/derive-testing.md` "T2. LLM
  compression call rate" — the bench that measures whether
  this design works (≥ 50 iterations per compression call).
- `docs/architecture/core/derive-testing.md` "B4 memory
  + leak" + "T1 surface smoothness" — adjacent benches
  that catch silent regressions in adaptive compression's
  budget/cost behaviour.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: **draft (possible mechanism)** — the 5-step
  algorithm and the 4-layer adaptation are working hypotheses; the
  EWMA alpha, the trust margin, the TTL, and the anchor
  selection policy are placeholders that land with the
  slice. No final code.
