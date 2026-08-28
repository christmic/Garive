# bench/AGENTS.md

> **SWE-bench verification runtime.** `bench/` orchestrates a
> SWE-bench evaluation: it loads official cases, spins up an
> environment, runs the agent inside it, captures the patch,
> and feeds it to the official eval. **`bench` itself is an
> orchestrator — it does not implement the agent, the
> environment, or the eval.**
>
> This file applies to everything under `bench/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Hard Rules

These are non-negotiable. Violating any of them breaks
benchmark validity.

### Rule 1 — Cases Are Always From Official Public Datasets

All validation cases in `bench/` come from **publicly
available, official Agent SWE / SWE-style benchmarks**. We do
**not** invent cases, we do **not** curate private cases, and
we do **not** allow Garive-specific cases to masquerade as
benchmark cases.

Recognised sources (this list grows as new public benchmarks
land):

| Source | Repository | Notes |
|--------|-----------|-------|
| SWE-bench Verified | [princeton-nlp/SWE-bench](https://github.com/princeton-nlp/SWE-bench) `verified/` | 500 cases, human-validated; the headline number to publish. |
| SWE-bench Lite | same repo `lite/` | 300 cases, lighter; faster iteration. |
| SWE-bench Multimodal | same repo `multimodal/` | cases with visual inputs. |
| SWE-bench Multilingual | same repo `multilingual/` | non-English issues. |
| Terminal-Bench | [laude-institute/terminal-bench](https://github.com/laude-institute/terminal-bench) | terminal / shell tasks, complements swe-bench's repo-edit style. |
| (future) | — | When a new public benchmark is needed, add it here and update the schema doc in `cases/`. |

A run **must** declare its source (`--source swe-bench-verified |
` `--env` config` flag).
`` `--env` config` flag).
` `--source terminal-bench`). Results from different sources
are not directly comparable — tracking records the source
explicitly so historical scores can be filtered by it.

If a private / Garive-specific case set is ever needed, it
goes in a separate non-`bench/` directory (e.g.
`experiments/private-cases/`) and is explicitly **not**
counted as benchmark output. The branch never becomes a
backdoor for Garive-favourable cases to leak into benchmark
scores.

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
   and by per-env limits (e.g. Docker's daemon can comfortably
   run ~16 swe-bench containers before IO thrashes).

These three are not optional add-ons. The future impl
**must** ship with all three wired before the first benchmark
result is recorded as a version score. Premature sequential
runs (one case at a time, full image pull per case) are
explicitly forbidden for any official / published score run.

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

Concretely, no run is allowed to bypass CI. A local run is
fine for iterating on the agent, but the same code path that
the agent and harness exercise in dev must also be exercised
on a runner before the score is trusted. Local → GH Actions
sync is a code-review gate, not just a documentation nicety.

Implementation notes for the runner environment:

- Docker buildx / buildkit configured for caching across runs.
- A persistent runner **must** keep the image pool warm
  between jobs (else we lose the pool between runs and fall
  back to cold pulls).
- If the runner has multiple hosts, the pool is per-host —
  not shared. Use GH Actions matrix strategy carefully.
- `actions/cache` for swe-bench dataset JSON, `registry` cache
  for Docker images, both keyed by case set version.

## What `bench/` Is

A **pluggable SWE-bench runner**:

