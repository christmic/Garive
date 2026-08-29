# bench/scripts/

> **Helpers for `bench/`** — case fetch, eval bootstrap, score
> reports. Shell / Python; not Rust.

| Script | Purpose |
|--------|---------|
| `fetch-cases.sh` | Clone or download the official swe-bench dataset into `bench/cases/`. Args: `--set verified|lite`. |
| `bootstrap-eval.sh` | Create the Python venv + pin swe-bench tag for `bench/eval/`. |
| `report.sh` | Print a Markdown score table from `bench/tracking/versions/`. |

These are **operational scripts**, not part of the bench
crate. They run before / after the bench runner, not inside
it.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: optional helpers are not part of admitted B0; the executable CLI is `bench run`.
