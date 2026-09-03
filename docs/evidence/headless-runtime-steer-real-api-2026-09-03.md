# Headless Runtime in-flight steer — real-provider evidence

Date: 2026-09-03

Runtime: freshly compiled `garive-headless` on loopback

Provider profile: `anthropic.messages.v1`

Model target: configured `deepseek-v4-flash` deployment

## Claim boundary

This run used H1 HTTP, the real Runtime worker, SQLite ledger, the configured
model service, and the compiled post-fix binary. It was not a model double.
The API caller created one Session, submitted two Turns, and steered the second
Turn only after its first `model.started` fact existed and before any terminal
fact existed. No credential or provider endpoint is recorded here.

## Accepted flow

Session:

```text
session-efe4640b4869d770327a25233907099c7f5b761b5281e875c3daf6ef9eab8774
```

The first Turn completed normally:

```text
turn-c43da9fe3911a9bf7855b3738489024027f16ff218af9bd151b2b183b191c81f
FIRST_FINAL_OK
```

The same Session then admitted a second Turn:

```text
turn-496bce0231932beba781bc46d54e93743ccdb08987c50cb0a8a3b8bf0e351620
execution-793b9b11d489caa2201b9a702716a9b6264ac21363cef45da4378061f027c3f6
```

Before the steer request, an independent ledger query observed one
`model.started` and zero `turn.completed`, `turn.failed`, or `turn.suspended`
facts. `POST /v1/sessions/{session}/turns/{turn}/steer` returned HTTP 200 and
committed the exact override as `turn.steered` at Session position 17.

The first model call then durably completed with the requested draft. Runtime
started iteration 2 and sent the original input, prior assistant output, opaque
Messages thinking continuation state, and `[steered]` user input in order. The
real model's second completion explicitly followed the override and returned:

```text
STEER_FINAL_OK
```

The same Turn then committed `turn.completed`. There was no
`model.uncertain`, `turn.failed`, or suspension in the accepted run.

## Defect exposed and fixed

The first freshly compiled attempt proved the H1 ordering but failed on the
second model request. The Messages-compatible service rejected continuation
because the prior extended-thinking block had not been passed back. Runtime had
retained only assistant text while the provider normalizer reduced hidden
thinking to its signature.

The provider-compatible layer now encodes the complete official
`thinking + signature` block, or exact redacted-thinking data, into a versioned
opaque reference. Only the Messages mapper decodes that envelope back into the
official request block. Runtime persists and replays the reference in the
original model-output order without interpreting or displaying its content.
Malformed, incomplete, unknown-version, or cross-protocol references fail
closed before provider dispatch.

The accepted rerun proves the fix against the real service: both
`model.completed` facts contain versioned opaque continuation records, the
second response is `STEER_FINAL_OK`, and the Turn completes.

## Automated gates

- compatible-provider request tests normalize an official Messages thinking
  response and reconstruct one ordered assistant turn containing thinking then
  text;
- the Runtime H1 test blocks an in-flight model call, commits steer over HTTP,
  proves opaque reasoning plus assistant text reach the next request, and
  completes the second Turn;
- provider-compatible and Runtime tests, strict Clippy, formatting, and strict
  rustdoc remain required before publication.
