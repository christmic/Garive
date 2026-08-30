# TUI source audit

> This research note records the exact source revisions inspected before the
> Garive TUI contract is admitted. It separates verified implementation facts,
> product observations, transferable patterns, and rejected ownership choices.

## Audience

Engineers designing or reviewing `tui/`, `clients/host-rs/`, and the Runtime
Host boundary that the terminal client consumes.

## Why

At the audit-start revision, `garive-tui` was a one-shot line printer. The
resident replacement required explicit decisions for event routing, terminal
lifecycle, input, Session recovery, persistence, rendering, and testing. Those
decisions were derived from Garive's durable contracts and inspected
implementations rather than product screenshots or remembered behavior.

This note is evidence, not a Garive contract. Normative decisions belong in
the focused TUI Specs linked below.

## Source manifest

| Source | Revision | Status | License/use |
|---|---|---|---|
| Garive | `0439eb80741bdcbc3c84da44a6affb9a5faaa62c` | Local and remote `master` at audit start | MIT; normative local baseline |
| Grok Build | Git `bc7f02eddd3d84085849dc19ed216f11c23b0571`; `SOURCE_REV` `d5a0335a47221e8c9519936cb693e9b6450227ec`; pager `1.0.12` | Official public source cloned from `xai-org/grok-build` on 2026-08-30 | Apache-2.0; implementation evidence |
| OpenAI Codex | Git `16fbfe557446a1af94da81e1144029ccc1311ad0` | Local official public source from `openai/codex` | Apache-2.0; implementation evidence |
| Pi coding agent | Git `11b5403fade10cfda22d31f80a1ed276458b1fbe` | Local official public source from `badlogic/pi-mono` | MIT; independent implementation evidence |
| Claude Code | Official repository Git `681a8be245e7759a405e276b16ae69ea6b75076f`, tag `v2.1.228`; installed binary `2.1.231` | Official repository does not contain the shipping TUI implementation | Product and public-interface evidence only |
| Claude Code unpacked study | Git `3da94d5e5f2b99c9d82b0d8f09448b04775cd41f`, package claim `2.1.88` | Third-party extraction; README declares 108 missing modules and non-commercial restrictions | Corroboration only; no code or distinctive text may be copied |
| Microsoft Win32 security/file APIs | Microsoft Learn pages retrieved 2026-08-30 | Official platform contract | Primary evidence for the Windows persistence boundary |

The two Apache repositories may inform implementation structure, but Garive
still authors its own types and code against its Host contract. The unpacked
Claude material cannot establish absence, completeness, or a transferable
implementation.

## Garive audit-start baseline

This section records the state at the source-audit revision. It is intentionally
historical; the closure table below points to the current implementation rather
than silently rewriting what was originally inspected.

### Confirmed ownership

| Fact | Evidence | Consequence |
|---|---|---|
| Clients depend on a Runtime-owned Host boundary and must not read Engine state or SQLite. | `.agents/architecture.md`; `runtime/AGENTS.md`; `spec/design/live-host-clients.md` | `tui/` owns presentation and local disposable state only. |
| Session, Turn, event positions, terminal outcomes, and continuation authority are durable Host truth. | `spec/design/host-api-v1.md`; `spec/design/host-read-model-v1.md` | TUI reducers never infer a terminal from EOF, timeout, or local process state. |
| Mutation retry must reuse the same command identity and byte-equivalent request. | `spec/design/live-host-clients.md`; `clients/host-rs/src/client.rs:45-153` | Pending commands require an explicit lifecycle and optional crash-safe local record. |
| Event positions may contain gaps; duplicates at or below the saved cursor are valid. | `clients/host-rs/src/reducer.rs:15-69` | Cursor reduction is monotonic but not contiguous. |
| The audit-start Rust client blocked until a terminal and exposed no read-model queries. | Audit revision `clients/host-rs/src/client.rs:155-266`; `clients/host-rs/src/lib.rs:1-15` | A resident UI needed an async event bridge plus H2 query support; it could not poll storage. |
| The audit-start binary always created a Session, submitted one Turn, printed events, and exited. | Audit revision `tui/src/main.rs:1-71` | That code was a transport smoke client, not the target application architecture. |
| Audit-start TUI evidence used a scripted TCP responder rather than Runtime composition. | Audit revision `tui/tests/live_h1.rs:1-74`; `spec/STATUS.md` | It was useful as a parser test but could not close product E2E. |

