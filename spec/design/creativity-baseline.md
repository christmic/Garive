# CR-A — bounded creativity baseline experiment

## Status

Accepted prerequisite implementation contract. CR-A collects paired evidence;
it does not admit `engine/creativity` behavior, a production prompt strategy or
numeric regression thresholds.

## Question and bounded hypothesis

For tasks with more than one defensible answer, compare the same generator and
budget under two explicit arms:

- `control`: produce and select exactly one candidate;
- `bounded_alternatives`: produce two through the task's declared maximum, then
  select exactly one candidate.

The hypothesis is only that bounded alternatives may increase the number of
distinct correct solution clusters without reducing selected-answer
correctness. CR-A reports both axes independently and does not combine them
into one score or choose an acceptable trade-off before measured runs exist.

## Neutral task taxonomy

The strict corpus contains at least one task from every v1 class:

1. `design_alternatives` — compare materially different valid designs;
2. `diagnostic_hypotheses` — form distinct testable explanations;
3. `constraint_reconciliation` — satisfy competing requirements by different
   defensible plans;
4. `transformation_reframing` — preserve required meaning through distinct
   representations or approaches.

These are evaluation labels, not Agent roles or model prompt instructions. A
corpus is representative only for its exact ID, revision, tasks and generator /
evaluator coordinates.

## Strict corpus and gold separation

One bounded UTF-8 JSON document contains:

```text
CreativityCorpusV1 {
  contract = "garive.creativity-corpus"
  version = 1
  corpus_id, corpus_revision
  tasks[]
}

Task {
  task_id, class
  generator_prompt
  evaluator_rubric_json
  max_candidates
  max_candidate_utf8_bytes
  max_total_candidate_utf8_bytes
}
```

IDs are unique/non-empty/bounded. Rubric text is a strict JSON value retained
only for evaluator intake; it never enters generator requests, diagnostics or
published evidence. Unknown/duplicate fields, missing classes, invalid JSON,
zero bounds and contradictory aggregate bounds fail before a port is called.

The checked-in reference corpus is synthetic and public. A fixture run proves
the pipeline only and is permanently non-publishable.

## Sole paired route

Every task executes both arms through exactly:

```text
validated task
  -> GeneratorPort.generate(arm, prompt, seed, exact bounds)
  -> validate candidates + selected candidate
  -> EvaluatorPort.evaluate(rubric, candidates)       # arm-blind
  -> validate complete candidate verdict coverage
  -> pure paired reduction
  -> non-overwriting evidence sink
```

The evaluator request deliberately excludes arm identity, generator revision,
selection rationale and which candidate was selected. It returns exactly one
verdict per candidate:

```text
CandidateVerdict {
  candidate_id
  correct
  correct_cluster_id?   # required iff correct
}
```

Cluster identity is evaluator-owned semantic evidence. A wrong candidate has
no cluster and never contributes diversity. The runner does not infer
correctness or semantic similarity from candidate text.

## Pure evidence and reduction

`engine/eval` owns provider/model-neutral values and checked reduction:

```text
CreativityArmEvidence {
  task_id, arm
  candidate_count
  correct_candidate_count
  distinct_correct_cluster_count
  selected_correct
}

CreativityTaskPair {
  task_id, class
  control, bounded_alternatives
}

CreativitySummary {
  ordered_pairs
  per-arm exact totals
  per-class exact totals
}
```

Reduction rejects duplicate/missing arms, task/class mismatch, incomplete
source coverage, impossible counts and arithmetic overflow. Source order is
preserved. It reports exact integer totals and rational means only; no floating
point, weighted composite score, significance claim or threshold exists in v1.

## Authority and budgets

CR-A grants no Agent authority. Generator output is inert text and candidate
identity only. It cannot expose tools, execute effects, read Memory/Knowledge,
delegate, persist facts or publish a final user answer. The selected candidate
is evidence, not an Agent outcome; a future accepted production slice must keep
final authority with the primary Agent/Turn policy.

Per task and arm, configuration freezes:

- exactly one generator invocation and one evaluator invocation;
- deterministic non-secret seed;
- candidate count and per/aggregate UTF-8 bounds from the corpus;
- non-zero process timeout, stdout and stderr bounds;
- exact generator/evaluator implementation and configuration revisions.

Infrastructure failures remain distinct from incorrect/low-diversity outcomes.
There is no retry, fallback arm, hidden extra model call or best-of rerun.

## Command ports and evidence maturity

The first executable uses strict command ports: explicit executable, argv,
working directory, complete post-clear environment and resource bounds. One
strict JSON request is written to stdin and one strict JSON response is read
from stdout. Shell insertion, inherited environment, unbounded output and
secret-bearing evidence are forbidden.

Fixture commands are always non-publishable. A later CR-B publication runner
must add clean-revision/tool attestation and bind exact external generator and
evaluator coordinates before any run can satisfy the missing Creativity
admission evidence.

## Acceptance

- a shared/reference corpus covers all four classes and strict mutation cases;
- pure reduction tests cover both arms, correct-only clusters, selected
  correctness, duplicates, missing coverage and overflow;
- fake port tests cover exact blind evaluator intake, bounds, malformed output,
  timeout, exit and no retry;
- an end-to-end fixture run writes deterministic non-publication evidence
  without rubric or candidate content;
- source scans keep Engine evaluation pure and creativity behavior empty;
- focused and full Rust gates pass.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
