# O0 — Agent observability semantics

## Status

Draft implementation contract in the Agent capability review set.

## Scope and truth boundary

O0 defines neutral semantic signals, measurements, redaction classes and sink
behavior. Engine Observability owns portable values and validation. Runtime
owns correlation, redaction, sampling, buffering, backpressure and exporters.

Observability is never durable Agent truth. C6 facts remain authoritative for
recovery and H1 events remain the public product stream. Losing, duplicating or
reordering telemetry cannot change a Turn, authority decision or schedule.

## Signal envelope

```text
AgentSignal {
  signal_name
  schema_version
  observed_at_utc
  severity: Trace | Debug | Info | Warn | Error
  correlation: Correlation
  attributes: ordered unique Attribute[]
  measurements: ordered unique Measurement[]
  redaction_class: Public | Operational | Restricted
}

Correlation {
  trace_id?, span_id?, parent_span_id?
  session_id?, turn_id?, execution_id?
  model_request_id?, tool_invocation_id?
  durable_position?
}
```

Trace/span IDs are observability identities and cannot substitute for domain
or command IDs. Runtime may omit domain correlation under privacy policy.
Timestamps are diagnostic and never ordering/recovery evidence.

Attributes are bounded string/bool/integer values. Each signal schema declares
an allowlist, value length, maximum count and redaction class. Arbitrary maps,
raw exception text and provider payloads are forbidden.

Measurements use checked integer values and explicit units:
`Count`, `Bytes`, `Milliseconds`, `Tokens`, or `BasisPoints`. Unknown token
evidence is `Unknown`, never zero. Histograms and floating values are exporter
projections outside the portable contract.

## Stable signal catalogue

V1 admits these low-cardinality names:

- `agent.execution.started`, `agent.execution.terminal`;
- `agent.iteration.started`;
- `agent.context.derived`;
- `agent.model.attempt`, `agent.model.terminal`;
- `agent.effect.prepared`, `agent.effect.terminal`;
- `agent.interaction.required`;
- `agent.recovery.classified`;
- `agent.host.command`, `agent.host.event_page`;
- `agent.scheduler.claim`, `agent.scheduler.dispatch`;
- `agent.delegation.requested`, `agent.delegation.terminal`;
- `agent.telemetry.dropped`.

Outcome/reason attributes use accepted Engine/C6 stable enums. Provider model,
endpoint, HTTP status, raw error and credential values do not enter portable
signals; Runtime may expose separately governed transport diagnostics.

Metric labels must be low cardinality: signal/outcome/reason/capability class,
protocol family and success boolean. Session, Turn, Execution, request,
invocation, actor, source and schedule IDs are forbidden as metric labels.
They may appear only in trace/log correlation after redaction policy.

## Production and durability ordering

Core may propose semantic signals but cannot send them to an exporter. Runtime
emits durability-sensitive signals only after the corresponding commit and
includes its durable position. Pre-commit attempt signals must state
`durable_position` absent and cannot use a terminal name.

Signals derived during restart use the original durable fact position and a
`replayed=true` allowed attribute. Duplicate export is permitted. Consumers
must not infer a missing fact from a missing signal.

## Redaction

Portable signals never contain:

- prompt, response, Memory, Knowledge or tool content;
- content bindings/references that disclose storage paths;
- credentials, headers, connection strings or environment values;
- raw provider/executor/database errors;
- authority documents, interaction answers or user identifiers.

Allowed content metadata is digest-presence boolean, bounded byte/token count,
item count and stable safe classification. `Restricted` signals require an
explicit Runtime sink policy; changing a sink cannot relax source validation.

## Sink and backpressure contract

```text
ObservabilitySink::emit(batch) -> Accepted | Backpressured | Unavailable
```

Runtime owns a bounded queue with explicit maximum signals/bytes and flush
deadline. Agent execution never waits without bound for telemetry. On pressure,
Runtime applies a frozen priority policy, increments a local dropped counter
and later emits `agent.telemetry.dropped` when possible. Durable/audit facts are
never placed in this queue.

Exporter retry has an independent bounded budget and cannot reuse Agent model
or tool retry policy. Shutdown performs a bounded flush; timeout drops
telemetry and does not delay a committed terminal indefinitely.

## Stable failures

`invalid_signal`, `unknown_signal`, `attribute_not_allowed`,
`attribute_limit_exceeded`, `measurement_invalid`, `redaction_violation`,
`sink_backpressured`, `sink_unavailable`, and `serialization_failure`.

Only the first six are portable validation results. Sink/serialization
failures are Runtime operational outcomes and never become `AgentOutcome`.

## Acceptance evidence

- shared Rust/Kotlin catalogue, validation, units and redaction fixtures;
- property tests that forbidden/high-cardinality attributes never validate;
- Runtime fake sink tests for commit ordering, replay duplication and sampling;
- bounded queue/backpressure/drop/shutdown tests with no Agent-state changes;
- secret canary tests across Debug/Display/serialized exporter payloads;
- Engine Observability imports no exporter SDK, async runtime or environment.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: draft
