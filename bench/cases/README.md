# bench/cases/

> **Official SWE-bench dataset.** Submodule or downloaded JSON.
> Read-only at runtime; `bench/` loads from here.

Two flavours to load:

| Dataset | Source | Size | Use |
|---------|--------|------|-----|
| SWE-bench Verified | [princeton-nlp/SWE-bench](https://github.com/princeton-nlp/SWE-bench) `verified/` | 500 cases | Headline comparison vs published numbers |
| SWE-bench Lite | same repo `lite/` | 300 cases | Faster, still official |

`bench/cases/` is populated either by:

- `git submodule add https://github.com/princeton-nlp/SWE-bench.git cases/swe-bench-official`
- `scripts/fetch-cases.sh --set verified --out cases/`

Cases are immutable inputs. `bench/` reads them; `bench/eval/`
delegates to the official swe-bench Python harness against the
same dataset.

## Schema

Each case (JSON) carries:

```
instance_id          "django__django-11099"
repo                 "django/django"
base_commit          "abc1234"
patch                gold unified diff (for reference, not the eval target)
test_patch           test-file changes needed to evaluate
problem_statement    the issue text the agent reads
fail_to_pass         tests that must pass after the patch
pass_to_pass         tests that must still pass after the patch
```

This mirrors the official swe-bench JSON schema; `bench/`
loads it verbatim.