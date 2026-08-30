# H3 — Public Agent activity projection v1

> H3 exposes a bounded, redacted account of committed interaction and effect
> progress. It extends H1 events and H2 timelines without exposing Engine facts,
> tool arguments/results, credentials, reasoning, or executor diagnostics.

## Audience

Runtime Host, Proto, shared-client, Desktop, Web, and mobile engineers building
the product activity surface promised by A-UX1.

## Why

H1 deliberately projects only Session and Turn lifecycle. The Ledger already
records governed tool and interaction lifecycles, but clients must not read or
interpret those internal facts. A product UI still needs to distinguish waiting
for approval, running work, completed work, and work requiring reconciliation.
H3 makes Runtime the sole owner of that public projection.

## Boundary

- H3 is additive to Host API v1. Existing H1 event names and protobuf tags keep
  their meaning.
- Only verified committed C6 facts may produce H3 values. Model callbacks,
  executor progress, log records, and receipt arrival before commit are not UI
  truth.
- Runtime projects one fixed Ledger prefix and validates the complete activity
  lifecycle before returning a snapshot or event page.
- The projection grants no authority. Approval/input still uses the exact H1
  continuation command and H2 suspension coordinates.
- H3 remains loopback-only with H1. It does not add remote access.

## Public wire values

The Proto SSOT adds the following message and field without changing tags 1–7:

```text
HostActivityV1 {
  api_version = "v1"
  activity_id
  kind: "tool" | "interaction" | unknown preserved string
  label_key
  state
  source_position
  terminal
  safe_code?
}

HostEventV1.activity = field 8, message presence
TurnTimelineItemV1.activities = repeated HostActivityV1
```

`HostActivityV1` assigns `api_version=1`, `activity_id=2`, `kind=3`,
`label_key=4`, `state=5`, `source_position=6` (`uint64`), `terminal=7`
(`bool`), and optional `safe_code=8`. H3 assigns `HostEventV1.activity=8` and
the coordinated H2 `TurnTimelineItemV1.activities=9`; no existing tag changes
meaning. Optional scalar presence is Proto `optional`, while activity message
presence is native message presence.

`activity_id` is the outer `tool_invocation_id` for admitted tool/effect facts
and the payload `interaction_id` for interaction facts. A preparation rejection
has no Tool Invocation ID; Runtime derives `activity_id` as lowercase SHA-256
over UTF-8 `"preparation-rejected-v1\n" + fact_id`. IDs are opaque and scoped to
their Session.

`label_key` is an admitted public localization key from the immutable installed
Agent snapshot. Runtime resolves a prepared tool name/revision to this key. Raw
tool names and arbitrary model text never become labels. Preparation rejection
uses the fixed key `agent.activity.tool_rejected`; interaction uses
`agent.activity.approval` or `agent.activity.external_input`. A missing or
invalid admitted mapping is `corrupt_state`.

H3 additively admits this D0 snapshot value:

```text
PublicToolActivityDescriptorV1 { tool_name, tool_revision, label_key }
PublicToolActivityCatalogueV1 {
  schema_version: 1, catalogue_revision, descriptors[]
}
```

Descriptors sort by raw UTF-8 tool name then revision and are unique. All
strings are non-empty/bounded; `label_key` uses the repository localization-key
grammar and is not user/model content. The catalogue is included in the
effective Agent snapshot digest. Changing a mapping requires a new catalogue
revision, Definition revision, and installed snapshot; clients never receive
tool name/revision.

Existing D0 snapshot preimage v1 remains unchanged. A Definition admitting H3
uses `effective_snapshot_version = 2`; v2 extends the exact v1 preimage with the
complete canonical `PublicToolActivityCatalogueV1` and includes the version in
the preimage. Unknown versions fail definition resolution.

Known states are:

```text
prepared | waiting_for_input | input_received | authorized | running |
completed | denied | failed | cancelled | attention_required
```

Unknown future strings remain decodable and render as a neutral non-terminal
activity. Clients never infer authority or terminal state from an unknown value.
`terminal` is authoritative and must agree with the known-state table:
`input_received`, `completed`, `denied`, `failed`, and `cancelled` are terminal;
all other known states are not. `source_position` is non-zero and no greater
than the enclosing event/timeline watermark.

`safe_code` is absent except for the closed mappings below. It is a stable enum
string, not an internal error, evidence value, exception, or provider response.
Empty strings never mean absence. JSON uses lower-snake protobuf field names;
H2's JavaScript-safe integer restriction also applies.

## Fact-to-public mapping

| Durable fact | Public event | State | Safe code |
|---|---|---|---|
| `tool.preparation_rejected` | `agent.activity.rejected` | `failed` | exact admitted rejection code |
| `effect.prepared` | `agent.activity.prepared` | `prepared` | absent |
| `interaction.requested` | `agent.activity.input_requested` | `waiting_for_input` | absent |
| `interaction.resolved` | `agent.activity.input_received` | `input_received` | absent |
| `interaction.cancelled` | `agent.activity.cancelled` | `cancelled` | exact cancellation reason |
| `effect.authorized` | `agent.activity.authorized` | `authorized` | absent |
| `effect.denied` | `agent.activity.denied` | `denied` | exact denial code |
| `effect.started` | `agent.activity.started` | `running` | absent |
| `effect.completed` | `agent.activity.completed` | `completed` | absent |
| `effect.failed` | `agent.activity.failed` | `failed` | exact admitted failure code |
| `effect.uncertain` | `agent.activity.attention_required` | `attention_required` | exact uncertainty reason |
| `effect.reconciled` | `agent.activity.reconciled` | `completed` or `failed` from decision | `reconciled_completed` or `reconciled_failed` |

