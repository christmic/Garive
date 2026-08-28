# bench/

> **Thin benchmark driver for Agent SWE-style evaluations.**
> Four steps: **load → drive → adapt → eval**. Plugs in the
> official public datasets, the agent under test, and the
> official swe-bench eval scripts. Designed for GitHub
> Actions.

`bench/` itself is a thin orchestrator. It does not implement
the agent, the environment, or the eval.

## The Four Steps

```
   ┌────────────┐    ┌──────────┐    ┌─────────────┐    ┌──────┐
   │   LOAD     │ →  │  DRIVE   │ →  │   ADAPT     │ →  │ EVAL │
   │  cases     │    │  loop:   │    │  intake +   │    │ call │
   │  from      │    │  intake  │    │  prefication│    │ offi-│
   │  official  │    │  run     │    │  translate  │    │ cial │
   │  datasets  │    │  collect │    │             │    │ swe- │
   │            │    │  diff    │    │             │    │ bench│
   └────────────┘    └──────────┘    └─────────────┘    └──────┘
   cases/            runner          adapters/          eval/
```

| Step | Module | What it does |
|------|--------|--------------|
| **1. LOAD** | `cases/` | Reads cases from official public datasets (SWE-bench Verified / Lite / Multimodal / Multilingual, Terminal-Bench, …). Immutable input. |
| **2. DRIVE** | `src/runner.rs` | Thin loop. For each case: setup env, run agent, collect raw output. No translation happens here. |
| **3. ADAPT** | `adapters/intake/` + `adapters/patch/` | Two translations: **intake** (case → agent input), **prefication** (agent output → canonical unified diff). |
| **4. EVAL** | `eval/` | Calls the official swe-bench eval scripts in an independent env. Computes score. |

## Driver Loop (Step 2)

`src/runner.rs` runs the following per case. There is **no
other code path**:

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

Every recorded score came through this loop.

## Adapters (Step 3)

| Adapter | Translates | Lives in |
|---------|-------------|----------|
| **Intake** | case + env state → agent-consumable input | `adapters/intake/` |
| **Prefication** | agent output → canonical unified diff | `adapters/patch/` |

Both are pluggable. New agents → add an intake adapter + a
prefication adapter; nothing else in `bench/` changes.

## Env Adapters (the "run" in the loop)

| Env | Use |
|-----|-----|
| `envs/official/` | Docker harness; matches published swe-bench numbers. |
| `envs/self-cow/` | Host-based; fast; dev-loop and nightly. |

## Eval (Step 4)

`eval/` does **not** implement evaluation. It calls the
official swe-bench Python harness in an **independent env**.

## Where It Runs

| Run kind | Where | When |
|----------|-------|------|
| Dev loop | developer laptop | iterating on agent / adapter |
| PR smoke | GH Actions hosted runner | per-PR, small subset, `self-cow`, fail fast |
| Nightly | GH Actions self-hosted | nightly full-suite `self-cow`, posts score |
| Release | GH Actions self-hosted | manual trigger; full-suite `official` |

Workflow files live in `.github/workflows/`. Local runs are
dev-loop only. See `bench/AGENTS.md` Rule 3.

## Usage

```
just bench                          # full default run
just bench -- --source swe-bench-verified --mode official
just bench -- --source terminal-bench --mode self-cow --jobs 16
```

See `bench/AGENTS.md` for the full rules.