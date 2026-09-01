# TUI competitive evidence audit — 2026-09-01

> Evidence for Garive's streaming, composer, overlay, status, keyboard,
> reconnect, and persistence decisions. This is a pinned local-source audit,
> not a claim that competitor internals are compatible with Garive protocols.

## Method and provenance

Commands were run locally on macOS. No source was downloaded for this audit.
Line references are valid at the pinned revisions below.

| Product | Pinned local evidence | Provenance and limit |
|---|---|---|
| Codex | `/Users/christmix/OraculoSpace/codex` at `16fbfe557446a1af94da81e1144029ccc1311ad0`; `/opt/homebrew/bin/codex --version` prints `codex-cli 0.150.1` | `git remote get-url origin` prints `https://github.com/openai/codex.git`; implementation source is available. |
| Claude Code | `/Users/christmix/OraculoSpace/claude-code` at `681a8be245e7759a405e276b16ae69ea6b75076f`; `/opt/homebrew/bin/claude --version` prints `2.1.231 (Claude Code)` | `git remote get-url origin` prints `https://github.com/anthropics/claude-code.git`. `README.md:46-50` identifies the repository's CLI entry point and plugins; local tree inspection found no TUI implementation source. Only official changelog and executable behavior support UI conclusions. |
| QoderCLI | `/opt/homebrew/bin/qodercli -> /opt/homebrew/Caskroom/qodercli/0.2.16/qodercli`; `qodercli --version` prints `0.2.16` | `find /Users/christmix/OraculoSpace -maxdepth 2 -type d \( -iname '*qoder*' -o -iname '*qorder*' \)` returns no source tree. Only executable help behavior is admissible. |

`/Users/christmix/OraculoSpace/claude-code-source-code` was found, but its
origin is `git@github.com:sanbuphy/claude-code-source-code.git`, not Anthropic.
It is excluded: **Reject** any requirement inferred from that decompiled or
reverse-engineered tree because provenance and version parity are unverified.

## Evidence ledger

### Codex implementation source

- **CX-S1 — two-region stream.**
  `/Users/christmix/OraculoSpace/codex/codex-rs/tui/src/streaming/controller.rs:1-10`
  defines a stable scrollback region plus mutable active-cell tail owned by
  `StreamCore`; `:30-36` states its ordering invariants.
- **CX-S2 — structural holdback and final truth.**
  `StreamCore::push_delta`, the same file `:127-158`, admits only
  newline-terminated source and holds ambiguous table rows out of both stable
  output and visible tail. `StreamCore::finalize_remaining`, `:160-178`,
  re-renders the full raw source as the canonical final transcript.
- **CX-S3 — cadence is a separate queue.** `StreamCore::tick` and
  `StreamCore::tick_batch`, the same file `:180-198`, drain one or a bounded
  number of already-rendered lines. `StreamCore::current_tail_lines`,
  `:218-229`, starts after enqueued rather than merely emitted lines to prevent
  duplication.
- **CX-S4 — source-backed resize.** `StreamCore::set_width`, the same file
  `:236-284`, recomputes from source and rebuilds the pending stable queue.
  `/Users/christmix/OraculoSpace/codex/codex-rs/tui/src/history_cell/messages.rs:354-381`
  stores finalized raw Markdown for reflow; `StreamingAgentTailCell`,
  `:481-529`, is transient and replaced by new deltas or finalization.
- **CX-I1 — popup ownership.**
  `/Users/christmix/OraculoSpace/codex/codex-rs/tui/src/bottom_pane/chat_composer.rs:12-17`
  describes routing to the active popup before the ordinary composer and
  synchronizing popup state after every key. `ChatComposer::handle_key_event`,
  `:1891-1919`, implements that dispatch.
- **CX-I2 — one keymap snapshot.** `ChatComposer::set_keymap_bindings`, the
  same file `:844-881`, updates submit, queue, editor, and footer hints from one
  `RuntimeKeymap`. `BottomPane::set_keymap_bindings` in
  `/Users/christmix/OraculoSpace/codex/codex-rs/tui/src/bottom_pane/mod.rs:392-408`
  propagates that snapshot to composer, status, and pending-input surfaces.
- **CX-I3 — modal and cancellation precedence.**
  `BottomPane::handle_key_event`, the same `bottom_pane/mod.rs:616-697`, gives
  the top view first ownership and prefers popup dismissal before task
  interruption. `BottomPane::on_ctrl_c`, `:709-742`, lets the active view or
  history search consume cancellation before draft clearing or process quit.
