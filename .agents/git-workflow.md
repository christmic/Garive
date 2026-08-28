# Git Workflow

## Branching Model

Garive uses a **trunk-based** model on `master`:

- **`master`** is the trunk. Always green, always deployable.
- **Long-lived branches** are only used for major-version
  stabilization (`release/1.x`).
- **Short-lived feature / fix branches** carry every change,
  merged back via PR / fast-forward.

| Branch | Lifetime | Branched from | Merges into |
|--------|----------|---------------|-------------|
| `master` | permanent | — | — |
| `release/<major>.<minor>` | weeks–months | `master` | `master` (tag only) |
| `feature/<slug>` | days | `master` | `master` (PR) |
| `fix/<slug>` | hours–days | `master` | `master` (PR) |
| `hotfix/<slug>` | hours | `master` (or `release/*` for live fixes) | both targets |

## Branch Naming

| Prefix | Use | Example |
|--------|-----|---------|
| `feature/` | New functionality. | `feature/agent-loop`, `feature/wire-ping-pong` |
| `fix/` | A bug fix scoped to one area. | `fix/conformance-fixture-encoding` |
| `hotfix/` | Urgent fix to live behavior; cherry-pick across `master` + `release/*`. | `hotfix/gateway-auth-bypass` |
| `release/` | Stabilization branch for an upcoming tag. | `release/0.4` |
| `docs/` | Docs-only change (also fine on `feature/`). | `docs/spec-vs-docs-split` |
| `chore/` | Tooling / CI / repo maintenance with no product impact. | `chore/ignore-cargo-lock` |

The **slug names the requirement**, not the file. Multi-file
changes belong on the same branch.

If a branch touches more than one requirement, split it.

## Commit Rule

**Small & Frequent Commits**

| Rule | Description |
|------|-------------|
| **Commit Early** | Commit each sub-feature or logical unit separately. Never wait until everything is done to make one giant commit. |
| **Commit Message** | English, verb-first, ≤50 chars, no trailing punctuation. |
| **Atomic** | Each commit must leave the tree in a buildable, testable state. |
| **Scope Tag** | Prefix with a scope when the change is single-area: `<scope>: <verb-phrase>`. Examples: `engine: add Agent trait`, `spec: bump agent id field`, `docs: clarify tier split`. |

## Before Push Checklist

- `cargo fmt --check` (when Rust code is touched)
- `cargo clippy --workspace --all-targets -- -D warnings` (Rust)
- `cargo test` (Rust)
- `just conformance` (when `spec/proto/`, `engine/proto/`,
  `experiments/kotlin/`, or `mobile/` is touched)
- Self-review: read `git diff master` once before pushing. If
  anything looks wrong, fix it before the PR.

## Merge Flow

1. Branch from `master`.
2. Commit in small units (see Commit Rule).
3. Push and open a PR against `master`.
4. CI runs the Before-Push Checklist plus the cross-language
   conformance gate.
5. Reviewer signs off.
6. **Squash-merge** by default — keeps `master` history linear
   and one commit per landed feature. Use a regular merge only
   when the per-commit history carries reviewer-relevant
   context.
7. Delete the branch on merge.

## Rebase Strategy

- **Rebase interactively before review**, not after. Force-push
  during the branch's lifetime is fine; force-push after a
  branch is shared is forbidden.
- **Never rebase `master`** or any `release/*` branch.
- **Linear history** is preferred. Avoid merge commits from
  feature branches.

## Tagging

- Semantic versioning: `MAJOR.MINOR.PATCH`.
- Tags are created from `master` (or from a `release/*` branch
  during stabilization) and pushed with `git push origin <tag>`.
- A `v` prefix is conventional: `v0.4.1`.
- Annotated tags only (`git tag -a`). Lightweight tags are for
  local scratch.
- Breaking changes bump MAJOR. Additive changes bump MINOR.
  Bug fixes bump PATCH. See `spec/AGENTS.md` for schema-version
  coordination.

## Isolation

**Use a worktree per non-trivial slice.**

For isolated sub-feature work, use a worktree to keep the main
checkout clean. Branch, commit, push; merge back when green.

## What NOT to Do

- ❌ Do not create separate branches for each file change.
- ❌ Do not create separate branches for code vs tests vs docs
  that belong to the same requirement.
- ❌ Do not commit directly to `master` or `release/*`.
- ❌ Do not wait until everything is done to make one giant
  commit.
- ❌ Do not amend published commits without coordination.
- ❌ Do not force-push to a branch that others are working on.
- ❌ Do not merge your own PR without a reviewer (exception: solo
  branches under `chore/` or `docs/` with no code impact).

## Example Flow

```
master
  │
  ├── feature/agent-loop
  │     ├── commit: engine: add Agent trait skeleton
  │     ├── commit: engine: add loop driver with bounded turns
  │     ├── commit: engine: add cancellation token plumbing
  │     ├── commit: engine: add unit tests for happy path
  │     ├── commit: engine: add conformance fixture for loop
  │     └── PR + squash-merge → master
  │
  └── feature/conformance-gate
        ├── commit: spec: add ping fixture v1
        ├── commit: bench: add conformance binary (Rust)
        ├── commit: experiments/kotlin: add conformance runner
        ├── commit: Justfile: wire conformance target
        └── PR + squash-merge → master
```