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

1. **Setup**: `docker pull swe-bench/<instance_id>:<tag>`,
   start the container, capture container id.
2. **Work**: agent issues commands via `docker exec <id> <cmd>`.
   Reads / writes use `docker cp` or `docker exec cat/echo`.
3. **Patch apply**: at eval time, `git apply` the agent's
   diff inside the container.
4. **Teardown**: `docker rm -f <id>`.

## Limitations

- Per-case `docker pull` is slow — first run of a new case
  can take 5–10 minutes for image fetch.
- Requires Docker daemon; no nested Docker-in-Docker.
- The published SWE-bench Docker harness only supports Linux
  x86_64 — macOS runners use `colima` or `Docker Desktop`.

## Config

`bench/config/official.toml` (placeholder) controls image
registry, runtime args, network policy, and resource limits.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
