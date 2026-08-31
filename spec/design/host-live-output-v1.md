# Host live output v1

> This Spec defines the bounded ephemeral Runtime-to-client channel for
> progressive Agent answer presentation. It does not change H1 durable event
> replay, Ledger authority, or terminal-state semantics.

## Audience

Engineers implementing Runtime local execution, `LiveHost`, Host clients, and
the TUI live-answer projection.

## Why

Core already emits validated `ModelStreamEvent` values, but the production
local worker discards them. H1 intentionally exposes only committed SQLite
facts and cannot attach durable positions to transient output. A separately
named H4 channel is required so clients can present real received progress
without treating it as replayable conversation truth.

## Ownership

| Concern | Owner |
|---|---|
| model stream validation and order | Engine LLM/Core |
| public event admission, redaction, bounds, active snapshot | Runtime H4 |
| SSE encoding and subscriber lifetime | Live Host transport |
| wire validation, bounded delivery, gap detection | Host client |
| frame batching, mutable preview, viewport behavior | TUI |
| completed answer, terminal state, reconnect truth | Ledger plus H1/H2 |

No H4 value is written to the Ledger or TUI local storage. Diagnostics may
record only event kind, byte count, sequence, and safe failure code.

## Endpoint

```text
GET /v1/sessions/{session_id}/live
Accept: text/event-stream
```

The route uses the same loopback and Session ownership boundary as H1. An
unknown Session returns H1's redacted `404 not_found`. A valid Session without
an active execution opens the stream and sends keepalive comments until an
execution starts or the client disconnects.

Every semantic record is:

```text
event: live
data: {LiveOutputEventV1 JSON}
```

It has no SSE `id`. `Last-Event-ID` is ignored. Heartbeat comments carry no
semantics. EOF is neither a Turn terminal nor proof that the execution ended.

## Envelope

```text
LiveOutputEventV1 {
  api_version: "v1",
  session_id: string,
  turn_id: string,
  execution_id: string,
  stream_id: string,
  sequence: u64,
  kind: "snapshot" | "text_delta" | "phase_changed" |
        "preview_unavailable" | "ended",
  text?: string,
  phase?: "preparing" | "generating" | "finalizing",
  label_key?: "agent.live.preparing" | "agent.live.generating" |
              "agent.live.finalizing",
  through_sequence?: u64,
  reason?: "terminal_committed" | "suspended" | "stopped" |
           "failed" | "publisher_closed"
}
```

All identities are non-empty, bounded by the existing Host identity limits,
and must match the requested Session. `stream_id` is a fresh lowercase UUID v4
for one in-memory publisher generation. `sequence` starts at one and increases
by exactly one within that stream.

Fields forbidden by a kind are absent, not null:

| Kind | Required fields | Meaning |
|---|---|---|
| `snapshot` | `text`, `through_sequence` | full admitted preview through the stated source sequence |
| `text_delta` | `text` | ordered non-empty UTF-8 suffix |
| `phase_changed` | `phase`, `label_key` | closed public activity phase |
| `preview_unavailable` | none | preview cannot remain complete; discard local text |
| `ended` | `reason` | publisher ended; wait for durable truth |

A snapshot's envelope `sequence` equals `through_sequence`, the latest source
sequence represented by its text. The next broadcast event is exactly one
greater. The first event delivered to a new subscriber for an available active
execution is `snapshot`, even when its text is empty. An execution whose bound
was exceeded instead starts that subscriber with `preview_unavailable` at the
current sequence.

## Admission mapping

Runtime admits only these Core events:

| Core event | H4 event |
|---|---|
| `ExecutionStarted` or `ContextDerived` | `phase_changed(preparing)` |
| `ModelRequestPrepared` | `phase_changed(generating)` |
| text `OutputItemStarted` | `phase_changed(generating)` |
| `TextDelta` for an admitted text item | `text_delta` |
| `OutcomeProposed` | `phase_changed(finalizing)` |
| Runtime terminal commit outcome | `ended` with matching safe reason |

Repeated equal phases are coalesced. Runtime never publishes `ReasoningDelta`,
`RefusalDelta`, `ToolArgumentsDelta`, usage, model target, request identity,
provider payload, tool identity, prompt/context content, exception text, or
credentials through H4. A later Spec may admit typed tool activity by mapping
durable C5/H3-safe values; raw Agent callbacks cannot do so.

If multiple text output items are admitted for one answer, Runtime inserts the
same deterministic separator used by committed completion projection. Clients
never guess item boundaries from delta timing.

## Hub state and bounds

`LiveOutputHub` is an explicitly injected Runtime port shared by the execution
worker and `LiveHost`. It owns at most one active generation per Execution and
these product-default bounds:

| Bound | Default |
|---|---:|
| active executions | dispatch queue capacity |
| accumulated preview per execution | 1 MiB UTF-8 |
| one encoded event | 32 KiB |
| broadcast capacity | 256 events |
| subscribers per Session | 8 |