`effect.receipt` and `effect.observation` remain internal because they add no
safe user action or lifecycle state. Their omission creates legal position
gaps. No ContentBinding is copied into H3.

The rejection code set is exactly C6's five
`tool.preparation_rejected` codes. Cancellation, denial, failure, and
uncertainty code sets are exactly their accepted C6 v1 enums. Adding a new code
requires an additive Host contract review; an unknown internal code fails the
known v1 projection instead of leaking it.

## Projection reducer

Within one activity, Runtime applies committed facts in ascending position.
The allowed known transitions are:

```text
effect:      prepared -> authorized | running | denied
             authorized -> running | denied | failed
             running -> completed | failed | attention_required
             attention_required -> completed | failed  (reconciled only)
rejection:   failed
interaction: waiting_for_input -> input_received | cancelled
```

The effect and interaction views may share a Tool Invocation but keep distinct
`activity_id` values. An interaction transition does not mutate the effect
activity. Receipt facts are validated between `running` and an ordinary
terminal even though they are not public. Duplicate semantic terminal facts,
terminal-to-nonterminal movement, missing preparation, mismatched identities,
or reconciliation without uncertainty is `corrupt_state`; no partial page is
returned.

An H3 SSE event contains the state after applying its one source fact. H1 event
ordering, replay, gap, duplicate, heartbeat, and EOF rules are unchanged. A
client applies only a greater position for the same activity; equal replay is
ignored and conflicting equal position fails its reducer.

Before projecting a requested event page, Runtime reconstructs activity state
through `after_position` from the verified prefix. This is required to validate
later transitions and recover the admitted label; it does not re-emit earlier
events. A scan bound failure returns no page and emits no guessed activity.

H2 timeline `activities` contains the latest public state for every activity
whose first public fact is at or before the frozen timeline prefix. Items order
by first public position ascending, then raw UTF-8 `activity_id`. A continuation
updates existing activities and may add new ones; it does not erase terminal
history. Timeline pagination never splits an activity from its owning Turn.

## Bounds and privacy

Runtime construction supplies independent non-zero limits for activities per
Turn, activity facts scanned, label bytes, activity ID bytes, and total encoded
activity bytes. Exceeding a bound returns H2 `read_bound_exceeded` for queries
or closes an H1 stream with the existing redacted protocol failure behavior;
it never truncates an activity set silently.

Public events and snapshots contain none of:

- tool arguments, results, observations, receipt/evidence content, paths, URLs,
  process commands, headers, or resource keys;
- interaction response, arbitrary prompt content, model text, reasoning, token
  usage, provider values, or credential/configuration data;
- executor, grant, authority, dispatch-attempt, receipt, model-request, or fact
  identities other than the opaque derived rejection identity.

Logs and errors may name only the stable Host code and public event kind. Debug
representations follow the same exclusion.

## Client behavior

The A-UX1 controller loads an H2 timeline snapshot, seeds activity state, then
follows H1/H3 events after the exact observed watermark. It may render a
localized label, known state, and safe action affordance derived from the Turn's
current H2 suspension. Activity state alone never enables approval, input,
cancellation, retry, or reconciliation.

Clients show `attention_required` as non-terminal and do not claim completion.
Unknown kind/state/code renders a neutral localized fallback without raw text.
Sorting, grouping, elapsed-time decoration, and animation are presentation only
and never alter durable order.

## Acceptance evidence

- additive Proto bindings and Rust/KMP/TypeScript semantic round trips preserve
  message presence and unknown strings;
- D0 snapshot fixtures prove catalogue ordering/digest, definition revision
  binding, missing/duplicate mapping refusal, and no raw tool-name projection;
- shared fixtures cover every mapping, optional interaction, denial,
  cancellation, uncertainty/reconciliation, omitted facts, gaps, duplicate
  replay, unknown client strings, and every invalid transition;
- file-backed SQLite restart tests produce byte-equivalent public event JSON
  and timeline snapshots for the same fixed prefix;
- controller fixtures prove snapshot-then-follow has no loss or duplicate
  mutation across reconnect and continuation;
- bounds, redaction canaries, source/log scans, and generated JSON snapshots
  prove no forbidden field or content crosses the Host boundary.

The fixture file is `spec/fixtures/host/host-agent-activity-v1.json`, declares
`schema_version = 1`, and contains `projection_cases`, `timeline_cases`,
`reducer_cases`, `bound_cases`, and `redaction_cases`. Every case name is unique;
readers reject unknown case fields and duplicate names.

## See also

- [`host-api-v1.md`](host-api-v1.md) — H1 event transport and replay rules.
- [`host-read-model-v1.md`](host-read-model-v1.md) — H2 bounded timelines.
- [`durable-runtime-facts.md`](durable-runtime-facts.md) — C6 fact schemas.
- [`client-product-experience.md`](client-product-experience.md) — consuming UI contract.

## Delivery evidence

- Runtime reduces verified committed effect and interaction facts through one
  closed transition implementation shared by H1 event pages and H2 timelines.
- The reducer validates admitted safe-code sets, receipt identity/classification,
  reconciliation order, JavaScript-safe source positions and independent count,
  identity, label and encoded-byte bounds without partial query results.
- File-backed restart tests prove byte-equivalent events and timelines. Canary
  assertions prove internal tool names, results, observations, executor/grant,
  receipt and dispatch identities do not cross the public projection.
- `host-agent-activity-v1.json` enumerates every fact mapping, gap, transition,
  bound and redaction family and is consumed by strict Rust, Kotlin and
  TypeScript readers.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
