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

Installing Tool, Memory, Knowledge, Skill, Scheduler or delegation modules
neither grants authority nor advertises them to a model. Each capability enters
through its accepted Runtime port; tool-capable composition additionally
requires the complete F0 governance bundle.

## Explicit configuration

```text
LocalReplicaConfig {
  database_path, loopback_address, installed_agent
  model_target_id, deployment_id, recovery_policy_revision
  request/output/context limits
  worker_owner_id, execution_lease_duration_ms, dispatch_queue_capacity
  optional constructed topology-only Plan proposal port
  optional constructed Plan admission policy
}
```

Every text value is non-empty, every count/duration is non-zero and the address
is loopback. The model port, Host clock and OS monotonic source are injected
constructed values. Runtime maps boot-scoped OS readings into one
SQLite-persisted logical clock revision. Every observation durably reserves its
complete lease horizon; the first reading from a new boot starts beyond that
horizon, so old claims expire through normal PL1 semantics. Audit RFC 3339 time
is never reused as a lease tick. Provider endpoint, credential, headers and protocol
configuration enter only through the constructed model port. R1 reads no
process environment and performs no implicit config-file or credential-store
lookup. Debug/error output contains stable codes and non-secret identities only.
An absent Plan admission policy denies automatic proposal adoption.
An absent Plan proposal port leaves a Goal at the explicit planning boundary.
An implementation that invokes a model for proposal content must use the
durable C6/model prepared-started-terminal lifecycle. Direct ModelPort
invocation from a proposal adapter would create an unrecoverable dispatch cut
and is forbidden.

## Commit and dispatch ordering

`LiveHost.start_turn` first commits `turn.started`, `turn.input` and
`execution.started`. Only its `CommittedTurn` may enter the bounded worker
queue. Rejection cannot roll back the transaction and is a recoverable dispatch
failure. No model preflight or network attempt occurs in the Host request task
before commit.

The worker acquires the C6 execution lease before Core starts. One Turn has at
most one active worker. Duplicate delivery is harmless: a terminal Execution
is ignored and a lease conflict is left for bounded recovery rather than run
concurrently. A replacement Execution may fence a non-expired lease only after
the same recovery transaction has durably terminalized the old Execution as
`execution.abandoned`; a second owner of the same active Execution cannot use
this rule.

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
state. A recovery replacement for an original Start is reconstructed as
`AgentEntry::Continue(ResourceReady)` with the persisted completed-iteration
count and last safe position; it is never converted back into a zero-cursor
Start.

The context port derives only from admitted durable fact projections. It
includes the trusted `turn.input` as required user input. The durable execution
wrapper additionally projects every canonical `effect.observation` belonging
to the same Turn through the current committed position as a required neutral
`ToolObservation`, ordered by Ledger position and correlated by
`model_call_id`. It verifies the observation content binding before model
admission. This projection is refreshed between Agent iterations and after
restart. Telemetry and public Host events are never Agent truth.

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
classifier before dispatch. Product execution admission remains closed while
this bounded scan or any returned replacement dispatch is pending. It never
blindly repeats an uncertain model attempt. Even when only the start
transaction exists, restart cannot prove that the lost process did no work: C6
atomically abandons that Execution and creates a replacement before it is
queued. A Prepared-v3 pre-dispatch interruption remains non-terminal, resumes
the same invocation through the configured F0 brokers, commits its observation,
then creates and executes the continuation. Startup limits both inspected Turns
and reconstructed argument bytes and reports unfinished work instead of opening
admission. Shutdown stops admission, performs an explicitly bounded drain and
leaves unfinished durable work discoverable.

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
- Desktop restart tests proving new work is rejected until recovery completes,
  one Prepared-v3 invocation resumes without duplicate governance/dispatch
  facts, and its durable observation reaches the replacement model request;
- one loopback H1 + H1-T protocol fixture flow with no external network;
- strict Rust gates. Kotlin has no R1 Runtime parity claim.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-09-01
- Status: accepted
