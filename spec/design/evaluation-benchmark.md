# E0/B0 — Evaluation evidence and SWE benchmark driver

## Status

Accepted implementation contract. E0 defines neutral, deterministic evaluation
evidence. B0 is the thin official SWE-bench driver consuming E0 without
implementing Agent behavior, repository environments or benchmark grading.

## Reviewed upstream

- Repository: `SWE-bench/SWE-bench`
- Revision: `7a21e05772954cc81471ae19d56f436cecf43c54`
- Dataset contract: `docs/guides/datasets.md`
- Evaluation contract: `docs/guides/evaluation.md` and
  `swebench/harness/run_evaluation.py`

Upstream changes do not silently alter a published run. A new revision requires
an explicit harness-profile revision and evidence update.

## E0 ownership

`engine/eval` owns pure values and reductions only:

```text
EvaluationCaseId, EvaluationSuiteId, EvaluationRunId
EvaluationCaseOutcome = Passed | Failed | InfrastructureFailure | NotAttempted
EvaluationCaseResult { case_id, outcome, duration_ms, input_tokens?, output_tokens? }
EvaluationSummary { attempted, passed, failed, infrastructure_failed, score }
EvaluationBaseline { suite, dataset_revision, harness_revision, agent_revision,
                     config_digest, summary }
```

Identities are non-empty and bounded to 256 UTF-8 bytes. Counts use checked
arithmetic. Duration is finite and bounded by the run contract. Token usage is
unknown when evidence is absent, never zero by default. Score is exactly
`passed / (passed + failed)`; infrastructure failures and not-attempted cases
are reported separately and never counted as Agent failures or successes. A
zero attempted denominator has no score.

E0 performs no filesystem, process, network, Docker, dataset or model I/O. It
does not define what passing means. Summary reduction is order-independent,
rejects duplicate case IDs and consumes only terminal outcomes.

## Official B0 case input

B0 V1 admits only explicit UTF-8 JSONL exported from the official public
SWE-bench Lite or Verified `test` split. Every record has exactly:

```text
instance_id, repo, base_commit, problem_statement, version,
FAIL_TO_PASS[], PASS_TO_PASS[]
```

Gold `patch`, `test_patch`, hints and evaluation output are forbidden from
Agent intake. Unknown/duplicate members, duplicate instance IDs, empty values,
non-hex commits, invalid `owner/repo`, oversized lines/documents and duplicate
test IDs fail before environment setup. Case order is the source order.

The smoke fixture is the public Lite instance `astropy__astropy-12907` at base
commit `d16bfe05a744909de4b27f5875fe0d4ed41ce607`. It proves schema and driver
behavior only; its result is not a published score.

## Four mandatory ports

Every case crosses one and only one route:

```text
CaseSource.load
  -> EnvironmentPool.acquire(case)
  -> IntakeAdapter.translate(case, workspace)
  -> AgentDriver.run(input, workspace)
  -> PatchAdapter.translate(output, case)
  -> OfficialEvaluator.evaluate(prediction, isolated environment)
  -> ResultSink.append(case result)
  -> EnvironmentPool.release(workspace)
```

Setup, intake, Agent, patch and evaluator failures are typed infrastructure
failures. Release is attempted exactly once after every successful acquire.
The driver never interprets test output or changes an evaluator verdict.

## Concurrency and isolation

`jobs` is non-zero and bounded to 64. At most `jobs` cases may be acquired or
driven concurrently. Admission is FIFO by source position; durable/result
output is emitted in source order so repeated runs are diffable. Each acquired
workspace is bound to one case and exact base commit. Evaluator isolation is a
separate workspace/container; Agent state cannot enter it.

Official/published mode requires an injected warm environment pool with at
least `jobs` capacity and the official Docker evaluator. Sequential published
runs (`jobs = 1`) are rejected. Smoke/development mode may use one job but is
always labeled non-publishable.

## Adapters and predictions

V1 intake contains only the exact problem statement, repository identity,
base commit and workspace handle. Patch output is a non-empty canonical
unified diff beginning with `diff --git`; absolute paths, parent traversal,
binary patches and changes outside the acquired repository fail closed.

The official prediction writer emits exact JSONL:

```json
{"instance_id":"...","model_name_or_path":"...","model_patch":"diff --git ..."}
```

No additional member is sent to the evaluator.

## Official evaluator descriptor

B0 constructs, but does not reinterpret, this pinned invocation:

```text
python -m swebench.harness.run_evaluation
  --dataset_name SWE-bench/SWE-bench_Lite|SWE-bench/SWE-bench_Verified
  --split test
  --predictions_path <explicit file>
  --instance_ids <ordered IDs...>
  --max_workers <jobs>
  --run_id <non-empty ID>
  --cache_level env
  --clean true
```

Executable, dataset path, predictions path, run directory, timeouts and jobs
are constructor inputs. B0 reads no environment. The subprocess receives an
explicit cleared environment plus explicitly admitted Docker/Python values
from the runner. Non-zero exit, missing report, schema mismatch and incomplete
instance coverage are infrastructure failures.

## Tracking

Run events and version summaries use schema version 1 and E0 outcomes. Files
are append-only, secret-free and bounded. A published baseline additionally
records Garive git revision, dirty flag (must be false), dataset identity and
revision, upstream harness revision, configuration digest, adapter identities,
environment kind, jobs and exact case count. A smoke result cannot be promoted
by renaming a file.

## Verification

- E0 unit/property tests cover duplicate rejection, checked counts, unknown
  usage, ordering independence and denominator rules;
- official public smoke fixture validates loader/intake without gold leakage;
- fake four-port matrix covers every failure boundary, release-once behavior,
  bounded concurrency and deterministic result order;
- prediction JSONL and evaluator argv round-trip exact official shapes;
- an injected fake official process proves exit/report/coverage failures;
- source scans enforce no Engine I/O and no benchmark environment lookup;
- real official smoke execution is separate external evidence requiring Docker,
  the pinned harness and its image; absence keeps published-score evidence gated.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
