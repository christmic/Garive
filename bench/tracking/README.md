# bench/tracking/

> **Per-version + per-case score history.** Append-only JSONL;
> one file per run, plus a manifest entry that points at the
> latest. Designed to be diff-able between versions.

## Schema

### Run-level (`runs/<run_id>.jsonl`)

```
{"kind":"run-start","run_id":"...","date":"...","env":"official|self-cow","agent":"...","adapter":"...","set":"verified|lite"}
{"kind":"case-start","instance_id":"...","case_index":N,"case_total":M}
{"kind":"case-result","instance_id":"...","passed":true|false,"runtime_ms":...,"tokens_in":...,"tokens_out":...,"fail_to_pass_passed":N,"pass_to_pass_passed":N}
{"kind":"run-end","run_id":"...","score":F,"cases_passed":N,"cases_total":M,"duration_ms":...}
```

### Version-level (`versions/<vX.Y.Z>.json`)

```
{
  "version": "v0.4.1",
  "date": "2026-08-27",
  "env": "official",
  "set": "verified",
  "agent": "garive",
  "adapter": "garive-bridge",
  "score": 0.094,
  "cases_passed": 47,
  "cases_total": 500,
  "duration_ms": 12345678
}
```

## Querying

```bash
# Latest published score on official Verified
jq 'select(.env == "official" and .set == "verified") | max_by(.date) | {version, score}' versions/*.json

# Score delta vs previous version
diff <(jq -S . versions/v0.4.0.json) <(jq -S . versions/v0.4.1.json)
```

## What Lives Here

- `runs/` — raw JSONL event stream per run.
- `versions/` — one summary JSON per released version.
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
- Status: stub — slice not yet landed; content is scaffolding.
