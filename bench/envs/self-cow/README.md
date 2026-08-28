# bench/envs/self-cow/

> **Host-based env.** No containers. The agent works on a
> workspace directory on the host. Fast iteration; cheap CI.
>
> Scores are **Garive-internal** — they are not directly
> comparable to published SWE-bench numbers because the env
> is not identical to the official Docker image.

## Why

Fast. Per-case env lifecycle is microseconds, not minutes.
Suits regression suites that need to run on every PR and
dev-loop iterations against the agent.

## Lifecycle

1. **Setup**: clone `<repo>` at `<base_commit>` into a temp
   directory under `bench/tracking/workspaces/<run_id>/<case_id>/`.
2. **Work**: agent issues commands via `tokio::process::Command`.
   Reads / writes are direct filesystem ops.
3. **Patch apply**: at eval time, `git apply` the agent's diff
   inside the temp directory.
4. **Teardown**: `fs::remove_dir_all`.

## Trade-offs vs `official`

| | self-cow | official |
|--|----------|----------|
| Speed | seconds per case | minutes per case (cold image pull) |
| Env fidelity | host — may diverge from official image | exact |
| Comparable to published | no | yes |
| Suitable for CI | yes | only on a long schedule |
| Suitable for dev loop | yes | no |
| Risk | dependency drift, host toolchain assumptions | Docker pull failure, image availability |

## Config

`bench/config/self-cow.toml` (placeholder) controls workspace
root, per-case resource limits, and toolchain assumptions
(pinned Python / Node / Go versions etc.).

## When to Use

- CI regression on every PR.
- Comparing agent versions against each other.
- Quick A/B of patch adapter implementations.
- Daily / nightly smoke against a small case subset.

Switch to `official` whenever the goal is to publish a number
that someone else might compare against.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