Construction fails when any bound is zero or preview/event bounds exceed Host
read-response bounds. A provider delta larger than the event bound is split at
UTF-8 scalar boundaries without changing text. Adjacent deltas may be
coalesced before publication, but their concatenation is exact.

When accumulated preview would exceed its bound, the hub clears the text,
publishes `preview_unavailable`, and accepts no more public text for that
generation. It continues phase/end publication. Truncating a prefix or suffix
and presenting it as a complete preview is forbidden.

The hub removes terminal generations after publishing `ended` and retaining it
for one subscriber grace interval. It removes abandoned publishers on bounded
worker shutdown. Restart begins a new `stream_id`; nothing is recovered from
disk.

## Subscriber and backpressure rules

The publisher never awaits a terminal renderer. A subscriber that falls behind
the bounded broadcast buffer receives a local `gap` result, not a silently
advanced sequence. The HTTP route closes that stream on a gap; the client turns
the non-terminal EOF into local `PreviewUnavailable` before reconnecting.
Reconnect receives a current snapshot when the generation is still active.

The Rust client validates exact version, Session ownership, identity bounds,
closed kind/field combinations, stream consistency, and contiguous sequences.
It sends semantic values through a bounded channel of 256. Unlike durable H1,
H4 may drop the incomplete preview instead of applying TCP backpressure to
Agent execution. A full client channel emits `PreviewUnavailable` locally,
cancels the live follow, and reconnects for a snapshot.

Unknown kinds are a protocol failure for v1 because their field and redaction
semantics are not known. Keepalives are consumed inside the adapter.

## TUI reduction

The TUI keys live state by `(session_id, turn_id, execution_id, stream_id)`:

```text
LiveAnswerState {
  received_text,
  presented_text,
  phase,
  last_sequence,
  availability,
  received_at,
}
```

- `snapshot` atomically replaces received and presented text.
- `text_delta` appends exactly once after contiguous validation.
- `phase_changed` changes safe feedback without transcript content.
- `preview_unavailable` clears both text buffers and shows one quiet waiting
  state; it never leaves a misleading suffix.
- `ended` stops the live caret but does not create a terminal timeline item.
- an H1 terminal or H2 terminal snapshot atomically removes matching live state
  and installs the committed answer.
- live events arriving after a known terminal or for an older Execution are
  ignored without changing the durable cursor.

Selecting a Session installs H2 durable state first, starts H1 from the durable
watermark, and independently starts H4. H4 reconnect never changes the H1
cursor. Switching away may keep a bounded background live follow, but only the
selected Session stores preview text in the application model.

## Frame presentation

Received deltas request rendering immediately. The event loop coalesces them to
one frame when multiple values arrive before the next draw. `presented_text`
must reach `received_text` within two available render frames; catch-up is
immediate when the bound would be exceeded. Reduced motion disables the live
caret and transition effects, not progressive content.

Markdown rendering maintains a monotonic stable block prefix and reparses only
the mutable final block. Resize may reflow the whole ephemeral preview. User
scroll detachment is preserved; new text updates the unseen count and never
forces follow mode.

## Failure and convergence matrix

| Condition | Live presentation | Durable behavior |
|---|---|---|
| H4 disconnect/EOF | `live feedback unavailable`; reconnect | unchanged |
| sequence gap/client overflow | clear preview; request snapshot | unchanged |
| hub preview overflow | clear preview; wait for terminal | unchanged |
| malformed/redaction-invalid event | clear preview; protocol failure | unchanged |
| worker failure before terminal | end may be absent | H1/H2 failure remains authority |
| terminal arrives before late live data | remove/ignore live state | committed result wins |
| process restart | new generation or no preview | H2 snapshot plus H1 cursor wins |

## Acceptance

- a real model transport fragment produces multiple visible TUI frames before
  terminal commit;
- no completed text is replayed as fake deltas;
- Runtime tests prove exact text, phase admission, redaction, split boundaries,
  overflow, lag, reconnect snapshot, generation replacement, and cleanup;
- HTTP tests prove no SSE ID, heartbeat neutrality, session ownership, and
  gap-close behavior on a real loopback listener;
- client tests prove field validation, sequence gaps, bounded-channel overflow,
  cancellation, and independence from the H1 cursor;
- reducer/render tests prove snapshot replace, delta append, unavailable clear,
  late-event rejection, durable final replacement, resize, scroll detachment,
  reduced motion, and Unicode Markdown boundaries;
- macOS PTY evidence records first delta, at least two intermediate frames,
  final committed frame, and restored terminal state from the shipping binary.

## Meta

- Owner: Runtime H4 and Client presentation
- Last reviewed: 2026-08-31
- Status: accepted; Runtime hub and worker publication implemented, Host/client composition pending
