# bench/eval/

> **Calls the official SWE-bench eval scripts.** Does not
 implement evaluation; delegates to the Python harness
 that ships with swe-bench.

## Why Delegate

The official swe-bench eval (the Python script that runs
`fail_to_pass` + `pass_to_pass` test sets) is the source of
truth for what "passing" means. Reimplementing it here would
introduce drift and produce numbers that don't match the
published benchmarks.

## Lifecycle

1. **Bootstrap externally**: supply an explicit Python executable containing
   pinned SWE-bench revision `7a21e05772954cc81471ae19d56f436cecf43c54`.
2. **Per-case run**: in an **independent env** (a fresh
   container / temp dir; never the agent's workspace) apply
   the patch, then `python -m swebench.harness.run_evaluation ...`
   with the case's `fail_to_pass` / `pass_to_pass` lists.
3. **Score**: parse the eval output (which tests passed, which
   failed) into a `CaseResult` and hand it to `tracking/`.

## Independent-env Rule

The eval env is created **fresh per case**. State from one
case (installed deps, build artefacts, .git history) must
not leak into the next. This is non-negotiable — leakage is
the #1 source of fake benchmark wins.

## Config

The explicit B0 JSON run config controls:

- swe-bench repo tag to pin to.
- Python version / venv bootstrap script.
- Test-timeout and per-test retry policy.
- Network policy (eval should run with **no** network).

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: official invocation/report boundary implemented; real Docker run gated.
