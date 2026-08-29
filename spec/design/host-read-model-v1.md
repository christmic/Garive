# H2 — Client-safe Host read model v1

> This Spec adds bounded installed-Agent discovery, durable Session listing,
> and restart-safe conversation timelines to the loopback Host. Views are pure
> redacted projections of verified Ledger prefixes; clients gain no new authority.

## Audience

Runtime Host, Proto, shared client, Desktop, Web, and mobile engineers replacing
hard-coded bootstrap values and process-only conversation state.

## Why

H1 can create a Session, mutate a Turn, and follow public terminal events. It
cannot list installed Agent definitions or reopen a prior conversation, and its
event projection intentionally omits user input and suspension coordinates.
Product clients therefore cannot provide durable navigation, conversation
history, or a usable continuation flow. H2 adds read-only projections without
exposing Engine facts, credentials, provider configuration, or database access.

## Boundary

- Runtime is the sole projection owner and reads one verified fixed Ledger
  prefix per response.
- H2 is additive to Host API v1. Existing H1 routes and protobuf tags retain
  their meaning.
- Every result carries `api_version = "v1"` and a non-zero observed durable
  watermark where a Session exists.
- Queries use no idempotency key and commit no fact. They never start recovery,
  execution, model, Memory, or effect work.
- H2 remains loopback-only under H1. Remote auth and tenant policy belong to a
  future Gateway contract.

## Public views

```text
AgentDefinitionSummaryV1 {
  api_version, definition_id, definition_revision, capabilities[]
}

SessionSummaryV1 {
  api_version, session_id, agent_instance_id,
  definition_id, definition_revision, opened_at,
  latest_position, latest_turn_id?, latest_turn_state?, turn_count
}

TurnTimelineItemV1 {
  turn_id, started_position, latest_position, state,
  user_text, completion_text?, suspension?
}

SuspensionViewV1 {
  suspension_id, session_version, kind, prompt
}
```

`capabilities` is a sorted set of stable public capability names installed for
new Sessions, not a promise that every action is authorized. V1 may return one
definition because R1 installs one immutable Agent; the wire shape is a list so
clients never hard-code that composition limit.

`latest_turn_state` and timeline `state` are exactly `running`, `suspended`,
`completed`, `stopped`, or `failed`. A continuation updates the existing Turn;
it never creates another timeline item. `turn_count` counts verified first-start
facts only.

`user_text` is reconstructed from the exact committed `turn.input` bound to the
first start. `completion_text` uses the same redacted committed response-item
projection as H1 `turn.completed`. `prompt` is the redacted structured public
interaction prompt admitted by C5/C6. No model request, hidden instruction,
context, reasoning, tool arguments/results, raw failure, credential, endpoint,
or internal fact payload is included.

## HTTP queries

| Method and path | Result |
|---|---|
| `GET /v1/agent-definitions` | Bounded ordered `AgentDefinitionSummaryV1` list. |
| `GET /v1/sessions?limit=N&before=TOKEN` | Reverse-opened Session page plus optional next token. |
| `GET /v1/sessions/{session_id}` | One exact `SessionSummaryV1`. |
| `GET /v1/sessions/{session_id}/timeline?after_position=P&limit=N` | Ascending complete Turn items whose latest position is after `P`. |

`limit` is required, non-zero, and no greater than the Runtime construction
limit. Unknown query fields, duplicate fields, malformed UTF-8/percent encoding,
zero positions, and oversized tokens return `invalid_request`.

Timeline pagination never splits one Turn: Runtime scans at most its separate
fact bound, completes the current Turn projection, and returns
`scanned_through_position`. The next request uses that position. A Turn whose
first start is before `after_position` is still returned when it changed after
that position. Gaps are valid.

## Session ordering and page token

Sessions order by `session.opened.recorded_at` descending, then raw UTF-8
`session_id` descending. Runtime validates RFC 3339 timestamps from durable
facts; an invalid value is `corrupt_state`.

`before` is an opaque, authenticated local token containing schema version,
ordered key, and a digest bound to the immutable Host installation identity.
It contains no content or secret. Runtime accepts only tokens it issued for the
same installation. Newer Sessions created between pages do not duplicate or
hide older results; deletion is outside H2.

## Snapshot and integrity rules

- Each Session response freezes `latest_position` before reading and projects
  only through that prefix.
- Session listing validates `session.opened` identity/definition bindings and
  terminal state. One corrupt Session fails the page rather than disappearing.
- Timeline requires exactly one first start and one input per Turn, monotonic
  lifecycle, exact owner identities, and at most one current terminal or
  suspension. Invalid facts return `corrupt_state` with no partial body.
- Unknown future internal fact kinds are ignored only when they cannot alter a
  public lifecycle. Unknown public lifecycle schema fails closed.
- Response bodies have explicit total byte, item-count, text, and prompt bounds.
  Oversized committed display content uses the existing redaction/truncation
  policy with an explicit `content_truncated` flag; silent truncation is forbidden.

## Freshness and client behavior

H2 is a point-in-time read model. After loading a timeline, clients reconnect to
H1 events from `latest_position`. A race is safe: events committed after the
frozen prefix are delivered by SSE; duplicates at or below the cursor are
ignored under A1 rules. EOF never changes a timeline state to terminal.

A client must not synthesize a missing input/output, guess suspension identity,
or treat Session-list order as execution priority. It may cache a response only
under its exact installation, Session, and watermark key.

## Stable failures

| HTTP | Code | Meaning |
|---|---|---|
| 400 | `invalid_request` | Path, bounds, cursor, or query encoding is invalid. |
| 404 | `not_found` | Requested Session is absent. |
| 413 | `read_bound_exceeded` | A verified view cannot fit declared server bounds. |
| 503 | `durability_unavailable` | The durable store cannot complete the snapshot read. |
| 500 | `corrupt_state` | Durable identities, lifecycle, timestamp, or content projection is invalid. |

Errors use `HostErrorV1` and never contain tokens, text, raw facts, SQL, file
paths, or exception strings.

## Acceptance evidence

- Proto adds documented messages without changing existing tags; Rust, KMP,
  and TypeScript consumers pass semantic round trips and unknown-field gates;
- shared fixtures cover one/many definitions, empty and paged Sessions, equal
  timestamps, concurrent creation, invalid/cross-install tokens, and all errors;
- file-backed SQLite restart tests prove identical summaries/timelines and
  fixed-prefix behavior while a new fact commits concurrently;
- timeline matrices cover running, continuation, suspension, completion, stop,
  failure, gaps, pagination across changed Turns, bounded truncation, and every
  corrupt lifecycle;
- loopback client tests load timeline then reconnect without loss or duplicate
  mutation;
- source/log scans prove no credential, provider configuration, Engine value,
  raw fact, SQL diagnostic, or process environment discovery crosses H2.

## See also

- [`host-api-v1.md`](host-api-v1.md) — H1 command and durable event semantics.
- [`live-host-clients.md`](live-host-clients.md) — client retry and reducer rules.
- [`durable-ledger.md`](durable-ledger.md) — verified fixed-prefix reads.
- [`client-product-experience.md`](client-product-experience.md) — product UI consuming H2.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
