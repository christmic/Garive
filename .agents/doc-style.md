# Doc Style

> **Every document in this repo follows the same writing
> conventions.** A blend of Karpathy's "code-first, terse,
> self-contained" engineering-doc style and Google's
> "audience, cross-references, working examples, owners"
> documentation discipline. **`AGENTS.md` and the tier-level
> `<tier>/AGENTS.md` files are exempt** — those are
> agent-tooling entry points with their own conventions
> (top-of-file pointer, `@`-reference chain, etc.) and they
> are read by Claude Code / Codex / Cursor directly.

This file is a **checklist**, not a style guide. Apply it to
every other doc: design docs in `docs/`, spec docs in `spec/`,
READMEs, ADRs, runbooks, tutorials.

## The Skeleton (Karpathy + Google)

Every document under `docs/`, `spec/`, and the per-tier
READMEs follows this structure. Sections may be merged when
genuinely empty; if a section is empty by default, the doc
isn't done.

```
# <Title>

> One-paragraph TL;DR. What is this doc, who needs to read
  it, what does it drive.

## Audience

Who reads this. What they bring. What they don't need
explained. (Google: every doc names its audience.)

## Why

The problem, the motivation, the force that made this doc
necessary. Cite links to issues, prior docs, prior ADRs.
(Karpathy: the *why* comes before the *what* — readers
rationing attention skim to here first.)

## Quick start

The smallest **runnable** example. Code that copies and
works. (Karpathy: example-driven; Google: examples must be
working, not aspirational.)

## Reference

The complete description. Every option, every parameter,
every exit code, every error path. Tables and bullet lists
beat prose for reference material. (Google: reference is
where readers come back to.)

## See also

- Link to related docs.
- Link to relevant `AGENTS.md` / tier docs.
- Link to the spec / design doc that drives this.
```

That's the skeleton. The rest of this file is the **rules**
that apply to every section.

## Rules

### 1. TL;DR is one paragraph, ≤ 4 lines

A reader who reads only this paragraph should know:
- **What** the doc is about.
- **Who** should read it.
- **What decision or action** it drives.

### 2. Audience is explicit

Every doc names its reader:
- "Engineers extending `engine/core/`."
- "Anyone touching `spec/proto/`."
- "Future contributors adding a new bounded context."

If you can't name the reader, the doc probably shouldn't exist.

### 3. Why comes before What

The first non-TL;DR section is **Why**. The reader earns the
context for the design by understanding the problem first.
Reference docs (API listings, table-of-options) can skip
the Why — but only reference docs.

### 4. Examples must run

Every code block in a Quick Start or How-to section is
**runnable**. If a snippet depends on fixtures, link the
fixture. If a snippet doesn't compile, the snippet doesn't
ship.

```
# Good: copy-paste-able
$ cargo run -p engine-proto --example decode_fixture ping.json

# Bad: hand-wave
$ cargo run ...                # ... with appropriate setup
```

### 5. Self-contained where possible

A doc that requires reading three other docs to make sense is
usually a doc that's **asking to be split**, or a doc that's
**asking for a TL;DR at the top of those three others**.

Cross-link freely; do not make cross-link a prerequisite for
understanding.

### 6. Cross-link with relative paths

```
See [engine/AGENTS.md](../engine/AGENTS.md) for tier rules.
See [.agents/testing.md](.agents/testing.md) for the pyramid.
```

Relative paths make the repo portable and the link survives
moves within a tree.

### 7. One source of truth per fact

If a fact lives in `AGENTS.md` (the constitution), link to it
— don't restate it. If a fact lives in `.proto`, link to the
message — don't redefine it. If a fact lives in `spec/`, link
to the section — don't paraphrase.

**Don't restate the constitution. Don't redefine wire types.**
**Don't re-document per-language conventions.** Link instead.

### 8. Banned phrases

From `.agents/engineering-rules.md`, applied to prose:

- "obviously", "simply", "just", "of course"
- "should be fine", "probably passes", "I think it's fixed"
- "we can just" — the word "just" is a flag that the writer
  hasn't thought about edge cases

### 9. Tables beat prose for reference material

Lists of options, parameters, error codes, return values, tier
configurations — **use tables**, not paragraphs. A table
column has a one-word header; a row is one example.

### 10. Errors and exit codes are documented

If the code in this doc can fail, the doc says how it fails:

- HTTP status codes
- Rust `Result::Err` variants
- Exit codes (`0`, `1`, `2`, `64+`)
- Log strings a reader can grep for

A doc that says "the program may fail in various ways" is
failing the doc, not the program.

### 11. Owners and dates on ADRs / design docs

Every ADR / design doc / spec doc has at the bottom:

```
## Meta

- Owner: <github handle or team>
- Last reviewed: <date>
- Status: draft | accepted | superseded | deprecated
```

This is cheap and it stops stale docs from masquerading as
current ones.

### 12. English, plain prose

English for all technical writing — code comments, commit
messages, log strings, doc text. When updating existing
content, follow the document's existing language.

Plain prose beats jargon where jargon isn't necessary. "Sends
a `POST` to `/v1/messages` with the request in the body" beats
"invokes the messages endpoint via HTTP POST semantics" — same
meaning, less friction.

### 13. Doc is complete when sections are filled short

If a section needs paragraphs of explanation, the explanation
probably belongs in another doc and the current one should
link to it.

### 14. Re-read before commit

Read the doc once end-to-end before committing. Check:
- TL;DR says what the doc actually says.
- Every cross-link resolves.
- Every example actually runs (or links to a fixture that
  does).
- No banned phrases.

## When to Skip Sections

| Doc kind | Skippable sections |
|----------|--------------------|
| Quick reference (one-page API listing) | Why, Audience (implied by being in the right repo path) |
| Tutorial (step-by-step walkthrough) | Why, Reference (the tutorial is the reference) |
| Skeleton doc (a stub awaiting content) | All — but mark `[STUB]` at the top and add a Meta block |

Skipping a section is fine. **Skipping it silently is not**;
the section header should be omitted, not left as a placeholder
heading.

## Anti-patterns

- ❌ A doc that opens with "Introduction" or "Overview" — the
  TL;DR is the introduction; a section called "Overview" is a
  TL;DR you didn't write.
- ❌ A doc whose first code block is "set up the environment,
  install X, Y, Z, then..." — the Quick Start is the smallest
  thing that works, not the full bootstrap.
- ❌ A doc that cross-links to a sibling doc without a one-line
  hint about *why* the sibling is relevant. Cross-links are
  navigation; one-line hints are signposts.
- ❌ A doc that survives only because someone copy-pasted it
  from elsewhere. If you can't answer "what decision does this
  drive?" — delete it.
- ❌ A doc whose code block doesn't compile. Aspirational
  examples rot; running examples get verified.
- ❌ A doc without a Meta block. Stale docs are silent debt;
  Meta is the cheap cure.

## Doc Style Self-check

Before committing a doc, run through this list mentally:

- [ ] TL;DR ≤ 4 lines, names the reader, names the decision
- [ ] Audience section exists and names a specific reader
- [ ] Why comes before How (unless this is a reference doc)
- [ ] Quick start code blocks are runnable / link to a fixture
- [ ] Tables for any list of options / parameters / errors
- [ ] Cross-links are relative paths
- [ ] No banned phrases
- [ ] Errors / exit codes documented
- [ ] Meta block present on ADRs / design / spec docs
- [ ] Doc re-read end-to-end before commit

A doc that fails any of these is **not merge-ready**.