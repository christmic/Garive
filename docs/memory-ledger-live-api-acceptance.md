# Memory ledger live-API acceptance

Date: 2026-08-31

This acceptance separates two claims that must not be conflated:

1. A committed Memory retrieval improves model work when Runtime supplies it.
2. The default Desktop worker automatically performs that retrieval.

The first claim is now supported by repeatable real-model evidence. The second
is not: `LocalExecutionWorker` currently invokes the default execution path
without constructing `PreparedAgentCapabilities.memory_retrieval`.

## Controlled experiment

The ignored integration test
`live_memory_ledger_improves_factual_work_and_rejects_stale_revision` uses the
shipping Runtime HTTP transport and durable execution coordinator. It calls
`deepseek-v4-pro` through the local token9 Messages-compatible endpoint only
when `GARIVE_LIVE_API=1` is explicitly set.

All conditions use the same task and output constraints:

- **No memory:** no retrieval capability is supplied.
- **Correct memory:** one active `client-brief` revision is retrieved.
- **Superseded conflict:** a stale `Friday` revision and active `Tuesday`
  revision are scored; deterministic retrieval must admit only the active one.

The task requests four strict JSON fields. Its three values are absent from the
user prompt, so a successful answer must come from admitted Memory evidence.

## Evidence

Two independent live runs produced the same semantic results:

| Condition | Codename | Deploy day | Region | Evidence | Result |
| --- | --- | --- | --- | --- | --- |
| No memory | `null` | `null` | `null` | `[]` | Correct abstention |
| Correct memory | Amber Heron | Tuesday | Qingdao | `client-brief` | Exact |
| Superseded conflict | Amber Heron | Tuesday | Qingdao | `client-brief` | Exact |

The test also verifies:

- `memory.retrieval_recorded` commits before `model.started`;
- the conflict retrieval fact contains neither `Friday` nor `revision-stale`;
- only `revision-active` is selected;
- reopening the SQLite ledger preserves the committed retrieval fact;
- the model output identifies the exact record used.

Provider-reported token usage varied materially between identical runs. It is
recorded for cost observation, but is not treated as the quality verdict.

## Run command

```sh
GARIVE_LIVE_API=1 \
GARIVE_LIVE_API_ENDPOINT=http://127.0.0.1:9527/v1/messages \
GARIVE_LIVE_API_KEY=token9-loopback \
cargo test -p garive-runtime --test durable_core_execution \
  live_memory_ledger_improves_factual_work_and_rejects_stale_revision \
  -- --ignored --nocapture
```

The placeholder loopback credential is intentionally non-secret; token9 owns
upstream credentials. Do not put upstream keys in this test or repository.

## Product conclusion

The commit-before-context Memory design improves factual task completion and
prevents a superseded revision from contaminating the prompt in this controlled
experiment. The remaining product work is automatic recall composition in the
default Desktop worker. Until that is wired, the Desktop app does not receive
these benefits merely because Memory facts exist in its ledger.