### Audit-start contract gaps and current closure

| Audit-start gap | Current evidence |
|---|---|
| Installed Agent discovery, Session listing, and timeline reopen | H2 Runtime projection, strict cross-language fixtures, Rust client queries, and resident Session navigation tests |
| Public suspension coordinates and response schema | H2 timeline suspension view, canonical typed continuation, schema-form tests, and production Runtime continuation E2E |
| Typed redacted activity | H3 fixed-prefix projection, strict redaction fixtures, Rust client reduction, and Runtime activity tests |
| Raw-string-only continuation | Exact RFC 8785 JSON continuation field and byte-identity client/Runtime tests |
| Whole-operation terminal follow | Bounded incremental client stream plus supervised foreground/background TUI follow tasks |

These dependencies are closed. `A-TUI` remains active for the independent T7
completion gates listed in `tui-quality-and-verification.md`.

## Grok Build findings

### Application pipeline

`crates/codegen/xai-grok-pager/src/app/mod.rs:1-12` names the application
modules. `app/actions.rs:1-7`, `30-36`, `1312-1315`, and `2177-2178` define a
three-stage pipeline:

```text
terminal or agent event -> Action -> pure dispatch -> state + Effect
Effect -> async task -> TaskResult -> Action
```

The event loop is intentionally I/O-only
(`app/event_loop.rs:1-4`). Its biased `tokio::select!` begins at line 2390 and
prioritizes connection cancellation, quit, writer failure, protocol traffic,
task results, input, and timers. Lines 2426-2435 document a starvation policy
rather than relying on default scheduler fairness.

Garive adopts the separation, but not Grok-specific ACP types or the large
enumerations. The Garive reducer will use small domain intents, effects, and
correlated results derived from Host v1.

### Terminal lifecycle

`app/mod.rs:1380-1445` enters raw/alternate-screen state and enables bracketed
paste. `app/mod.rs:1584-1745` centralizes ordered teardown for normal exit and
panic paths. The writer is drained before leaving alternate screen so a late
frame cannot corrupt the user's shell. `app/mode_switch.rs` treats screen-mode
changes as lifecycle transitions rather than styling flags.

Garive adopts one idempotent terminal guard that owns raw mode, alternate
screen, bracketed paste, focus/mouse features, cursor visibility, and panic
restoration. Garive initially ships one fullscreen mode; a scrollback-native
mode stays out until its reflow and terminal-compatibility evidence exists.

### Session and persistence boundary

`xai-grok-shell/src/session/storage/mod.rs:1177-1473` defines a storage adapter
instead of allowing the pager to parse session files. JSONL handling explicitly
documents torn-tail recovery and crash-atomic rewrite behavior in
`storage/jsonl/mod.rs:409-461` and `625-713`. The pager also has a separate
active-session crash registry dependency in its manifest.

Garive rejects copying this persistence model into `tui/`. Runtime already owns
durable conversation truth. Only bounded UI preferences and an exact pending
command envelope may be local; both use atomic replacement and fail-clean
validation.

### Rendering and tests

The pager separates `xai-grok-pager-render`, markdown rendering, terminal
capability detection, input normalization, and PTY harnesses. Its manifest
declares Ratatui, Crossterm event streams, bracketed paste, Unicode width and
segmentation, snapshots, and a PTY harness. The source contains focused render,
input, dispatch, terminal, resize, session, and reconnect tests.

