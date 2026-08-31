# TUI product redesign

> This decision record resets the rejected terminal-product direction. It
> defines one coherent interaction model and the ownership boundary required
> for truthful progressive output before detailed Specs or code are changed.

## Decision

Garive TUI is a conversation-first work surface, not a terminal dashboard. Its
primary plane is one readable transcript, one persistent composer, and one
small context line. Navigation, activity, recovery, and commands appear only
when they answer the user's current question.

Codex is the primary implementation baseline for streaming and transcript
discipline. Claude Code corroborates low-latency render batching, atomic
partial-to-final replacement, and prompt-adjacent feedback. Qoder CLI informs
discoverable commands, resumable work, and background-task visibility only
where its official package or documentation proves the behavior. Garive keeps
its own identity, Host semantics, components, tokens, and interaction grammar.

## Product principles

1. **The answer is the visual center.** Chrome never competes with the current
   user request, active work, or Agent response.
2. **Truth changes presentation.** Durable facts, ephemeral progress, local
   drafts, and inferred convenience state are visually and structurally
   distinct.
3. **One action has one home.** Keyboard, mouse, command palette, and help route
   to the same typed intent instead of implementing parallel behavior.
4. **Motion communicates change.** A live caret, bounded frame batching, and
   stable-to-final replacement are allowed. Decorative loops and delayed fake
   typing are not.
5. **Density is progressive.** The default surface is quiet; details expand in
   place and remain available in the transcript inspector.
6. **Narrow is a composition, not a crop.** Components reflow or collapse by
   priority without horizontal scrolling of the application shell.

## Information architecture

The fullscreen frame has four vertical regions:

```text
┌ context line: Session title · Agent · connection ──────────────┐
│                                                                │
│ transcript: user request                                       │
│             compact activity stack                             │
│             progressively rendered Agent answer                │
│                                                                │
├ composer: prompt, attachments, mode, submit/cancel ────────────┤
└ transient hint: one contextual chord or recovery action ──────┘
```

- The context line is one row. It is not a bordered toolbar. Session identity
  leads; connection state appears only when non-healthy.
- Transcript content uses a bounded reading measure on wide terminals. Extra
  width becomes breathing room or an optional inspector, never longer prose.
- User turns use a restrained surface and a left role marker. Agent prose uses
  the terminal background with stronger typographic rhythm. Neither is placed
  in a heavy full-width card.
- Activity belongs between request and answer. The active item is one visible
  row; completed siblings collapse into a summary such as `3 actions · 8s`.
- The composer is the only persistent framed surface. Its border, cursor, and
  footer share one focus state and never create a second status bar.
- Overlays are centered, bounded components for selection or confirmation.
  They do not permanently divide the transcript into panes.

## Component system

Each component owns semantics, layout, render, hit regions, and snapshots:

| Component | Stable responsibility |
|---|---|
| `ContextLine` | Session/Agent breadcrumb and exceptional connection state |
| `TurnBlock` | one user request, its work, answer, and terminal outcome |
| `ActivityStack` | active safe activity plus collapsed completed summary |
| `LiveAnswer` | ephemeral received text, live caret, gap/overflow state |
| `MarkdownAnswer` | canonical committed answer and code/table rendering |
| `Composer` | multiline draft, selection, attachments, modes, submission |
| `CommandPalette` | slash discovery, global actions, exact typed intents |
| `DecisionSheet` | approval, typed response, destructive confirmation |
| `SessionSwitcher` | recent Sessions, lifecycle, search, resume |
| `Inspector` | opt-in activity/recovery/details without changing truth |
| `HintLine` | one highest-priority contextual hint; absent when unnecessary |

Components consume presentation values. They cannot start HTTP tasks, read
SQLite, classify Host errors, or infer a Turn terminal.

## Visual grammar

- Spacing is the primary separator: one blank row between Turns, none between
  tightly related activity rows, and one row above the composer.
- Borders are reserved for focus, modal boundaries, and the composer. Repeated
  transcript cards and nested boxes are prohibited.
- Color encodes role or state, never structure alone. Every state also has a
  word, glyph, or placement distinction and remains legible in mono mode.
- The accent color appears on the insertion caret, current selection, active
  work indicator, and primary decision only. Large accent fills are prohibited.
- Muted text is secondary, not low-contrast. Public status keys use stable
  copy; raw internal codes and opaque IDs stay out of the default surface.
- Animation is capped by the reduced-motion preference. The no-motion path
  still presents every admitted state transition.

## Progressive output experience

The desired typewriter-like experience comes from real intermediate events,
not replaying a completed answer character by character.

```text
ModelStreamEvent::TextDelta
  -> Agent EventSink
  -> Runtime LiveOutputHub
  -> ephemeral Host live stream
  -> client validation and bounded channel
  -> LiveAnswer accumulator
  -> frame governor
  -> stable markdown prefix + mutable tail + live caret
```

The frame governor renders at most once per terminal frame and always advances
toward the latest received text. It may coalesce adjacent deltas, but it cannot
invent characters, reorder fragments, or intentionally trail received content
by more than two frames. When a burst exceeds that latency bound, it catches up
in one frame rather than preserving a cosmetic per-character cadence.