- **CX-V1 — restrained prompt surface.**
  `/Users/christmix/OraculoSpace/codex/codex-rs/tui/src/style.rs:16-90`
  derives a low-contrast User surface from the terminal background, while
  `history_cell/messages.rs:100-126` and its reviewed
  `user_history_cell_wraps_and_prefixes_each_line_snapshot` use one `›` marker
  and hanging continuation indent. Garive adopts that density baseline, then
  strengthens it with grapheme/display-width wrapping, explicit light/dark/mono
  semantic styles, and a separate linear `You` announcement.
- **CX-V2 — open composer density.**
  `/Users/christmix/OraculoSpace/codex/codex-rs/tui/src/bottom_pane/chat_composer.rs:4420-4458`
  derives height from textarea plus footer rather than a surrounding border;
  `:4751-4780` applies the prompt surface and paints one `›` lead. Reviewed
  snapshots `footer_mode_hidden_while_typing` and
  `footer_collapse_empty_mode_only` keep the input open and reserve compact
  footer context. Garive adapts this density into `ComposerDock`, while
  retaining its own Host truth, Unicode editor, selection, frozen-state row,
  and pointer geometry.
- **CX-R1 — provider retry is not subscription resume.**
  `/Users/christmix/OraculoSpace/codex/codex-rs/core/src/responses_retry.rs:15-16`
  sets 5 s initial and 60 s maximum connection delay.
  `handle_retryable_response_stream_error`, `:38-112`, reports actionable
  reconnect state, may switch WebSocket to HTTPS, and otherwise retries a
  model response. It carries no Garive H1 cursor or H4 generation/sequence.

### Claude Code official product evidence

- **CC-S1 — bounded live rendering outcomes.**
  `/Users/christmix/OraculoSpace/claude-code/CHANGELOG.md:663` says live-preview
  updates no longer rerender the whole screen; `:867` records 100 ms text
  coalescing; `:964` records line-by-line long-paragraph streaming; `:721` and
  `:1002` record preservation of partial output on mid-stream failure. These
  are release claims, not implementation evidence.
- **CC-I1 — selection and editor behavior.** The same changelog `:29` records
  a command menu whose highlight marks only the selected row and whose matches
  use bold; `:1132` records Up/Down traversing wrapped visual rows before
  prompt history.
- **CC-A1 — accessible projection.** The same changelog `:125`, `:195`, and
  `:220-221` records incremental input echo and cursor placement on the focused
  row. `:498` records opt-in plain-text screen-reader rendering. Locally,
  `claude --help` describes `--ax-screen-reader` as flat text without decorative
  borders or animations.
- **CC-R1 — reconnect must remain visible and intentional.** The same
  changelog `:47` records a growing-backlog reconnect defect; `:73-76` records
  rejecting prior-history upload into a replacement session and replacing an
  expiring toast with a persistent failure indicator, details, and reconnect
  shortcut. `:84` records that resume must not silently re-enable a connection
  the user disabled.
- **CC-P1 — headless partial/persistence boundary.** Local `claude --help`
  exposes `--include-partial-messages` only with `--print` plus
  `--output-format=stream-json`, `--input-format=stream-json` for streamed
  input, and `--no-session-persistence` as making a print session unsaved and
  non-resumable. It separately exposes `--continue`, `--resume`, and
  `--fork-session`.

### QoderCLI executable evidence

- **QD-B1 — explicit headless stream boundary.** Local `qodercli --help`
  exposes `--output-format` values `text|json|stream-json` and says
  `--input-format=stream-json` reads NDJSON messages from stdin.
- **QD-P1 — explicit session actions.** The same command exposes
  `--continue`, `--resume`, `--fork-session`, `--session-id`,
  `--remote-session`, `--teleport`, `--list-sessions`, and
  `--delete-session`. It exposes no interactive TUI keymap, screen-reader
  projection, partial-message switch, storage format, cursor, or reconnect
  contract.

## Adopt / Adapt / Reject decisions