Garive adopts separate model/reducer, view, terminal, and transport modules;
semantic render snapshots; Unicode-width-aware wrapping; and a PTY launch gate.
It does not adopt image, voice, Mermaid, game, worktree, or foreign-session
features because Garive has no admitted Host contracts for them.

`app/status_blocks.rs` keeps queue, task, and usage presentation as pure,
unit-tested formatting outside dispatch. Rows use stable status-first columns,
short public descriptions, explicit empty states, and grouped summaries.
Garive adopts that separation for public activity/status presentation, but not
Grok-specific task, workflow, usage, or Agent data.

### Prompt-adjacent command discovery

`views/prompt_widget/mod.rs:1068-1307` makes `refresh_slash` the sole update
path for a typed slash snapshot, keeps the registry/controller separate from
the immutable view state, wraps keyboard movement, clamps wheel movement, and
accepts completion by replacing the controller-owned text range. Tests at
`views/prompt_widget/tests.rs:1423-1764` cover bare-slash activation, ordinary
prose, live registry changes, availability gates, argument separators,
embedded paste elements, completion ranges, and selection invalidation.
`app/agent_view/mod.rs:1241` and `views/slash_dropdown.rs:122+` make the
rendered prompt-anchored row geometry the source of mouse hit testing.

Garive adopts the component and ownership pattern, not Grok's commands,
previews, MRU ranking, or ACP/tool registry. Garive's smaller catalog is
static, exact-prefix filtered, and bounded to five visible rows.

The same pinned tree's
`crates/codegen/xai-ratatui-textarea/src/editor.rs:44-105` gives every
directional movement an explicit selection-collapse edge. Tests at
`textarea_tests.rs:6437-6615` distinguish plain grapheme arrows, which stop at
the chosen edge, from word/vertical/Home/End movements, which continue from
that edge. Garive adopts this interaction invariant in its smaller editor
model, not Grok's textarea implementation, keymap, kill ring, or mouse grammar.

For mouse selection, the pinned Grok textarea directly implements
Down/Drag/Up routing in
`crates/codegen/xai-ratatui-textarea/src/textarea.rs:1157-1375`, and
its tests at `textarea_tests.rs:4056-4198` bind cursor placement, drag anchors,
selection persistence, and replacement-ready ranges. Grok's
`crates/codegen/xai-grok-pager/src/app/agent_view/selection.rs:376-410`
separately preserves prompt ownership for Drag/Up after the pointer leaves the
prompt. At their pinned revisions, Codex explicitly drops mouse events in
`codex-rs/tui/src/tui/event_stream.rs:189-250`, while
Pi's editor has no mouse handler (its `stdin-buffer.ts:102-110` only frames SGR
sequences). Garive therefore authors its own smaller grapheme/cell hit test and
focus-cancel lifecycle; it adopts only the directly observed down/drag/up
ownership invariant, not Grok's multi-click, clipboard, element, or accelerated
edge-scroll behavior.

## Codex findings

### Event and terminal ownership

`codex-rs/tui/src/tui.rs:549-568` separates terminal events from the
application. Lines 220 and 305 enable and disable bracketed paste, lines
540-546 restore on panic, and lines 675-680 suspend and resume the input event
stream when another process needs stdin. Alternate-screen transitions are
explicit at lines 784 and 809.

`codex-rs/tui/src/app.rs:1305-1401` routes input/paste/draw events and keeps
rendering behind the application state. Resize reflow and terminal history are
separate modules rather than incidental widget behavior.

Garive adopts typed terminal events, explicit ownership of stdin, redraw
coalescing, and terminal restoration. External-editor and subprocess stdin
handoff remain outside the first admitted contract.

### Input model and slash discovery

`codex-rs/tui/src/bottom_pane/chat_composer.rs` and
`bottom_pane/paste_burst.rs` document independent composer and paste state
machines. The nested `AGENTS.md` requires the documentation and implementation
to stay synchronized. Snapshot coverage under `bottom_pane/snapshots/` includes
empty, narrow, multiline, paste, history, command, footer, and permission
states.

