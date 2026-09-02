# Runtime API Turn and in-flight steer contract

Status: normative implementation contract.

## Scope

This contract covers one Runtime process driven through H1 HTTP:

1. create a Session;
2. submit a Turn and observe its durable completion;
3. submit a later Turn in the same Session;
4. while that Turn's model request is in flight, append `turn.steered`;
5. execute another derive/model iteration in the same Turn and commit the
   response that includes the steered input.

The model port used by conformance tests is controlled and provider-neutral.
Provider credentials and environment variables are outside this contract.

## Durable ordering

`turn.steered` is an admitted concurrent control fact, not cancellation and
not an arbitrary Session mutation. Runtime MUST reconcile it when a model
lifecycle commit loses the Session-version compare-and-swap race. All other
unexpected concurrent fact kinds fail closed.

For every derive pass, Runtime records the inclusive ledger watermark consumed
by the context port. A successful terminal transaction is allowed only when no
`turn.steered` position exists above that consumed watermark. Therefore:

- if terminal wins the ledger transaction, a later steer observes the closed
  Turn and is rejected;
- if steer wins, Runtime MUST NOT commit the stale terminal proposal;
- an acknowledged steer MUST either reach a later model request or remain
  durably recoverable under the still-open Turn.

## Context projection

At each derive boundary Runtime projects, in ledger order:

- the original trusted input;
- prior model text/refusal output as assistant messages;
- each `turn.steered` payload as a user message;
- existing governed effect observations.

The projection is bounded by `ContextRequest.through_position`; facts beyond
that watermark cannot leak into the request. A steer advances the watermark
only at the next derive boundary. Core consumes another iteration from the
frozen Turn limit, so steering cannot create an unbounded loop.

## Observable completion

The final `execution.completed + turn.completed` transaction binds the last
model response and cumulative usage for every model attempt in the Execution.
Clients determine completion from the durable Turn/event query, never from the
lossy worker queue or an in-memory notification.

## Conformance proof

The Runtime integration test MUST use the real loopback HTTP server, SQLite
ledger and local worker. Its model double MUST block the selected invocation,
signal that the invocation has started, accept steer through HTTP while still
blocked, and then prove a later invocation contains the steer before the Turn
becomes Completed. The test also proves a completed first Turn does not prevent
a second Turn in the same Session.
