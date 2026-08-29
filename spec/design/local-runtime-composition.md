# R1 — Local Runtime composition and execution worker

## Status

Accepted implementation contract. R1 composes the accepted D0, C6, H1 and
H1-T slices into the first runnable local Agent without changing portable
Engine, protocol-adapter, Provider or Host API semantics.

## Ownership and scope

R1 is Runtime code under `runtime/replica`. It owns explicit product
configuration, post-commit dispatch, fixed-prefix request reconstruction, one
model-only execution worker and restart discovery. The standalone local Host
and Tauri backend may embed the same composition. Other Apps remain Host API
consumers and never construct Provider secrets.

The first slice is model-only. Merely installing Tool, Memory, Knowledge,
Skill, Scheduler or delegation modules neither grants authority nor advertises
them to a model; each requires its accepted Runtime port in a later composition
increment.

## Explicit configuration

```text
LocalReplicaConfig {
  database_path, loopback_address, installed_agent
  model_target_id, deployment_id, recovery_policy_revision
  request/output/context limits
  worker_owner_id, execution_lease_duration_ms, dispatch_queue_capacity
}
```

Every text value is non-empty, every count/duration is non-zero and the address
is loopback. The model port, Host clock and monotonic lease clock are injected
constructed values. Provider endpoint, credential, headers and protocol
configuration enter only through the constructed model port. R1 reads no
process environment and performs no implicit config-file or credential-store
lookup. Debug/error output contains stable codes and non-secret identities only.

## Commit and dispatch ordering

`LiveHost.start_turn` first commits `turn.started`, `turn.input` and
`execution.started`. Only its `CommittedTurn` may enter the bounded worker
queue. Rejection cannot roll back the transaction and is a recoverable dispatch
failure. No model preflight or network attempt occurs in the Host request task
before commit.

The worker acquires the C6 execution lease before Core starts. One Turn has at
most one active worker. Duplicate delivery is harmless: a terminal Execution
is ignored and a lease conflict is left for bounded recovery rather than run
concurrently.

## Fixed-prefix reconstruction

The worker opens SQLite independently and loads the verified Turn and Session
prefix through `committed_position`. It requires exact matching:

- one owning `session.opened` binding for Agent instance, definition, revision
  and snapshot digest;
- one latest `turn.started` for the supplied Turn/Execution;
- its exact `turn.input` content binding;
- one matching `execution.started` containing effective limits and prefix;
- no terminal fact for that Execution.

The input content digest is rechecked. HTTP/caller values cannot replace
persisted identities, limits or content. Start creates `AgentEntry::Start`
with cursor `(0, 0)`; continuation uses the existing C6 typed continuation
state and is a separate implementation increment.

The context port derives only from admitted durable fact projections. V1
includes trusted `turn.input` as required user input and never scans past the
frozen prefix. Telemetry and public Host events are never Agent truth.

## Worker ports and terminal publication

R1 freezes one `AgentTurnRequest`, `DurableExecutionConfig` and model target,
then invokes `execute_durable_model_only`. H1-T still owns exactly one HTTP
attempt after `model.started`. Runtime supplies a logical clock, cooperative
cancellation view, semantic-event sink and terminal publisher. The C6 terminal
commit is authoritative; publication failure cannot reopen the Turn.

H1 exposes `turn.completed`, `turn.suspended`, `turn.stopped` or `turn.failed`
only from committed facts. R1 never invents replayable token deltas.

## Restart and shutdown

Startup discovers Open Turns from SQLite and applies the accepted C6 recovery
classifier before dispatch. It never blindly repeats an uncertain model
attempt. A Turn containing only its start transaction may reuse its exact
Execution. Shutdown stops admission, performs an explicitly bounded drain and
leaves unfinished durable work discoverable on restart.

## Stable local failures

`invalid_composition`, `dispatch_queue_full`, `reconstruction_failed`,
`execution_already_leased`, `durability_unavailable`, and `worker_stopped` are
Runtime operational classes. They contain no raw model, SQLite, endpoint or
credential text and never become an Agent outcome outside accepted C6 mapping.

## Acceptance evidence

- explicit configuration validation and source scans for environment/file
  loading in the composition module;
- real SQLite commit-before-queue and commit-before-model tests;
- fixed-prefix identity/content/limit mismatch tests;
- fake model completion producing a durable terminal and H1 event after gaps;
- bounded queue, duplicate dispatch, lease conflict and shutdown tests;
- separate-process restart tests before dispatch, after model start and after
  terminal commit;
- one loopback H1 + H1-T protocol fixture flow with no external network;
- strict Rust gates. Kotlin has no R1 Runtime parity claim.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: accepted