`bottom_pane/command_popup.rs:36-202` owns filtered rows, selected index,
scroll window, and bounded wrapped height. `chat_composer/slash_input.rs:171-312`
limits the popup to the first command token and owns completion/cancel keys.
`chat_composer.rs:3830-3941` refreshes after edits, suppresses conflicting
popups, and keeps an explicitly dismissed token closed until that token
changes. Its tests at `chat_composer.rs:12000+` prove bare slash, valid prefix,
space/invalid-prefix closure, and history isolation.

Garive adopts grapheme-aware cursor movement, bracketed paste as one edit,
multiline drafts, bounded history, mode-specific key ownership, and snapshot
coverage. It also adopts prompt-adjacent prefix discovery with explicit
dismissal, while retaining a separate searchable command palette. Garive does
not adopt Codex's command set, attachments, shell mode, model selection, or
approval shapes without corresponding Host authority.

Direct inspection of `bottom_pane/textarea.rs` and its `textarea/` modules at
the pinned revision found cursor, wrapping, paste, undo, and Vim behavior but
no range-selection state or selected-text renderer. Pi's pinned
`packages/tui/src/components/editor.ts` likewise declares only lines and cursor
position in its editor state. Garive's Shift-selection model and visible
grapheme-safe selection are therefore a Garive-authored requirement, not a
behavior inferred from either product or copied from those sources.

Codex's pinned `bottom_pane/textarea.rs:436-489,1883-1960` derives height,
cursor position, viewport state, and rendering from cached grapheme-safe
wrapped ranges; `bottom_pane/textarea/wrapping.rs` owns those ranges. Pi's
pinned `packages/tui/src/components/editor.ts:106-177,469,909` independently
uses one `wordWrapLine` result for grapheme-aware word/hard wrapping and records
the same content width for navigation. These are direct source facts. Garive
adopts the invariant that painting and cursor geometry share one wrap model,
but authors its own Rust `EditorLayout`, safety-marker measurement, selection
spans, exact-width continuation, and tests; it does not copy either data model
or implementation.

The same Codex ranges make desired height depend on wrapped lines rather than
logical newlines. Garive now adopts that directly observed invariant through
its own bounded `EditorLayout::desired_height`; the `3..=7` frame policy and
tiny-height fallback are Garive product decisions.

Codex `bottom_pane/textarea.rs:1229-1340` also uses its cached wrapped ranges
for Up/Down, retains a preferred display-width column, and clamps at visual-row
boundaries. Pi `packages/tui/src/components/editor.ts:1319-1375` independently
moves between precomputed visual lines with a preferred visual column and
grapheme-safe segment correction. These are direct observations at the pinned
revisions. Garive authors a different boundary: its private `EditorLayout`
computes targets at the current composer width, while `EditorState` applies
grapheme-indexed cursor and selection intent. No reference data structure or
code is copied.

Home/End is an explicit Garive product divergence. Codex
`bottom_pane/textarea.rs:502-527,1371-1391` resolves beginning/end against
newline-delimited logical lines. Pi
`packages/tui/src/components/editor.ts:747-760,1461-1470` does the same against
its current logical line. Garive instead applies its normative visual-line
contract through the shared layout, while preserving `Ctrl+Home/End` for
document boundaries. This behavior is Garive-authored to keep navigation and
painted wraps consistent; it is not attributed to either reference product.

Pi directly couples history eligibility to visual boundaries at pinned
`packages/tui/src/components/editor.ts:393-435,804-825`: Up recalls only from
the first visual row, Down returns toward the draft only while browsing from
the last visual row, entering browse mode clones the draft state, and leaving
past the newest restores it. Codex independently documents a render-decoupled
shell-style state machine at pinned
`bottom_pane/chat_composer_history.rs:1-17,122-152,358-435`; ordinary Up/Down
and `Ctrl+R` search are distinct, and multiline boundary gating protects normal
cursor movement. Its async response path and rehydration boundary are explicit
at `bottom_pane/chat_composer.rs:1029-1067,1616-1624,1683-1708`.

