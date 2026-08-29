# C7-A — context-pressure baseline evidence

## Status

Accepted prerequisite contract. C7 compression remains gated until a
publication-grade baseline produced by this contract is committed and reviewed.

## Responsibility

Measure the uncompressed C3/C6 context presented to an exact provider token
counter. C7-A does not summarize, mask, rewrite, tune a threshold or mutate a
ledger. It makes the missing compression-admission evidence reproducible.

## Ownership

- `engine/core` continues to own pure C2 derivation and model assembly.
- `engine/eval` owns provider-neutral measurement values and pure reduction.
- `experiments/context-pressure-rs` owns corpus/config I/O and the injected
  token-counter process boundary. It is evidence tooling, not Runtime.
- Runtime and protocol/provider crates gain no tokenizer or benchmark I/O.

## Versioned corpus

One strict UTF-8 JSON document contains:

```text
ContextPressureCorpusV1 {
  contract = "garive.context-pressure-corpus"
  version = 1
  corpus_id, corpus_revision
  cases[]
}

Case {
  case_id, workload_class
  request: ContextRequest
  ordered ContextCandidate values
  model_input_limit_tokens
}
```

V1 workload classes are `conversation`, `tool_heavy`, `capability_heavy`, and
`long_running`. Each class must be non-empty. Identities are unique and bounded;
candidates obey C2 ordering/session/window rules. The request budgets must
retain every eligible candidate: any C2 dropped reference makes the run invalid
because this is the uncompressed baseline.

The corpus is public reference evidence, not production telemetry. A result may
be called representative only for the exact named workload classes and corpus
revision; it is never generalized to user traffic.

## Exact token-counter port

The runner receives an explicit immutable command descriptor:

```text
TokenCounterCommand {
  counter_id, counter_revision
  executable, argv[], cwd, complete environment
  timeout_ms, max_stdout_bytes, max_stderr_bytes
}
```

No value comes from process environment discovery. The child environment is
cleared and rebuilt from the descriptor. One case sends strict JSON containing
the exact ordered provider-neutral `ModelInputItem` array and expects exactly:

```json
{"schema_version":1,"input_tokens":1234}
```

Zero, unknown/duplicate members, excess output, timeout, non-zero exit and
invalid UTF-8 are infrastructure failures. A built-in heuristic counter may be
used only in tests and must make the run non-publishable.

## Pure evidence and reduction

Each successful case records:

```text
ContextPressureCaseEvidence {
  case_id, workload_class
  item_count, utf8_bytes, input_tokens, model_input_limit_tokens
  pressure_basis_points
}
```

`pressure_basis_points = ceil(input_tokens * 10_000 /
model_input_limit_tokens)` using checked integer arithmetic. Values above 10,000
are valid evidence of overflow pressure. Reduction preserves source order and
reports exact case count plus per-class maximum and rational mean pressure; no
floating point is used.

## Publication provenance

A publication-grade baseline binds:

- clean Garive revision and runner revision;
- canonical corpus SHA-256 and explicit corpus identity/revision;
- token counter identity/revision and command-configuration SHA-256;
- ordered case evidence and exact aggregate reduction;
- `publishable=true`, which is forbidden for a heuristic/fake counter.

The output is created without overwrite. Secrets, executable environment
values, raw context content and subprocess stderr never enter evidence.

## Acceptance

- pure reduction boundary/property tests cover zero/overflow/rounding and
  checked arithmetic;
- strict corpus parsing covers unknown/duplicate fields, all workload classes,
  invalid C2 streams and dropped uncompressed candidates;
- process tests cover cleared environment, exact request/response, timeout,
  bounds, exit and malformed JSON;
- an end-to-end non-publishable fixture run is deterministic;
- a publication-grade committed run using an admitted provider counter is the
  separate evidence that may unlock the C7 behavior Spec.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
