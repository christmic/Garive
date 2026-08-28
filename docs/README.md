# docs/

> **思考和设计.** Where ideas live before they become concrete
> specs. Exploratory notes, design write-ups, ADRs, runbooks,
> tutorials — the human-facing, **non-normative** side of the
> project.

This is the **deliberative** layer. Once an idea hardens into a
contract that must be implemented faithfully, it moves to `spec/`.
Until then, it lives here — even if it is rough, partial, or
wrong.

## The Garive Doc Hierarchy

```
design (here)   →   spec   →   agents (constitution)   →   tier AGENTS   →   code
  docs/              spec/                .agents/            <dir>/AGENTS.md
  natural language   normative contract    rules that apply    tier-specific overrides
  human-edited       machine-checked       to every tier       of the constitution
```

`docs/` is **stage 1**. Writing in `docs/` is how a design
**starts**: you have an idea, you compare options, you record
trade-offs in plain prose. The output may be wrong, may be
abandoned, may be superseded — that's fine, this is the
layer where iteration is cheap.

## What Goes Here

| Subdir / file | Purpose |
|---|---|
| `architecture/` | High-level design docs, system diagrams, exploratory sketches. |
| `adrs/` | Architecture Decision Records — context, decision, consequences. |
| `api/` | API walkthroughs and tutorials (not the contract; the contract lives in `spec/`). |
| `runbooks/` | Operational procedures, deployment, troubleshooting. |
| `tutorials/` | Step-by-step guides for new contributors. |
| `<feature>.md` | Per-feature design write-ups in flight. |

## What Does NOT Go Here

- Wire schemas. Those belong in `spec/proto/`.
- Implementation contracts or invariants. Those belong in `spec/`.
- Anything machine-read that other repos / crates depend on.

## Design-Doc Template

Every design doc under `docs/` follows this skeleton. Sections
may be merged or omitted only when there is genuinely nothing to
say; if a section is empty by default, it usually means the
doc isn't done.

```
# <Title>

> One-paragraph summary — what is this doc about, who needs
  to read it, what decision does it drive.

## Context

The problem / opportunity / force that motivates this work.
Cite links to issues, prior docs, prior decisions (ADRs).

## Options Considered

For each option (usually 2–4):

- **What it is.** One sentence.
- **Pros.**
- **Cons.**
- **Cost.** Time, risk, blast radius.

## Decision

Which option we picked, and why. Reference the discussion
in the previous section. This is the section a future reader
who has no time skims to.

## Consequences

What becomes easier, what becomes harder, what we now have to
do, what we now cannot do. Include operational consequences
(deploys, monitoring, runbooks to update) and codebase
consequences (new crates, new AGENTS.md, new spec).

## Open Questions

Things still undecided, with a clear owner and a deadline or
trigger that resolves them.

## Known Limitations

What this doc does *not* solve. What we knowingly punted.
What we'd need to revisit if X changes.
```

### Style

- English. Plain prose.
- Concrete, not abstract. Names of crates, files, runtimes.
- Banned phrases (`.agents/engineering-rules.md`): "should be
  fine", "probably passes", etc.
- Cross-link liberally. A design doc that stands alone is
  usually one that hasn't been read.

### When a Design Doc Graduates

When an idea becomes firm enough to implement, the relevant
sections move to `spec/` (see `spec/README.md`). The original
doc in `docs/` stays — with a forward pointer at the top:

```
> **Status:** superseded by [`spec/<slice>.md`](../spec/<slice>.md).
>  Kept here for historical context.
```

Don't delete design docs. They're the audit trail of how the
spec got to be the spec.

## Convention

- Each sub-document leads with a one-paragraph summary.
- Use English for all technical writing.
- When a doc graduates to a normative contract, move the
  relevant section to `spec/` and leave a pointer here.