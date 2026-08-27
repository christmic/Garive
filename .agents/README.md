# .agents/

> Project-level Agent resource directory.

This directory holds **project-level Agent resources** for Garive. It
is **not** the location of the canonical `AGENTS.md` file (that lives
at the repo root and is read by Codex, Cursor, Claude Code via
`CLAUDE.md`, and other tools). This directory is for the supporting
content that the root `AGENTS.md` `@`-references.

## What Goes Here

| Subdir / file | Purpose |
|---|---|
| `engineering-rules.md` | Project engineering standards (truthfulness, evidence, banned phrases). |
| `git-workflow.md` | Branch / commit / worktree rules. |
| `memory-flush.md` | Project memory conventions. |
| `architecture.md` | Design / architecture docs (index for sub-docs). |
| `conventions.md` | Language, file-operation, banned-phrase rules. |
| `skills/<name>/SKILL.md` | Agent skills (added when needed). |
| `agents/<name>/AGENT.md` | Sub-agent definitions (added when needed). |

## Why a Separate Directory

- Keeps the root `AGENTS.md` thin and easy to scan.
- Lets each rule file evolve independently without churning the
  entry point.
- Matches the pattern used by other multi-agent frameworks where
  `.agents/` holds project-level agent resources alongside the
  canonical entry file at the repo root.

## Status

Filled with the bootstrap rule files (`engineering-rules.md`,
`git-workflow.md`, `memory-flush.md`, `conventions.md`,
`architecture.md`). `skills/` and `agents/` sub-directories will be
added when the project grows an agent runtime that needs them.