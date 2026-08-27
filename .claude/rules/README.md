# Path-Scoped Rules

This directory holds **path-scoped rules** for Claude Code — files
that apply only when working on matching paths (e.g. `src/api/**`
rules loaded only when the active file matches that glob).

## When to Add a Rule Here

- A constraint is too narrow to belong in `.agents/AGENTS.md` (which
  applies repo-wide).
- A constraint applies to a specific directory or file type.
- The user wants the rule to be hidden from the global context
  budget until it is relevant.

## File Naming

`<glob>.md` — e.g., `src-api.md` for rules scoped to `src/api/**`.
Claude Code loads the file only when the active path matches.

## Status

**Empty placeholder.** Rules will be added here as the project
gains structure and a stack is chosen.