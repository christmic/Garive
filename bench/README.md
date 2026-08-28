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
                │ Case loader │  ← SWE-bench Verified / Lite
                └──────┬──────┘
                       ▼
            ┌────────────────────┐
            │   Env adapter      │  ← official (Docker) | self-cow (host)
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
            │   Tracking         │  ← per-version + per-case JSONL
            └────────────────────┘
```

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