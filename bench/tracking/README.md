# bench/tracking/

> **Per-version + per-case score history.** Append-only JSONL;
> one file per run, plus a manifest entry that points at the
> latest. Designed to be diff-able between versions.

## Schema

### Run-level (`runs/<run_id>.jsonl`)

Every record has `schema_version: 1`. `run-start` freezes run/suite/dataset,
harness and Agent revisions, dirty flag, canonical config digest, both adapter
revisions, environment kind, jobs, exact case count and publishability.
`case-result` records source index, case ID, E0 outcome, duration and nullable
token evidence. `run-end` records E0 counts and an exact nullable score
numerator/denominator; floating-point scores are not stored.

Publication evidence is the validated E0 `EvaluationBaseline`; only a clean
official run with jobs greater than one can construct it. Development JSONL
cannot become publishable by copying or renaming it.

## Querying

```bash
# Read the exact terminal score from one completed run
jq -c 'select(.kind == "run-end") | {score_numerator,score_denominator}' runs/run-1.jsonl
```

## What Lives Here

- caller-selected tracking path — one immutable JSONL event stream per run.
- E0 `EvaluationBaseline` — typed publication evidence returned after finish.
- `workspaces/` — ephemeral per-case workspaces for `self-cow`
  mode (gitignored; safe to delete after the run).

## What Does NOT Live Here

- Patches. Patches are recorded per case but discarded once
  the version summary is written — version history does not
  keep every intermediate patch.
- Eval logs. Raw stdout / stderr from eval goes to
  `runs/<id>/logs/` and is rotated after 30 days.

## Why Append-only

Append-only JSONL survives mid-run crashes, doesn't require
a database, and diffs cleanly across versions. A future
SQLite reader can ingest the same files.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: ordered append-only JSONL and E0 baseline construction implemented.
