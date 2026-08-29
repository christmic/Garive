# bench/adapters/patch/

> **Prefication adapter.** Translates the **agent's native
> output** into a **canonical unified diff** that the
> swe-bench eval harness can apply with `git apply`.

## Why

Different agents emit patches in different shapes:

| Agent | Output shape |
|-------|--------------|
| Direct-diff agents | raw `diff -u` output |
| Full-file-rewrite agents | whole file contents (must compute the diff) |
| Search-and-replace agents | search/replace blocks |
| Structured-patch agents | JSON object: `{ "edits": [...] }` |
| Native-patch agents (e.g. Garive) | a `Patch` struct with `FileEdit` entries |

The eval harness **only** consumes a unified diff. Anything
else must be normalized — the prefication adapter is that
normalizer.

## Contract (prose)

| Method | What it does |
|--------|--------------|
| `translate(agent_output, case) → unified_diff` | Produce a unified diff that, when applied at `case.base_commit`, reflects exactly what the agent changed. May need to diff two trees (full-file case) or build the diff from structured edits. |
| `name()` | Stable identifier. Recorded in tracking. |

## Per-agent Implementations

| Adapter | Agent |
|---------|-------|
| `garive-bridge/` | Garive agent's native `Patch` struct → unified diff |
| `unified-diff/` | raw `diff -u` pass-through |
| `search-replace/` | search-and-replace blocks |
| `file-rewrite/` | full-file contents → diff against `case.base_commit` |
| `noop/` | no-op (testing the runner itself) |

A new agent → a new sub-directory + a manifest entry.

## Why Separate From the Agent

A prefication adapter is **not** part of the agent. The
agent's job ends at producing an output; the adapter's job
is to make that output apply cleanly at `base_commit`. Keeping
them separate means:

- Any agent can be paired with any prefication adapter.
- Adapter bugs are fixed without touching agent code.
- New agents only need to ship two small adapters — they don't
  have to learn the swe-bench eval harness.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: bounded repository-relative unified-diff V1 implemented.
