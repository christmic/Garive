# bench/adapters/

> **Two translations** between the case / eval world and the
> agent's native input / output. Plug in a pair of adapters
> per agent.

| Adapter | Translates | Sub-directory |
|---------|------------|---------------|
| **Intake** | case + env state → agent-consumable input | `intake/` |
| **Prefication** | agent output → canonical unified diff | `patch/` |

## Why Two Adapters, Not One

The driver loop has **two** translation points, and they are
independent concerns. Splitting them:

- Different agents have different **input** expectations
  (intake). One agent takes a Markdown issue; another takes a
  JSON RPC; another takes a chat message. Each needs its own
  intake adapter.
- Different agents have different **output** expectations
  (prefication). One returns raw `diff -u`; another returns
  full file rewrites; another returns search-and-replace
  blocks; another returns a structured patch object. Each
  needs its own prefication adapter.
- Mixing the two would force every new agent to ship both
  adapters in one place. Keeping them separate means a new
  agent ships **two small focused files** instead of one
  tangled one.

## Per-agent Adapter Pairs

| Agent | Intake adapter | Prefication adapter |
|-------|----------------|---------------------|
| Garive agent | `intake/garive/` | `patch/garive-bridge/` |
| (other agent A) | `intake/<name>/` | `patch/<name>/` |
| (other agent B) | `intake/<name>/` | `patch/<name>/` |
| no-op (testing the runner itself) | `intake/noop/` | `patch/noop/` |

## Driver Loop Reference

```
case ─┐
      │
env_setup ─┐
          ▼
    [intake adapter]  ──→  raw_input
                              │
                              ▼
                          agent.run(raw_input, env_setup)
                              │
                              ▼
                          raw_output
                              │
                              ▼
                     [prefication adapter]  ──→  diff
                                                    │
                                                    ▼
                                                eval.run(diff, case)
```

Both adapters are wired into `bench/src/runner.rs`; their
concrete types are dispatched by adapter name from the run
config (`--intake garive --patch garive-bridge` etc.).