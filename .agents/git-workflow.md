# Git Workflow

## Branch Rule

**One Feature Branch Per Requirement**

| Rule | Description |
|------|-------------|
| **One Branch Per Requirement** | One complete requirement = one feature branch. All related changes (code, tests, docs) go into the same branch. |
| **Branch Naming** | `feature/<short-slug>` (e.g., `feature/agent-loop`, `feature/cache-eviction`). Use a slug that names the requirement, not the file. |
| **Branch From Master** | Always create feature branches from `master`. Never commit directly to `master`. |

## Commit Rule

**Small & Frequent Commits**

| Rule | Description |
|------|-------------|
| **Commit Early** | Commit each sub-feature or logical unit separately. Never wait until everything is done to make one giant commit. |
| **Commit Message** | English, verb-first, ≤50 chars, no trailing punctuation. |
| **Atomic** | Each commit must leave the tree in a buildable, testable state. |

## Isolation

**Use a worktree per non-trivial slice.**

For isolated sub-feature work, use a worktree to keep the main checkout
clean. Branch, commit, push; merge back when green.

## What NOT to Do

- ❌ Do not create separate branches for each file change.
- ❌ Do not create separate branches for code vs tests vs docs that
  belong to the same requirement.
- ❌ Do not commit directly to `master`.
- ❌ Do not wait until everything is done to make one giant commit.
- ❌ Do not amend published commits without coordination.

## Example Flow

```
feature/agent-loop
  ├── commit 1: Add Agent trait and default impl
  ├── commit 2: Add loop driver with bounded turns
  ├── commit 3: Add cancellation token plumbing
  ├── commit 4: Add unit tests for happy path
  └── commit 5: Update architecture notes
```

## Rationale

- Small commits are easier to review and roll back.
- One branch per requirement keeps related changes together.
- Isolation prevents contamination of the main checkout during
  in-flight work.
- Enables parallel work on independent requirements.