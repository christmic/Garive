# bench/

> **SWE-bench verification runtime for Garive.** Loads official
> SWE-bench cases, runs the agent inside a pluggable
> environment, captures the patch, evaluates against the
> official test sets, and tracks per-version scores.

`bench/` is **only an orchestrator**. The agent, the
environment, the patch format adapter, and the eval are all
injected — `bench/` itself ships none of them.

## Flow

```
                ┌─────────────┐
                │ Case loader │  ← SWE-bench Verified / Lite / Multimodal / Multilingual,
                │             │    Terminal-Bench, … (official public datasets only)
                └──────┬──────┘
                       ▼
            ┌────────────────────┐
            │   Env adapter      │  ← official (Docker, image pool) | self-cow (host, workspace pool)
            └────────┬───────────┘
                     ▼
               ┌──────────┐
               │  Agent   │  ← injected (Garive agent or compatible)
               └────┬─────┘
                    │ patch (agent-shaped)
                    ▼
            ┌────────────────────┐
            │  Patch adapter     │  ← unifies to SWE-bench diff
            └────────┬───────────┘
                     ▼
            ┌────────────────────┐
            │   Eval (independent env) │
            │   apply patch + run     │
            │   fail_to_pass +        │
            │   pass_to_pass          │
            └────────┬───────────┘
                     ▼
            ┌────────────────────┐
            │   Tracking         │  ← per-version + per-case JSONL;
            │                    │    committed to a tracking branch from CI
            └────────────────────┘
```

## Where It Runs

| Run kind | Where | When |
|----------|-------|------|
| Dev loop | developer laptop | iterating on agent / patch adapter |
| PR smoke | GH Actions hosted runner | per-PR, small subset, `self-cow`, fail fast |
| Nightly | GH Actions self-hosted | nightly full-suite `self-cow`, posts score |
| Release | GH Actions self-hosted | manual trigger, full-suite `official` |

Workflow files live in `.github/workflows/`. Any score that
lands in `bench/tracking/versions/` is produced on a CI runner
— local runs are dev-loop only. See `bench/AGENTS.md` Rule 3.

## Pieces

| Module | Role |
|--------|------|
| `cases/` | Loads official SWE-bench JSON (Verified / Lite). Git submodule or downloaded dataset. |
| `envs/official/` | Docker-based env: pulls swe-bench image, runs commands via `docker exec`. |
| `envs/self-cow/` | Garive's own env: chroot / workspace dir on the host, no containers. Fast iteration. |
| `adapters/` | Converts agent output → SWE-bench canonical unified diff. |
| `eval/` | Calls the **official** swe-bench eval scripts (Python). Runs in an independent env. |
| `tracking/` | Per-version + per-case pass/fail, score, runtime, token cost. Append-only JSONL. |
| `scripts/` | Helpers: case download, eval bootstrap, score report. |

## Modes

| Mode | When | What |
|------|------|------|
| `official` | Comparing Garive against published SWE-bench numbers | Docker harness; matches official env; slow but reproducible |
| `self-cow` | Fast iteration, CI regression, internal comparison | No containers; lightweight; fast |

`official` runs are slow and heavyweight. `self-cow` is the
default for development and CI. Switch via config.

## Usage

```
# Run the default benchmark set
just bench

# Run only conformance / regression
just conformance

# Compare against official SWE-bench numbers
just bench --mode official --set verified
```

See `bench/AGENTS.md` for the full rules and conventions.