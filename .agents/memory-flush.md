# Memory Flush

> Project-internal memory conventions. When Garive grows a runtime
> memory layer or a per-session journal, the rules below describe how
> to keep it clean.

## When to Flush

| Trigger | Description |
|---------|-------------|
| **Explicit command** | User says `/summary` or equivalent. |
| **Context compression** | Working context reaches ~70% of its budget. |
| **Task start / end** | Start or complete a long or complex task. |
| **Exit signal** | "done", "out", "later", "closing", or language-equivalent. |

## How to Flush

1. **Archive & reset**: when the active memory's date stamp mismatches
   the system date, move the active file to `archived/<date>.md` and
   reset the active file with the current date header. Preserve any
   template comments in the original file before overwriting.
2. **Write a fresh entry**: capture in-flight task IDs (`SN-x`
   numbering), open decisions, intermediate result locations, and the
   next-step plan.
3. **Re-anchor on resume**: after flush, a fresh session reads the
   active memory file, confirms SN numbering, and picks up the next
   step.

## Must Not

- Must not start work without confirming the active memory's date
  matches the system date.
- Must not write new content to the active memory file when the date
  mismatches — archive first.
- Must not duplicate SSOT entries across multiple files.