Garive adopts the directly observed separation and boundary invariant, not
either implementation. Its durable `prompt_history` projection, transient
`PromptHistoryBrowser`, and grapheme-indexed `EditorState` are three separate
owners. The saved draft includes its original grapheme cursor, while the shared
Garive `EditorLayout` remains the sole authority for responsive visual rows.
This exact data ownership and cursor-restoration contract is Garive-authored.

### Pi corroboration

At Pi revision `11b5403fade1`, `packages/tui/src/components/editor.ts:276-365`
owns autocomplete state, list, provider, cancellation, and a configurable
three-to-twenty-row bound with five rows by default. Lines 519-710 render the
list adjacent to the editor and give it first keyboard ownership; lines
1102-1126 trigger `/` only at a line start and refresh on continued typing.
`packages/tui/src/autocomplete.ts:272-427` composes slash and path providers
while keeping slash-context matching explicit. Pi additionally suppresses
autocomplete mutation during paste at `editor.ts:1194`; Garive instead
re-evaluates an atomic paste after insertion because its catalog is synchronous
and bounded. This is corroboration, not a copied implementation.

### Footer and visual hierarchy

`codex-rs/tui/src/bottom_pane/footer.rs` explicitly separates pure footer
rendering from `FooterMode` selection and higher-level quit/interrupt policy.
Its documented single-line fallback keeps the most actionable instruction,
shortens it before removal, and drops ambient context by width. Snapshots under
`bottom_pane/snapshots/` verify status/composer fill, shortcut modes, and active
Agent labels.

Garive adopts context-owned footer modes, separately styled key hints, and
deterministic width collapse. It adds a persistent full-height Session rail at
standard widths, a capped main reading column, semantic connection/execution
chips, modal background dimming, and reverse-video monochrome selection. Those
choices are Garive-authored against its Host contracts; they are not copied
product decoration.

### Markdown presentation

Codex `codex-rs/tui/src/markdown_render.rs:430-567` keeps inline styles in a
stack, pairs every emphasis/strong/strikethrough/link start with a pop, records
fenced-code language, and gives tables their own structured pipeline. Its
`markdown_render_tests.rs:717-768` directly proves nested strong/emphasis and
visible Web-link destinations. Its `render_table_lines` and width-allocation
path at `markdown_render.rs:1085-1670`, plus grid/record fallback tests at
`markdown_render_tests.rs:1674-1792`, establish that a table needs a semantic
model and an explicit narrow layout rather than delimiter text. Grok Build
independently separates Markdown
style, parsing, hyperlinks, code-block metadata, streaming, and syntax in
`xai-grok-markdown/src/{style,parse,hyperlinks,output,streaming,syntax}.rs`;
its `buffers.rs:124+`, `parse.rs:1526+`, and `render.rs:1310+` directly model,
constrain, and test table width.

Grok's `xai-grok-markdown/src/syntax.rs` owns `SyntaxSet`, theme, language-token
lookup, and stateful `HighlightLines`; an unknown language or highlighting
error returns plain code. Its pager-side `xai-grok-pager-render/src/syntax.rs`
maps chromatic source colors to terminal-safe semantic ANSI roles instead of
forwarding arbitrary theme colors. Pi independently exposes highlighting as an
optional Markdown theme capability in `packages/tui/src/components/markdown.ts`
and validates the fenced language before calling highlight.js in
`packages/coding-agent/src/utils/syntax-highlight.ts`; it explicitly rejects
automatic language detection because prose is misclassified, and catches
errors as plain code. The audited Codex Markdown renderer records fenced
languages but provides no equivalent syntax-coloring path. These are distinct
findings, not an inferred shared implementation.