Only model text explicitly admitted for public answer presentation is exposed.
Raw reasoning, tool arguments, provider payloads, credentials, and internal
events never cross this boundary. Safe activity remains a separate typed value.

The active answer has three regions:

- a source-backed stable markdown prefix that will no longer be reparsed;
- a mutable final block that may reflow as more text arrives;
- a subtle live caret shown only while the execution is active.

The durable completed answer is authoritative. It atomically replaces the
entire ephemeral projection, including when deltas were missed. Interruption
uses durable stopped/failed/suspended truth; it does not silently promote a
partial preview to a completed answer.

## Live-output ownership

Durable H1 events retain their existing SQLite positions and replay contract.
Live output uses a separately named Runtime/Host contract because it is
ephemeral and cannot truthfully share a durable cursor.

The Runtime owns one bounded in-memory live snapshot per active execution. A
subscriber receives that snapshot before later deltas, so reconnect never
concatenates a suffix onto an unknown prefix. Each event carries exact Session,
Turn, Execution, generation, and monotonically increasing sequence identity.

The channel is allowed to lose progress only visibly:

- a sequence gap clears the partial answer and requests the current snapshot;
- hub overflow changes the projection to `preview unavailable` until a snapshot
  or durable terminal arrives;
- disconnect leaves the durable Turn running and marks only live feedback
  unavailable;
- late events for a terminal or different Execution are ignored.

The live snapshot is memory-only, bounded, content-redacted, and removed after
terminal convergence. It is never written to TUI preferences, diagnostics, or
the Ledger.

## Responsive behavior

| Width | Composition |
|---:|---|
| `>= 120` | centered transcript plus optional 32-column inspector |
| `80..119` | centered transcript; inspector becomes overlay |
| `52..79` | full-width transcript; metadata collapses to compact labels |
| `40..51` | linear transcript; activity summary and single-line hint |
| `< 40` | explicit minimum-size view; composer draft remains recoverable |

Height pressure removes optional help and collapsed history before it reduces
the composer below two rows or hides an active decision.

## Interaction rhythm

- `Enter` submits only when the composer contract says the draft is complete;
  multiline insertion remains explicit and consistent with the published
  keymap.
- `Esc` first closes the nearest transient layer, then primes interruption of
  the active Turn. It never exits the application through an overlay.
- `/` opens the same command catalogue used by help and searchable actions.
- Session switching preserves drafts and follows active work without changing
  the selected Session's durable state.
- While output streams, the user can scroll, select, copy, inspect activity,
  edit the next draft, or interrupt without freezing rendering.
- Returning to follow mode is an explicit action once the user scrolls away;
  new output never steals their viewport.

## Rejected directions

- persistent dashboard columns around ordinary conversation;
- full-width borders for every timeline item;
- a permanent legend of shortcuts competing with the composer;
- fake typing generated from a final committed message;
- durable-looking IDs or positions attached to ephemeral deltas;
- raw reasoning or tool arguments presented as public progress;
- silent delta loss, suffix-only reconnect, or partial text promoted to final;
- separate keyboard and mouse behavior that bypasses typed application intents.

## Delivery order

1. Freeze the source audit and live-output wire/state contract.
2. Prove the missing EventSink publication with a failing Runtime test.
3. Implement the bounded hub, snapshot/delta/end behavior, and redaction.
4. Extend the Host client and TUI reducer with gap and terminal convergence.
5. Replace the current screen composition with the component hierarchy above.
6. Validate real macOS Runtime streaming, resize, scroll detachment, interrupt,
   reconnect, reduced motion, and final replacement in PTY and screenshots.

## Implementation status

The principal conversation-first composition and H4 path are implemented.
`LiveAnswer` receives the strict Host live stream, applies bounded frame
presentation, retains a stable Markdown prefix, and converges on durable H1/H2
truth. Client wire failures and bounds are exercised by
`clients/host-rs/tests/live_output_client.rs` at `3240d960`; shipping recovery
and two live frames before durability are exercised by
`tui/tests/live_h4_recovery.rs` at `98e17709` and `a973274d`; the production
Runtime interaction flow is restored at `7547d856` in
`tui/tests/production_runtime.rs`.

`TurnBlock`, `ActivityStack`, `Composer`, `DecisionSheet`, and `Inspector` now
have separate presentation/state owners. The schema-bound DecisionSheet work is
carried by revisions `d56ab5c7`, `9df9bcba`, and `16d26a94`; Inspector's shared
projection, interaction, snapshots, and macOS PTY evidence culminate in
`821a57e4`.

This is not product closeout. Revision `9929bbb0` introduces a correlated
asynchronous effect runner and persistence port, but the full application
reducer/effect migration is incomplete. Detached-live PTY coverage,
screen-reader no-per-delta/exactly-once-final evidence, and physical Apple
Terminal plus iTerm2-class screenshot admission also remain open.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted design; principal implementation present; completion gates active
