# bench/AGENTS.md

> **Thin benchmark driver for Agent SWE-style evaluations.**
> `bench/` orchestrates four steps — **load → drive → adapt →
> eval** — and does nothing else. It does **not** implement the
> agent, the environment, or the eval. Every piece is pluggable
> so Garive's agent design has an **objective, public,
> reproducible** score without anyone hand-rolling private
> cases.
>
> This file applies to everything under `bench/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## The Four Steps

`bench/` is a thin driver. It runs four steps per case:

```
   ┌────────────┐    ┌──────────┐    ┌─────────────┐    ┌──────┐
   │   LOAD     │ →  │  DRIVE   │ →  │   ADAPT     │ →  │ EVAL │
   │            │    │          │    │             │    │      │
   │  load      │    │  loop:   │    │  intake +   │    │ call │
   │  cases     │    │  intake  │    │  prefication│    │ offi-│
   │  from      │    │  run     │    │  translate  │    │ cial │
   │  official  │    │  collect │    │             │    │ swe- │
   │  datasets  │    │  diff    │    │             │    │ bench│
   │            │    │          │    │             │    │  eval│
   └────────────┘    └──────────┘    └─────────────┘    └──────┘
   cases/            runner          adapters/          eval/
```

| Step | Module | What it does |
|------|--------|--------------|
| **1. LOAD** | `bench/cases/` | Reads cases from official public datasets (SWE-bench Verified / Lite / Multimodal / Multilingual, Terminal-Bench, …). Immutable input. |
| **2. DRIVE** | `bench/src/runner.rs` | Thin loop. For each case: setup env, run agent, collect raw output. No translation happens here. |
| **3. ADAPT** | `bench/adapters/intake/` + `bench/adapters/patch/` | Two translations: (a) **intake** — case → agent-consumable input (problem statement + repo state, formatted for the agent); (b) **prefication** — agent output → canonical unified diff (SWE-bench format). |
| **4. EVAL** | `bench/eval/` | Calls the official swe-bench eval scripts in an independent env. Computes score. |

Everything else — env adapter, agent, adapters, eval — is
**pluggable**. `bench/` provides the loop, the interfaces, the
default implementations, and the tracking. Nothing more.

## Why a Thin Driver

The whole point of `bench/` is to **make the agent's score
objective and reproducible**:

- The **cases** come from official public datasets. Nobody
  picks which task favours Garive.
- The **eval** is the official Python harness shipped with
  swe-bench. Garive doesn't define pass / fail; the benchmark
  does.
- The **adapter layer** lets different agents plug in
  unchanged.
- The **driver loop** is a single uniform path — every run
  flows through the same code, so the score is comparable
  across runs, across versions, across envs, across runners.

Without `bench/`, every agent change would need a hand-rolled
"how does this version feel" demo, and the score would be
opinions. With `bench/`, every agent change has a number
that anyone can rerun.

## Hard Rules

### Rule 1 — Cases Are Always From Official Public Datasets

All validation cases come from **publicly available, official
Agent SWE / SWE-style benchmarks**. We do **not** invent
cases, we do **not** curate private cases, and we do **not**
allow Garive-specific cases to masquerade as benchmark
cases.

| Source | Repository | Notes |
|--------|-----------|-------|
| SWE-bench Verified | [princeton-nlp/SWE-bench](https://github.com/princeton-nlp/SWE-bench) `verified/` | 500 cases, human-validated; the headline number to publish. |
| SWE-bench Lite | same repo `lite/` | 300 cases, faster iteration. |
| SWE-bench Multimodal | same repo `multimodal/` | cases with visual inputs. |
| SWE-bench Multilingual | same repo `multilingual/` | non-English issues. |
| Terminal-Bench | [laude-institute/terminal-bench](https://github.com/laude-institute/terminal-bench) | terminal / shell tasks; complements swe-bench's repo-edit style. |
| (future) | — | Add a new public benchmark by appending a row and updating ` ` `bench/cases/README.md`. |

Current admitted B0 V1 is intentionally narrower than this future source
catalogue: only SWE-bench Lite and Verified `test` JSONL are executable.

A run **must** declare its source (`--source swe-bench-verified
| --source swe-bench-lite | --source terminal-bench | ...`).
Results from different sources are not directly comparable —
tracking records the source so historical scores can be
filtered by it.

If a private / Garive-specific case set is ever needed, it
goes in a separate non-`bench/` directory (e.g.
`experiments/private-cases/`) and is explicitly **not**
counted as benchmark output.

### Rule 2 — Pooling & Parallelism Are Mandatory

Per-case image pulls, dependency installs, and warm-up are
the single biggest cost in the runtime. **Future
implementations must design for pooling and parallelism
from day one** — bolting them on later is more expensive than
solving the problem now.

Three pillars (details in `bench/envs/README.md`):

1. **Image pool** (for `official` mode). A warm pool of
   swe-bench Docker images keeps N copies pulled and idle, so
   per-case setup is a `docker run` from local cache, not a
   fresh `docker pull`. Pool size tracks the concurrency
   target.
2. **Workspace pool** (for `self-cow` mode). A pool of
   pre-cloned repo workspaces at known commits. After a run,
   the workspace is wiped (not re-cloned) and returned to the
   pool.
3. **Bounded case parallelism**. Multiple cases run in
   parallel via async tasks. Concurrency is bounded by a
   configurable `--jobs N` flag (default: `min(cpus, 8)`),
   and by per-env limits.

These three are not optional add-ons. Sequential runs (one
case at a time, full image pull per case) are **forbidden**
for any official / published score run.

### Rule 3 — Designed for GitHub Actions and Remote Runners

`bench/` does not run only on a developer laptop. **Local runs
are dev-loop only.** Any score that lands in
`bench/tracking/versions/` is produced on a CI runner — a
GitHub Actions hosted runner or, more commonly, a
self-hosted runner.

This shapes the design in three ways:

1. **Workflow files live in `.github/workflows/`**. At least:
   - `bench-ci.yml` — per-PR smoke run (small subset, `self-cow`,
     fail fast).
   - `bench-nightly.yml` — nightly full-suite `self-cow` run,
     posts score to tracking.
   - `bench-release.yml` — manual trigger; full-suite `official`
     run on a self-hosted runner with sufficient disk and
     time budget.

2. **Resource budgets are runner-shaped**, not laptop-shaped.
   - GH-hosted runners cap at **14 GB disk, 6 h job, no
     nested Docker by default**. That is **not enough** for a
     full `official` run; `official` lives on self-hosted.
   - Self-hosted runners can be sized for the workload:
     ≥1 TB disk, 32+ cores, 24 h+ timeout, Docker daemon
     configured for nested-Docker if needed.
   - The runner decides pool size: `--jobs N` defaults to the
     runner's advertised `cpu_count`, capped by env limits.

3. **Output is CI-shaped**, not interactive:
   - Progress is **JSONL to stdout** (machine-parseable) plus
     a Markdown summary at the end (human-readable).
   - Per-case detail goes to `bench/tracking/runs/<run_id>.jsonl`,
     uploaded as a workflow artifact.
   - The version summary lands in `bench/tracking/versions/<vX.Y.Z>.json`,
     committed to a tracking branch (`bench-tracking`) by the
     workflow so the score history is durable across runner
     instances and visible in the repo.
   - Exit code is **0 on success, non-zero on infrastructure
     failure** — never on agent underperformance. Agent
     underperformance produces a low score, not a failed CI
     run.

## The Driver Loop (Step 2 in detail)

`bench/src/runner.rs` runs the following per case. There is
**no other code path**:

```
for case in cases:
    env_setup = env.setup(case)
    raw_input = intake_adapter.translate(case, env_setup)
    raw_output = agent.run(raw_input, env_setup)
    diff      = prefication_adapter.translate(raw_output, case)
    eval_result = eval.run(diff, case)
    record(eval_result)
    env.teardown(env_setup)
