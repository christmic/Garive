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