| Area | Decision | Garive resolution | Evidence |
|---|---|---|---|
| Intermediate output | **Adopt** | Render a monotonic stable Markdown prefix and a source-backed mutable tail; final durable truth atomically replaces the preview. | CX-S1, CX-S2, CX-S4 |
| Streaming cadence | **Adapt** | Coalesce arrivals by terminal frame and catch up within Garive's two-frame bound. Line-level competitor animation is evidence that cadence is separable, not a mandate for artificial token timing. | CX-S3, CC-S1 |
| Fake typewriter | **Reject** | Do not invent characters, replay completed text, or preserve cosmetic per-character delay. Exact H4 deltas remain the only source of intermediate text. | CX-S2, CX-S3; CC-S1 is product-only evidence |
| Partial structural Markdown | **Adapt** | Keep structurally ambiguous content mutable, but apply Garive's parser budgets and H4 gap rules rather than copying Codex's newline/table heuristic verbatim. | CX-S2, CX-S4 |
| Overlay ownership | **Adopt** | Top overlay owns the key; dismissal/selection resolves before editor, focused region, interrupt, or global actions. | CX-I1, CX-I3 |
| Binding consistency | **Adopt** | Resolve visual keycap, spoken name, controller action, and hint from one application-owned binding snapshot. | CX-I2 |
| Composer navigation | **Adapt** | Up/Down traverses wrapped visual rows before history; Garive retains its documented Unicode/grapheme and logical-line contracts. | CC-I1; implementation source unavailable |
| Selection styling | **Adapt** | Keep accent on one textual selection marker, bold grapheme-safe match spans, and reverse only the marker in mono so selection never depends on blue or floods the row. | CC-I1, CC-A1 |
| User request hierarchy | **Adapt** | Use one low-contrast, unbordered request surface with a non-color marker and hanging Unicode indent; keep screen-reader role wording explicit. | CX-V1 |
| Composer density | **Adapt** | Use an open low-contrast ComposerDock with one lead and separate contextual row; frozen/action truth changes wording and tone without adding a permanent frame. | CX-V2 |
| Screen-reader projection | **Adapt** | Keep Garive's linear presenter and semantic announcements; suppress decorative animation and per-delta transcript chatter. | CC-A1; executable `claude --help` behavior |
| Reconnect status | **Adopt** | A continuing failure is persistent, names the state, exposes details safely, and offers an explicit retry; it is not toast-only. | CX-R1, CC-R1 |
| H1/H4 reconnect algorithm | **Reject** | Do not copy provider sampling retry or remote-session behavior. H1 resumes from its typed durable cursor; H4 reconnects by generation/sequence snapshot rules and never advances H1. | CX-R1, CC-R1; neither exposes Garive cursor semantics |
| Headless stream format | **Adapt** | Treat stream-json/NDJSON as evidence for an explicit machine-stream boundary only; it does not define interactive H4 cadence or durable authority. | CC-P1, QD-B1 |
| Resume/fork semantics | **Adapt** | Keep resume, fork, and connection intent explicit, but retain Garive's Session/Turn identity and Host authority. | CC-R1, CC-P1, QD-P1 |
| Local persistence | **Reject** | Do not infer or copy competitor storage internals. Garive persists only disposable preferences/history and exact pending mutation envelopes; completion and cursor truth stay with Runtime/Ledger. | CC-P1 and QD-P1 expose behaviors but no storage schema; QD-P1 explicitly lacks implementation evidence |

## Normative consequences for Garive

1. An admitted H4 snapshot or contiguous delta updates ephemeral received
   source exactly once. Stable prefix and mutable tail are presentation state,
   never durable transcript or cursor state. [CX-S1, CX-S2, CX-S4]
2. Rendering is frame-coalesced and bounded; it is not a token timer. A burst
   catches up rather than turning finished output into a long animation.
   [CX-S3, CC-S1]
3. H1/H2 terminal truth atomically removes the matching preview. Disconnect,
   gap, overflow, or malformed H4 cannot manufacture a committed answer.
   [CX-S2, CX-R1]
4. The active overlay is the sole input owner. One resolved keymap snapshot
   supplies behavior, visible hints, and linear/screen-reader action names.
   [CX-I1, CX-I2, CX-I3, CC-A1]
5. Reconnect failures remain visible until recovery or explicit dismissal.
   H1 durable cursor and H4 ephemeral generation/sequence remain separate.
   [CX-R1, CC-R1]
6. Unknown mutation outcomes remain persist-before-send exact replays. Neither
   competitor transcript files nor headless stream-json become Host authority.
   [CC-P1, QD-B1, QD-P1]

## Known gaps

- The official Claude Code checkout does not contain TUI implementation source.
  Changelog and `--help` observations justify product-level adaptation only.
- No local Qoder/QoderCLI source checkout exists. The binary help establishes
  exposed commands only; interactive cadence, overlays, key precedence,
  accessibility, persistence format, and reconnect internals remain unknown.
- Therefore this audit deliberately makes no implementation-level parity claim
  for Claude Code or QoderCLI and requires new pinned evidence before adopting
  any unlisted behavior.