```

The loop is the **only** orchestration in the repo. There is
no parallel code path; no "skip the adapter" mode; no
"directly use agent's native patch" mode. Every score in
`bench tracking` came through this loop.

## Adapter Contracts (Step 3 in detail)

Two adapters, **both pluggable**:

### Intake adapter (`bench/adapters/intake/`)

`case + env_setup → agent-consumable input`. Different agents
expect different input shapes — natural-language prompt,
structured message, JSON RPC, etc. The adapter translates the
canonical case + env state into whatever the agent accepts.

### Prefication adapter (`bench/adapters/patch/`)

`agent output → canonical unified diff`. Different agents emit
patches in different shapes — raw `diff -u`, full-file
rewrites, search-and-replace, JSON edits, structured
patch objects. The adapter normalizes to SWE-bench's
unified-diff form so the eval harness always sees the same
input shape.

Both adapters are swappable. New agents → add an intake
adapter + a prefication adapter. The driver loop, env, eval,
and tracking stay untouched.

## Env Adapter (the "run" in the loop)

The driver loop calls into the env adapter for setup / teardown
only. The agent runs inside the env via the env's `exec`,
`read`, `write`, `patch` methods. Two concrete envs:

- `bench/envs/official/` — Docker harness (matches official
  swe-bench numbers).
- `bench/envs/self-cow/` — Host-based, no containers (fast,
  dev / nightly).

## Eval (Step 4)

`bench/eval/` does **not** implement the evaluator. It calls
the official swe-bench evaluation scripts (the Python harness
that ships with swe-bench Verified / Lite) in an
**independent env** so the patch cannot contaminate the next
case.

## Tracking

`bench/tracking/` records per-version:

```
version  | date  | env   | agent   | cases_passed | cases_total | score
v0.4.1   | 2026-08-27 | official | garive | 47 | 500 | 9.4%
```

Append-only JSONL. Per-case detail alongside.

## Layout

```
bench/
├── AGENTS.md                    this file (the 4-step design + hard rules)
├── README.md                    overview + flow diagram
├── Cargo.toml                   workspace member
├── src/
│   ├── lib.rs                   public surface
│   ├── case.rs                  Case struct + official loader
│   ├── runner.rs                 **the driver loop** (Step 2)
│   └── main.rs                  CLI: bench run | bench score | bench report
├── cases/                       Step 1 — official public datasets
├── envs/                        the "run" in the loop
│   ├── official/                Docker harness
│   └── self-cow/                host-based
├── adapters/                    Step 3 — two translations
│   ├── intake/                  case + env → agent input
│   └── patch/                   agent output → canonical diff
├── eval/                        Step 4 — call official swe-bench eval
├── tracking/                    per-version + per-case JSONL
└── scripts/                     helpers: fetch-cases, bootstrap-eval, report
```

## What NOT to Do

- ❌ Don't implement the agent inside `bench/`. Inject.
- ❌ Don't reimplement SWE-bench's eval. Call the official
  scripts.
- ❌ Don't skip the intake or prefication adapter. Every
  agent goes through both.
- ❌ Don't track scores inside the env / adapter. Tracking
  lives in `bench/tracking/` so it's swappable.
- ❌ Don't run multiple cases in the same env. Per-case env
  lifetime is non-negotiable.
- ❌ Don't bypass the driver loop with a "faster" custom
  path. The loop is the only path to a recorded score.
- ❌ Don't invent cases. All cases come from the official
  public datasets listed above.

## Build

```
just bench                  # cargo test -p bench (scaffold verification)
just conformance            # full repository conformance
```

## Testing

This tier **is** the Agent / SWE test layer (layer 8 in
`.agents/testing.md`). The other seven layers apply as
follows:

| Layer | Where |
|-------|-------|
| Static | `cargo fmt --check`, `cargo clippy -- -D warnings` on the bench crate |
| Unit | `bench/src/lib.rs`, `runner.rs` etc. — the driver loop, the pool logic, the adapter dispatch |
| Contract | round-trip the bench output JSON schema (`bench/tracking/`) on read |
| Cross-language | Add an executable harness only when two real consumers need comparison |
| Integration | `tests/integration/bench-*` — wiring the runner with a fake agent, fake env, fake eval |
| E2E | `tests/e2e/bench-*` — full sweep with the smallest fixture subset on a self-hosted runner |
| Agent / SWE | **the whole bench/** is layer 8 |
