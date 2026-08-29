# bench/envs/official/

> **Docker-based env.** Pulls the swe-bench Docker image for
> each case (`swe-bench/<instance_id>`) and runs the agent
> inside via `docker exec`. This is the **only** env that
> produces scores directly comparable to the published
> SWE-bench numbers.

## Why

The official swe-bench harness ships per-instance Docker
images with the repo checked out at `base_commit`, all
dependencies resolved, and the test commands wired. Running
the agent inside that image matches the environment the
published scores were measured against.

## Lifecycle

1. **Warm pool**: an external explicit environment broker pre-pulls and owns at
   least `jobs` isolated workspaces/containers.
2. **Acquire**: bind one warm workspace to the exact instance/base commit.
3. **Work**: agent issues commands through its injected driver.
   Reads / writes use `docker cp` or `docker exec cat/echo`.
4. **Patch apply**: the independent official harness applies the canonical
   diff inside the container.
5. **Release**: the broker cleans and returns the workspace exactly once.

## Limitations

- Pool bootstrap remains external and can be expensive; per-case acquisition
  must not perform an implicit image pull in published mode.
- Requires Docker daemon; no nested Docker-in-Docker.
- The published SWE-bench Docker harness only supports Linux
  x86_64 — macOS runners use `colima` or `Docker Desktop`.

## Config

The explicit B0 JSON run config supplies the broker command, post-clear
environment, timeouts, output bounds and warm capacity. The broker owns Docker
configuration; B0 does not discover it.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: command-port contract implemented; real warm Docker broker is deployment-owned.
