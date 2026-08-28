# bench/adapters/

> **Agent-output → SWE-bench patch format.**
>
> Different agents emit patches in different shapes — unified
> diff, full file rewrites, search-and-replace, structured
> JSON edits, etc. The adapter normalizes whatever the agent
> produces into the **SWE-bench canonical unified diff** that
> the eval harness expects.

## Contract (prose)

| Method | What it does |
|--------|--------------|
| `adapt(agent_output, case)` | Convert whatever the agent emitted into a unified diff that can be `git apply`'d at `base_commit`. |
| `name()` | Stable identifier for this adapter. Recorded in tracking so score history knows which adapter produced which patch. |

## Per-agent Adapters

| Adapter | Use when the agent emits |
|---------|--------------------------|
| `unified-diff/` | raw `diff -u` output |
| `search-replace/` | search-and-replace blocks |
| `file-rewrite/` | full file contents (must compute diff) |
| `garive-bridge/` | Garive agent's native patch struct (the canonical one) |
| `noop/` | no-op adapter (for testing the runner itself) |

Add a new adapter by registering it in `bench/src/adapter.rs`
dispatch.

## Why Separate From the Agent

A patch adapter is **not** part of the agent. The agent's job
ends at producing an output; the adapter's job is to make that
output apply cleanly at `base_commit`. Keeping them separate
means we can mix any agent with any adapter, and we can fix
adapter bugs without touching agent code.