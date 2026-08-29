# bench/cases/

> **Official Agent SWE datasets.** Submodule or downloaded
> JSON. Read-only at runtime; `bench/` loads from here.
>
> **All cases come from publicly available, official Agent
> SWE benchmarks.** We do not invent cases. See `bench/AGENTS.md`
> Rule 1 for the full policy.

## Recognised Sources

| Dataset | Source | Size | Use |
|---------|--------|------|-----|
| SWE-bench Verified | [princeton-nlp/SWE-bench](https://github.com/princeton-nlp/SWE-bench) `verified/` | 500 cases | Headline comparison vs published numbers |
| SWE-bench Lite | same repo `lite/` | 300 cases | Faster, still official |
| SWE-bench Multimodal | same repo `multimodal/` | varies | Cases with visual inputs |
| SWE-bench Multilingual | same repo `multilingual/` | varies | Non-English issues |
| Terminal-Bench | [laude-institute/terminal-bench](https://github.com/laude-institute/terminal-bench) | varies | Terminal / shell tasks; complements swe-bench's repo-edit style |

`bench/cases/` is populated either by:

- `git submodule add https://github.com/princeton-nlp/SWE-bench.git cases/swe-bench-official`
- `git submodule add https://github.com/laude-institute/terminal-bench.git cases/terminal-bench`
- `scripts/fetch-cases.sh --source swe-bench-verified --out cases/`

When a new public benchmark is needed, add it here **and**
update `bench/AGENTS.md` Rule 1's source table.

Cases are immutable inputs. `bench/` reads them; `bench/eval/`
delegates to the official Python harness against the same
dataset.

## Schema

The admitted JSONL intake carries exactly:

```
instance_id          "django__django-11099"
repo                 "django/django"
base_commit          "abc1234"
problem_statement    the issue text the agent reads
version              official repository version
FAIL_TO_PASS          tests that must pass after the patch
PASS_TO_PASS          tests that must still pass after the patch
```

Gold `patch`, `test_patch`, hints and all unknown members are forbidden from
Agent intake. B0 V1 supports only Lite and Verified `test` exports.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: strict V1 loader implemented with one official Lite smoke record.
