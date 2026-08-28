# Git Workflow

## Flow (current state — no merge)

Garive currently runs a **rebase-only, worktree-isolated**
flow on `master`. Merge / PR review will be added once the team
grows; today the trunk is `master` and every change lands
there by rebase.

| Change kind | Where it happens | How it lands |
|-------------|------------------|--------------|
| Feature, fix, refactor, behaviour change | `feature/<slug>` (or `fix/<slug>`) on a worktree | rebase + fast-forward onto `master` once verification passes |
| Hotfix to live behaviour | `hotfix/<slug>` on a worktree | rebase + fast-forward onto `master`, then onto any `release/*` |
| Docs-only edits (`*.md` in `docs/`, comment-only changes) | directly on `master` | regular commit, no branch needed |
| Repo maintenance scripts (`.gitignore`, `Justfile`, CI config) | directly on `master` | regular commit, no branch needed |

> The "directly on `master`" lane is intentionally narrow. If a
> doc edit touches any non-doc file, or if a script edit changes
> observable behaviour, it goes through a branch.

## Branch Naming

| Prefix | Use | Example |
|--------|-----|---------|
| `feature/` | New functionality. | `feature/agent-loop`, `feature/wire-ping-pong` |
| `fix/` | A bug fix scoped to one area. | `fix/conformance-fixture-encoding` |
| `hotfix/` | Urgent fix to live behaviour; cherry-pick across `master` + `release/*`. | `hotfix/gateway-auth-bypass` |
| `release/` | Stabilization branch for an upcoming tag. | `release/0.4` |
| `docs/` | Docs-only branch (only when the change is non-trivial; trivial edits go directly on `master`). | `docs/spec-vs-docs-split` |
| `chore/` | Tooling / CI / repo maintenance with product impact — branch, not direct. | `chore/ignore-cargo-lock` |

The **slug names the requirement**, not the file. Multi-file
changes belong on the same branch.

If a branch touches more than one requirement, split it.

## Worktree Isolation (mandatory for feature work)

Every feature / fix / hotfix branch lives in a **dedicated
worktree**, not in the main checkout. The main checkout stays
clean on `master`.

```
~/OraculoSpace/Garive/                 ← main checkout (master)
~/OraculoSpace/Garive.feature-agent/    ← worktree on feature/agent-loop
~/OraculoSpace/Garive.fix-fixture/      ← worktree on fix/conformance-...
```

- Branch from `master` inside the worktree.
- Commit small units.
- Verify (see Before-Rebase Checklist).
- Rebase onto the latest `master` (no merge).
- Fast-forward `master` to the branch tip.

The main checkout never holds in-flight work — it always shows
`master` and is always green.

## Commit Rule

**Small & Frequent Commits**

| Rule | Description |
|------|-------------|
| **Commit Early** | Commit each sub-feature or logical unit separately. Never wait until everything is done to make one giant commit. |
| **Commit Message** | English, verb-first, ≤50 chars, no trailing punctuation. |
| **Atomic** | Each commit must leave the tree in a buildable, testable state. |
| **Scope Tag** | Prefix with a scope when the change is single-area: `<scope>: <verb-phrase>`. Examples: `engine: add Agent trait`, `spec: bump agent id field`, `docs: clarify tier split`. |

## Before-Rebase Checklist

Run inside the worktree, before `git rebase master`:

- `cargo fmt --check` (when Rust code is touched)
- `cargo clippy --workspace --all-targets -- -D warnings` (Rust)
- `cargo test` (Rust)
- `just conformance` (when `spec/proto/`, `engine/proto/`,
  `experiments/kotlin/`, or `mobile/` is touched)
- Self-review: read `git diff master` once before rebasing. If
  anything looks wrong, fix it before the rebase.

All green → rebase, fast-forward `master`, push, drop the
worktree.

## Rebase Flow

Inside the worktree, on `feature/<slug>`:

```
git fetch origin
git rebase origin/master            # replay commits on top of latest master
# resolve any conflicts, then:
git push --force-with-lease origin feature/<slug>
```

Then, from the **main checkout** on `master`:

```
git fetch origin
git merge --ff-only origin/feature/<slug>   # fast-forward only
git push origin master
git worktree remove <path-to-worktree>
git branch -d feature/<slug>                 # local cleanup
```

Key rules:

- **Fast-forward only.** If a fast-forward is not possible,
  rebase again — never merge.
- **`--force-with-lease`, never `--force`.** A force-with-lease
  refuses to clobber a branch tip that moved underneath you.
- **No merge commits into `master`.** `master` is a linear
  history of fast-forwarded branches plus direct edits.
- **Never rebase `master`** itself, never rebase `release/*`.

## Direct-on-`master` Lane (docs / scripts only)

When the change is **strictly** documentation (`.md`) or repo
scripting (`Justfile`, `.gitignore`, CI YAML, hooks) and has
**no behaviour impact**:

1. Work in the main checkout on `master`.
2. Commit in small units (Commit Rule still applies).
3. Push.
4. No branch, no worktree, no rebase.

If the diff touches any non-doc / non-script file — even a
one-line tweak — move it to a branch.

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

## What NOT to Do

- ❌ Do not commit directly to `master` for code, tests, refactors,
  or anything behaviour-affecting.
- ❌ Do not create feature work without a worktree.
- ❌ Do not merge a feature branch into `master` — rebase.
- ❌ Do not create separate branches for each file change.
- ❌ Do not create separate branches for code vs tests vs docs
  that belong to the same requirement.
- ❌ Do not wait until everything is done to make one giant
  commit.
- ❌ Do not amend published commits without coordination.
- ❌ Do not force-push with `--force`; use `--force-with-lease`.
- ❌ Do not rebase `master` or any `release/*` branch.

## Example Flow

```
master  ─────────────────────────────────────────►  (linear history)

       feature/agent-loop  (worktree: ../Garive.feature-agent/)
         ├── commit: engine: add Agent trait skeleton
         ├── commit: engine: add loop driver with bounded turns
         ├── commit: engine: add cancellation token plumbing
         ├── commit: engine: add unit tests for happy path
         ├── commit: engine: add conformance fixture for loop
         ├── [verify: fmt + clippy + test + conformance]
         ├── git fetch + rebase origin/master
         ├── git push --force-with-lease
         └── (main checkout) git merge --ff-only origin/feature/agent-loop
                                 git push origin master
                                 worktree remove
                                 branch -d feature/agent-loop

       docs: clarify tier split           (direct commit on master)
       chore: update .gitignore           (direct commit on master)
```