# docs/

> Project documentation for humans (design docs, API references,
> runbooks, tutorials).

This directory holds **technical documentation** for the Garive
project. It is separate from `.agents/` (project Agent resources
read by tools) and `.claude/` (Claude Code configuration).

## What Goes Here

| Subdir / file | Purpose |
|---|---|
| `architecture/` | High-level design docs, ADRs, system diagrams. |
| `api/` | API references, protocol specs, wire formats. |
| `runbooks/` | Operational procedures, deployment, troubleshooting. |
| `tutorials/` | Step-by-step guides for new contributors. |
| `design/<feature>.md` | Per-feature design write-ups (markdown). |

## Convention

- Each sub-document leads with a one-paragraph summary, then sections
  for invariants, contracts, and known limitations.
- Cross-link from `.agents/AGENTS.md` or `AGENTS.md` if a doc defines
  rules rather than just describing state.
- Use English for all technical writing (per project conventions).

## Status

Empty placeholder. Add sub-dirs and documents as the project grows.