Garive adopts the bounded component boundaries that its transcript needs:
compositional inline styles, visible sanitized link destinations, ordered-list
indices, semantic fenced-code frames/language labels, stateful Syntect parsing
for recognized labels, terminal-palette token mapping, and width-aware
grapheme-safe code clipping. Unlabeled/unknown code stays plain; a 16 KiB line
or 64 KiB block budget disables highlighting for the remainder of that block
without changing its text. Garive's own `markdown_table.rs` bounds the model
to 12 columns, 64 body rows, and 4,096 characters per cell; preserves styled
spans and CommonMark alignment; allocates content-aware Unicode display widths;
and deterministically transposes an undersized grid into labeled records. It
does not copy either renderer, emit active OSC 8 links, auto-detect languages,
or admit Grok's extended language bundle.

### Motion ownership

`codex-rs/tui/src/motion.rs` centralizes time-varying indicators and requires
an explicit reduced-motion fallback. Its source-level test rejects direct
spinner or shimmer calls outside that boundary. `tui/frame_requester.rs`
coalesces requested frames behind a rate limiter instead of allowing widgets
to redraw independently. Grok's `notifications/title.rs` separately advances a
bounded frame index at a divisor of the event-loop tick and suppresses
unchanged terminal-title writes.

Garive adopts centralized pure motion presentation, explicit static fallbacks,
and active-state-only redraw scheduling. It does not copy either product's
frames, shimmer, title contents, or task semantics; Garive's calm pulse is
derived only from its typed connection and execution states, while terminal
titles remain semantic and nonanimated.

### Session resume and protocol boundary

Codex routes session list/read/resume through its app-server session boundary;
`app.rs:893-1022` constructs fresh, resumed, or forked UI state from typed
responses. `app/session_lifecycle.rs`, `history_pagination.rs`, and
`thread_session_state.rs` isolate their ownership. The TUI does not reconstruct
the authoritative session database from display cells.

Garive adopts the same boundary rule using H2. TUI transcript cells are derived
views and can be rebuilt from Host timeline plus later H1 events.

## Claude Code findings

The official `anthropics/claude-code` repository documents the product but does
not expose the shipping TUI implementation. The installed `2.1.231` executable
is a bundled artifact. The third-party `2.1.88` extraction is incomplete and
legally restricted, so all findings below are corroborating observations.

| Observation | Study path | Garive decision |
|---|---|---|
| Prompt history is separate from Session transcript and uses a locked, mode-`0600` append file. | `src/history.ts:114-133`, `299-321` | Keep optional local draft/history separate from Host truth; require owner-only permissions where supported. |
| Large pasted content uses references rather than blindly growing history entries. | `src/history.ts:23-44`, `228-277` | Enforce byte bounds and do not persist oversized pastes in TUI preferences. |
| Resume restores several state domains explicitly rather than treating rendered messages as the whole Session. | `src/utils/sessionRestore.ts:96-241`, `403-545` | Rebuild only admitted public TUI state; Runtime remains authoritative for all other domains. |
| Session transcripts are append-oriented and have dedicated resume filtering and torn-data handling. | `src/utils/sessionStorage.ts:204-257`, `1177-1253`, `2289-2322` | Do not imitate the private transcript format; consume H2 verified projections. |
| Input handling distinguishes typed input from multi-character paste. | `src/ink/hooks/use-input.ts:21-42` | Preserve bracketed paste boundaries and validate the final UTF-8 byte size. |

No Claude Code source is copied. A behavior is admitted only when Garive or an
Apache implementation independently supports the same conclusion.

## Windows persistence primary-source audit

The Windows backend was designed from the Win32 contract rather than inferred
from Unix modes or Rust API names:

| Official source | Confirmed fact | Garive decision |
|---|---|---|
| [File Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights) | A file/directory can receive a security descriptor at creation; a null descriptor inherits its parent's ACL. | Every Garive private object receives an explicit descriptor; parent inheritance is not trusted. |
| [Security Descriptor String Format](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-descriptor-string-format) and [ConvertStringSecurityDescriptorToSecurityDescriptorW](https://learn.microsoft.com/en-us/windows/win32/api/sddl/nf-sddl-convertstringsecuritydescriptortosecuritydescriptorw) | `D:P` denotes a protected DACL; conversion returns a self-relative descriptor released with `LocalFree`. | Build one canonical protected DACL for the current token SID and own the returned allocation with a drop guard. |
| [GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo) | A handle opened with `READ_CONTROL` can return owner/DACL pointers within one descriptor; the descriptor is released with `LocalFree`. | Validate owner and exact DACL from the already-open object handle, not from display metadata. |
| [CreateFile symbolic-link behavior](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew) | `FILE_FLAG_OPEN_REPARSE_POINT` opens the link object rather than silently following it; directory handles require `FILE_FLAG_BACKUP_SEMANTICS`. | Reject reparse-point targets and ancestors before admitting state. |
| [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) | `MOVEFILE_REPLACE_EXISTING` replaces a file subject to ACL checks; `MOVEFILE_WRITE_THROUGH` waits for the move to reach disk. | Use a flushed, ACL-validated same-directory temporary file and a write-through replacement. |

The backend does not map `0700`/`0600` to a broad Builtin Users or
Authenticated Users ACE, does not repair a hostile existing ACL, and does not
follow junctions for convenience. Administrator ownership privileges remain an
operating-system authority and are not represented as ordinary Garive access.

## Cross-source decisions

| Concern | Accepted decision | Evidence strength |
|---|---|---|
| Architecture | Pure reducer plus typed effects/results; I/O event loop remains thin. | Garive accepted application model + Grok source + Codex source |
| Truth | Host/Ledger owns Sessions, Turns, terminals, suspension, and timeline. | Garive normative Specs |
| Local persistence | Versioned bounded preferences plus separate pending-command envelope only. | Garive A-UX1 + Grok/Codex boundary practice |
| Terminal safety | RAII-style idempotent setup/restore, panic restore, signal-aware shutdown. | Grok and Codex source |
| Input | Grapheme-aware multiline editor, bracketed paste, explicit key ownership, bounded bytes. | Grok, Codex, and Claude corroboration |
| Async fairness | Cancellation and terminal teardown outrank traffic; redraw is coalesced; no blocking Host call on render/input paths. | Grok event-loop source + Garive command semantics |
| Resume | Load a verified Host snapshot, then follow after its watermark; ignore replay duplicates. | Garive H1/H2 normative Specs |
| Testing | Pure reducer matrices, render snapshots, hostile input, transport contracts, real Runtime loopback, and PTY launch/restore. | Garive testing constitution + both Apache sources |

The 2026-08-31 macOS syntax candidate rendered 64 labeled Rust blocks / 384
code lines at 100 cells with p95 111,839 µs in debug and 12,380 µs in release,
against a 150,000 µs debug gate. Its optimized executable was 10,205,176 bytes;
the previously pinned pre-syntax executable was 7,498,968 bytes. This size cost
is explicit and buys only Syntect's bundled common grammar set; Garive does not
include Grok's larger extended-language bundle.

## Rejected transfers

- Client-owned Agent execution, provider selection, credential lookup, or
  transcript truth.
- Product-specific commands that lack a Garive Host contract.
- Persisting raw Host responses, hidden reasoning, tool arguments, credentials,
  or provider values in TUI files or logs.
- Inferring success from connection close, spinner completion, process exit, or
  missing output.
- Copying Claude Code implementation or wording from the restricted extraction.
- A single unbounded application enum or module; Garive keeps bounded modules
  and explicit public APIs.

## See also

- [`../spec/design/live-host-clients.md`](../spec/design/live-host-clients.md) — current Rust client semantics.
- [`../spec/design/host-api-v1.md`](../spec/design/host-api-v1.md) — durable command and event truth.
- [`../spec/design/host-read-model-v1.md`](../spec/design/host-read-model-v1.md) — restart-safe navigation and timeline.
- [`../spec/design/client-product-experience.md`](../spec/design/client-product-experience.md) — shared product state-machine precedent.
- [`../tui/README.md`](../tui/README.md) — current executable surface.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: active
