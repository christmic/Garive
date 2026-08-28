# docs/

> **思考和设计.** Where ideas live before they become concrete
> specs. Exploratory notes, design write-ups, ADRs, runbooks,
> tutorials — the human-facing, **non-normative** side of the
> project.

This is the **deliberative** layer. Once an idea hardens into a
contract that must be implemented faithfully, it moves to `spec/`.
Until then, it lives here — even if it is rough, partial, or
wrong.

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

## Convention

- Each sub-document leads with a one-paragraph summary, then
  sections for context, design, open questions, and known
  limitations.
- Use English for all technical writing.
- When a doc graduates to a normative contract, move the relevant
  section to `spec/` and leave a pointer here.