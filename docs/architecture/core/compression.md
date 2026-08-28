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
5. on return: append model.usage; update EWMA calibration;
            flag overserved_max if 413
```

Four adaptive layers stacked on top:

| Layer | What it does | Tunable? |
|-------|-------------|----------|
| **Mathematical** | Projection formula + trigger structure | No — hard-coded contract |
| **Self-learning** | EWMA calibration ratio = actual / estimated | Per-round automatic |
| **Overflow** | `overserved_max` from provider 413s | Auto-recorded, TTL |
| **Configuration** | `trigger_ratio` (default 0.85), `min_preserve` | Runtime config |

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
   `overserved_max` records the real ceiling from the first
   413.
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
    anchor.send_true_count       # provider billed, post-cache
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
    overserved_max × overserved_max_ratio  # 0.95 — provider real ceiling
)
```

The trigger fires when the projection is `trigger_ratio`
(default **85 %**) of the effective window — leaves 15 % as
headroom for the model's reply + tool calls + assembly
overhead. `overserved_max` shrinks the effective window
when the provider's real ceiling is lower than the
advertised one.

### Step 5 — record + learn

After the model call returns:

1. Append a `model.usage` row with `tokens`, `model_id`,
   `model_reported`, `ref → loop.receipt`.
2. Update the **EWMA calibration ratio**:
   ```
   calibration_ratio =
       0.9 × calibration_ratio            # old
     + 0.1 × (actual / estimated)         # new round
   ```
3. If the provider returned **413**, record the request's
   actual byte size as `overserved_max`.

The calibration ratio is what `derive` uses to convert
heuristic estimates into better estimates:

```
E_now_corrected = E_now_heuristic × calibration_ratio
```

This is the **self-tuning layer** — the estimator improves
over time, automatically.

## The 4-layer adaptation

### Layer 1 — Mathematical (hard-coded contract)

- The projection formula (anchor + delta + trigger) is
  **math**.
- The shape is the contract; the coefficients are
  documented but not user-configurable in this layer.
- A test asserts the formula byte-for-byte; a regression
  here is a P0.

### Layer 2 — Self-learning (EWMA calibration)

```
EWMA_alpha = 0.1   # constitutional default
calibration_ratio[t+1] =
    (1 - alpha) × calibration_ratio[t]
  +     alpha  × (actual[t] / estimated[t])
```

The ratio converges over many rounds; one outlier doesn't
move it. The convergence rate is the constitutional
`EWMA_alpha` (0.1 = slow but stable).

The **calibration_ratio** is stored in `state` per-round
and re-used by `derive`'s estimator:

```
E_now_corrected = E_now_heuristic × calibration_ratio
```

### Layer 3 — Overflow (`overserved_max`)

A provider **413** (request too large) is a signal that
the runtime asked for more than the model can deliver. The
runtime records the request's actual size:

```
overserved_max = request_size × overserved_ratio   # default 0.95
```

`overserved_max` is **per-model-id** and TTL'd (default 7
days). After 7 days without a new 413, the runtime falls
back to the configured window size. `overserved_max` is
the runtime **trusting the provider's hard ceiling over
the configured window size**.

### Layer 4 — Configuration (runtime-tunable knobs)

| Knob | Default | Meaning |
|------|---------|---------|
| `trigger_ratio` | 0.85 | Compress when projection reaches 85 % of effective window. |
| `min_preserve` | model-specific | Minimum headroom preserved after compression. |
| `EWMA_alpha` | 0.1 | Calibration learning rate. |
| `overserved_ratio` | 0.95 | Trust margin on provider's reported 413 size. |
| `overserved_max_ttl` | 7 days | How long `overserved_max` is remembered. |

All knobs are **runtime-tunable**. Constitutional defaults
are documented here; the runtime can change them at startup
or per-session.

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
call:    input reported = 88k              # provider billed
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
A request for 195k tokens is rejected with 413.
overserved_max  = 195k × 0.95 = 185.25k
effective_window = min(200k, 185.25k) = 185.25k
trigger        = 185.25k × 0.85 = 157.5k

The trigger automatically becomes 157.5k instead of 170k —
the runtime **trusts the provider's real ceiling over the
configured window size**. Self-healing; no human in the
loop. The `overserved_max` TTLs after 7 days; if the
provider's behaviour normalises, the runtime relaxes back
to the configured window.
```

The four layers work in concert:

- **Math** — the formula's shape (anchor + delta + trigger)
  is constant.
- **Self-learning** — EWMA converges the estimator's
  bias over many rounds.
- **Overflow** — `overserved_max` corrects the trigger when
  the provider's spec is wrong.
- **Configuration** — `trigger_ratio`, `min_preserve`, and
  the EWMA coefficients are runtime knobs.

## Recording each round

Each round appends a `model.usage` row:

```python
class ModelUsage:
    tokens:         Tokens              # in / out / cache_read / cache_write / total
    model_reported: bool                # true = provider billed, false = client estimated
    model_id:       string              # the model that produced the bill
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
| Provider's advertised window is wrong | **Overflow** (`overserved_max`) | 413 → record real ceiling → trigger auto-tightens |
| Estimator drifts over many rounds | **Self-learning** (EWMA) | Ratio converges; trigger stops firing prematurely |
| First round / model swap | **Mathematical fallback** | No anchor → pure `E_now`; the next round gets a fresh anchor |
| Calibration ratio out-of-bounds (e.g. 2.0) | **Self-learning alert** | Ratio clamped at runtime; out-of-bounds value recorded in `ops_log` |
| Anchor has wrong `model_id` | **Mathematical** (anchor selection) | Anchor discarded; pure `E_now`; the next round gets a fresh anchor |

## Pending decisions (recorded, not resolved)

These are open for later iteration; the design above
**records** them but does not commit to a choice:

- **P1. EWMA alpha** — default 0.1. The right value depends on
  how stable the provider is; a noisy provider wants a
  smaller alpha (slower convergence, smoother), a stable
  provider wants a larger alpha (faster convergence, less
  memory). *Resolution deferred.*
- **P2. `overserved_ratio`** — default 0.95. Trust margin on
  the provider's 413 size. A more conservative margin (e.g.
  0.85) is safer but compresses earlier. *Resolution deferred.*
- **P3. `overserved_max_ttl`** — default 7 days. How long to
  trust a single 413. A flaky provider's 413 might be a
  transient; long TTL gives time for the EWMA to re-converge.
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
| **2. Anchor accounting (projection / EWMA)** | `derive` reads the previous round's `model.usage` from the ledger; computes `projection`; updates the EWMA calibration | `derive` reads from `ledger.usage` table | **No** — ledger port already exposes usage; `derive` is the consumer |
| **3. Anchor source (read prior round)** | The previous round's `model.usage` is the **anchor**; `derive` reads it via the ledger port | `ledger.usage` port + `derive` reads | **No** — pure read |
| **4. observed_max learning** | The model adapter recognises the **413** response; records `overserved_max` on the error entry | `model` adapter | **No** — error entries already flow through `append` |
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
         ├── compute EWMA             (2)
         ├── return (surface, receipt) (1)
       ───────────────────────────
       assemble                        (zero changes here)
       ───────────────────────────
       model adapter
         ├── assemble output → POST
         ├── detect 413               (4)
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
- **`model` adapter** already handles error responses; a 413
  path is one more branch.
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
- Last reviewed: 2026-08-27
- Status: **draft (possible mechanism)** — the 5-step
  algorithm and the 4-layer adaptation are settled; the
  EWMA alpha, the trust margin, the TTL, and the anchor
  selection policy are placeholders that land with the
  slice. No final code.