| Piece | Who owns it | What it does |
|-------|-------------|--------------|
| **Cases** | bench (downloads official dataset) | Loads SWE-bench Verified / Lite cases (issue → repo → commit → test_patch → fail_to_pass / pass_to_pass). |
| **Env adapter** | `bench/envs/official/` (Docker) or `bench/envs/self-cow/` (Garive's own) | Provides the agent a place to work. Pluggable — official uses official Docker harness; self-cow uses Garive's own env. |
| **Agent** | injected (Garive's agent, or any compatible runner) | Reads the issue, edits files, produces a patch. |
| **Patch adapter** | `bench/adapters/` | Converts agent output → SWE-bench expected diff format. |
| **Eval** | `bench/eval/` (delegates to official) | Independent env applies patch, runs `fail_to_pass` and `pass_to_pass` test sets. |
| **Tracking** | `bench/tracking/` | Records per-version + per-case pass/fail, score history. |

`bench/` orchestrates these. It does **not** ship its own
agent runtime, its own Docker harness, or its own eval
implementation. Pluggability is the point: every piece above
can be swapped without touching the orchestrator.

## Flow

```
case loader ─→ Env adapter (official | self-cow)
                    │
                    ▼
                Agent (injected)
                    │
                    ▼ patch
              Patch adapter ──→ canonical diff
                    │
                    ▼
              Eval env (independent)
                    │
                    ▼
              Score  ──→ tracking
```

## Layout

```
bench/
├── AGENTS.md                       this file
├── README.md                       overview + flow diagram
├── Cargo.toml                      workspace member
├── src/                            implementation lives here once the slice lands
│   ├── lib.rs                      public surface
│   ├── case.rs                     Case struct, loader (reads official SWE-bench JSON)
│   ├── env.rs                      Env abstraction
│   ├── adapter.rs                  PatchAdapter abstraction
│   ├── runner.rs                   orchestration loop
│   └── main.rs                     CLI entry: bench run | bench conformance | bench track
├── cases/                          official SWE-bench dataset (git submodule or downloaded JSON)
├── envs/
│   ├── official/                   Docker-based env (pulls swe-bench/<id> image, runs docker exec)
│   └── self-cow/                   Garive's own env (no containers; runs directly on the host)
├── adapters/                       agent output → SWE-bench patch format
├── eval/                           calls official swe-bench eval scripts (Python)
├── tracking/                       version + score history (JSONL or SQLite)
└── scripts/                        helpers: case download, eval bootstrap, score reports
```

## Pluggability Contract

These contracts are documented in prose; concrete types are
defined once the implementation lands.

### Env

The abstraction an Env implementation must satisfy:

| Method | What it does |
|--------|--------------|
| `setup(case)` | Bring up a fresh workspace for the given case. Return an opaque handle. |
| `exec(workspace, cmd)` | Run a shell command inside the workspace. Capture stdout / stderr / exit code. |
| `read(workspace, path)` | Read a file from inside the workspace. |
| `write(workspace, path, body)` | Write a file into the workspace. |
| `patch(workspace, diff)` | Apply a unified diff inside the workspace. |
| `teardown(workspace)` | Tear the workspace down. Discard all state. |

`Workspace` is opaque to the orchestrator — `official` makes
it a Docker container id; `self-cow` makes it a temp directory
path. The orchestrator never touches the contents; it only
forwards the handle.

### PatchAdapter

| Method | What it does |
|--------|--------------|
| `adapt(agent_output, case)` | Convert whatever the agent emitted into a unified diff that can be `git apply`'d at `base_commit`. |
| `name()` | Stable identifier for this adapter. Recorded in tracking so score history knows which adapter produced which patch. |

### Case

Mirrors the official swe-bench JSON schema. Fields:

| Field | Source |
|-------|--------|
| `instance_id` | swe-bench |
| `repo` | swe-bench |
| `base_commit` | swe-bench |
| `patch` (gold) | swe-bench — reference only, not the eval target |
| `test_patch` | swe-bench |
| `problem_statement` | swe-bench — what the agent reads |
| `fail_to_pass` | swe-bench — tests that must pass after patch |
| `pass_to_pass` | swe-bench — tests that must still pass after patch |
| env hint / agent config | Garive-side metadata |

These contracts are stable; implementations are pluggable.
The bench runner accepts a config that names the env and
adapter; all other code is shared.

## Official vs Self-cow Mode

| Mode | When to use | Pros | Cons |
|------|-------------|------|------|
| `official` (Docker) | Comparing against SWE-bench published numbers | Reproduces the official environment exactly; matches published scores | Heavy — Docker pull per case; slow; depends on official Docker registry availability |
| `self-cow` (host) | Fast iteration during development; regression runs in CI | Lightweight; no containers; fast | May diverge from official env → scores are *Garive's view*, not directly comparable to published SWE-bench numbers |

A run that targets published comparability must use
`official`. A run that targets Garive-internal regression
should use `self-cow`.

## Eval

`bench/eval/` does **not** implement the evaluator. It calls
the official swe-bench evaluation scripts (the Python harness
that ships with swe-bench Verified / Lite). The official eval
is run in an **independent env** so the patch cannot
contaminate the next case.

## Tracking

`bench/tracking/` records per-version:

```
version  | date  | env   | agent   | cases_passed | cases_total | score
v0.4.1   | 2026-08-27 | official | garive | 47 | 500 | 9.4%
```

Per-case detail (which tests passed, which failed, agent
runtime, token cost) lives alongside, keyed by case id.
Schema is JSONL — append-only, easy to diff between versions.

## What NOT to Do

- ❌ Don't implement the agent inside `bench/`. Inject.
- ❌ Don't reimplement SWE-bench's eval. Call the official
  scripts.
- ❌ Don't conflate env adapters and patch adapters. They
  have separate roles.
- ❌ Don't track scores inside the env / adapter. Tracking
  lives in `bench/tracking/` so it's swappable.
- ❌ Don't run multiple cases in the same env. Per-case env
  lifetime is non-negotiable — leakage between cases is the
  #1 source of fake benchmark wins.

## Build

`bench/` is a Cargo workspace member:

```
just bench                # cargo run -p bench
just conformance          # cargo run -p bench -- conformance
```