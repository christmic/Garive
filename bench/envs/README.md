# bench/envs/

> **Pluggable environments for the agent to work in.**
>
> Two concrete implementations:
>
> - `official/` — Docker harness that matches the swe-bench
>   published numbers.
> - `self-cow/` — host-based, no containers, fast.
>
> The orchestrator never picks the env directly — it reads
> the run config (`--mode official | --mode self-cow`) and
> constructs the matching impl.

## Env contract (prose)

An env implementation must support:

| Method | What it does |
|--------|--------------|
| `setup(case)` | Bring up a fresh workspace for the given case. Return an opaque handle. |
| `exec(workspace, cmd)` | Run a shell command inside the workspace. Capture stdout / stderr / exit code. |
| `read(workspace, path)` | Read a file from inside the workspace. |
| `write(workspace, path, body)` | Write a file into the workspace. |
| `patch(workspace, diff)` | Apply a unified diff inside the workspace. |
| `teardown(workspace)` | Tear the workspace down. Discard all state. |

`Workspace` is opaque to the orchestrator. `official` makes
it a Docker container id; `self-cow` makes it a temp directory
path. The orchestrator never touches the contents; it only
forwards the handle.

## Pooling & Parallelism (mandatory for future impl)

Per-case image pulls and dependency installs are the dominant
runtime cost. **Pooling + parallelism is the design, not an
optimisation.** The future implementation must ship all three
of these together; running cases serially with cold pulls is
forbidden for any official / published score run.

### 1. Image pool (`official` mode)

A **warm pool of swe-bench Docker images** keeps N copies
pulled and idle locally. Per-case setup is a `docker run` from
local cache, not a fresh network `docker pull`.

| Knob | Default | Notes |
| |
| Pool size | `--jobs N` (same as concurrency) | At least `N` images warm so the pool never starves. |
| Eviction | LRU when over-capacity | Cold images stay on disk until disk-pressure pruning runs. |
| Pre-pull | background worker before run start | Optional `--pre-pull all` mode pulls every case image before the run starts. |
| Failure | on pull failure, mark case as `INFRA_ERROR`, do not count | Infrastructure errors are tracked separately; they do not silently count as benchmark fails. |

Why this matters: a cold swe-bench Verified run with 500
cases pulls ~500 GB of images serially and takes hours. A
warmed pool with N=16 concurrency runs the same suite in
minutes.

### 2. Workspace pool (`self-cow` mode)

A pool of **pre-cloned repo workspaces at known commits**.
After a run, the workspace is wiped (`git clean -fdx` + drop
untracked) and returned to the pool — **not re-cloned**. This
makes per-case setup ~ms instead of seconds.

| Knob | Default | Notes |
| |
| Pool size | `--jobs N` | One workspace per concurrent case. |
| Storage | per-run temp root under `bench/tracking/workspaces/<run_id>/` | Each run is isolated; old workspaces are GC'd after the run finishes. |
| Re-warm | after teardown, restore base_commit + clean state | Verify by `git rev-parse HEAD` matching the case's `base_commit`. |

### 3. Bounded case parallelism

Cases run concurrently via async tasks (Tokio). Concurrency
is bounded by `--jobs N`, and by per-env limits.

| Knob | Default | Notes |
| |
| `--jobs N` | `min(num_cpus, 8)` | Total cases in flight. Tune up if env can sustain it. |
| Per-env cap | env-specific | `official`: Docker daemon's practical limit (~16 containers before IO thrashes); `self-cow`: disk + CPU. |
| Backpressure | queue up to `2 * N` cases | Beyond that, wait for one to finish before queuing more. |
| Fair scheduling | FIFO by `instance_id` | Deterministic order makes diffing run logs easier. |

The orchestrator does not pick a scheduling policy beyond
FIFO; smarter scheduling (priority by difficulty, early
exits) is a future optimisation.

### Forbidden for Official / Published Runs

- Sequential case execution (one at a time).
- Cold image pull per case.
- Re-cloning a workspace from origin per case.
- Skipping concurrency limits in the name of "stability" — if
  the env can't sustain N concurrent cases, lower N; don't
  drop to 1.

These rules apply to **every run that produces a score
recorded in `bench/tracking/versions/`**. Dev-loop and
local-iteration runs are exempt — there you can run a single
case end-to-end with cold pulls to debug.

### 4. CI / GitHub Actions integration

The pool is sized for the **runner**, not the laptop.
Workflow files live in `.github/workflows/`:

| Workflow | Runner | Mode | Set | Purpose |
|---------|--------|------|-----|---------|
| `bench-ci.yml` | GH-hosted (ubuntu-latest) | `self-cow` | small smoke subset (~10 cases) | per-PR regression gate; fail fast |
| `bench-nightly.yml` | GH Actions self-hosted | `self-cow` | full suite | nightly score, posted to tracking branch |
| `bench-release.yml` | GH Actions self-hosted | `official` | full suite (Verified) | manual trigger; published score |

Pool sizing rules:

- **GH-hosted runners** cap at 14 GB disk and 6 h job time.
  That is **not enough** for `official` (swe-bench Verified
  alone is ~500 GB of images). `official` lives on
  self-hosted only.
- **Self-hosted runners** are sized for the workload:
  ≥ 1 TB disk, 32+ cores, ≥ 24 h timeout, Docker daemon with
  nested-Docker if the env needs it.
- `--jobs N` on a CI runner defaults to the runner's
  advertised `cpu_count`, capped by per-env limits.

Cross-run caching:

- `actions/cache` for the swe-bench dataset JSON files, keyed
  by dataset tag.
- Docker registry cache (`type=registry` in buildx) for
  image layers, so cross-job cache survives even when the
  runner VM is recycled.
- The **image pool persists between jobs on a long-lived
  self-hosted runner**. If the runner is ephemeral, we lose
  the pool between runs — that is a known cost and an
  argument for self-hosted.

Pool is **per host**, not shared across a matrix. Using
`strategy.matrix` to fan out across many runners means each
runner has its own cold pool; either accept that cost, or
pre-warm via a setup job.

Output integration:

- JSONL progress to stdout (workflow step log).
- Per-case detail archived as a workflow artifact.
- Version summary committed by the workflow to a dedicated
  `bench-tracking` branch (so score history survives runner
  replacement and is visible in the repo).
- Workflow fails only on **infrastructure** errors (image
  pull failure, eval harness crash, network timeout).
  Agent underperformance produces a low score, not a CI
  failure.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: explicit bounded command broker port implemented; concrete pools are deployment-owned.
