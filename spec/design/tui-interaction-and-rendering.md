# TUI interaction and rendering

> This Spec defines the Garive TUI information architecture, responsive
> layouts, editor behavior, commands, key ownership, conversation rendering,
> accessibility, and terminal-safe visual language.

## Audience

Engineers implementing `tui/src/input/` and `tui/src/view/`, plus reviewers of
terminal snapshots and PTY behavior.

## Why

A resident event loop is not a product experience. The terminal client must
make durable work legible, keep high-frequency actions fast, handle narrow and
Unicode-heavy terminals, expose recovery without false claims, and remain
operable without color, mouse, or an alternate screen.

## Product surface

The TUI ships these first-class workflows:

1. discover installed Agent definitions;
2. list, filter, create, select, and reopen durable Sessions;
3. load older conversation timeline pages;
4. compose and submit multiple Turns in one Session;
5. observe real progressive Agent output and typed activity state;
6. request cancellation and wait for durable truth;
7. answer an admitted suspension using its public schema;
8. recover from disconnect and exactly retry an unknown mutation;
9. preserve bounded drafts, preferences, prompt history, and pending command;
10. inspect help, connection, command, and safe error details.

Features without a Host contract are not simulated. Session rename/delete,
attachments, image/media display, voice, model/provider configuration,
worktrees, filesystem diffs, tool argument display, hidden reasoning, and
remote authentication remain unavailable until their owning slices exist.

## Information architecture

```text
› Explain the recovery path.
  ✓ Read runtime state · +2 completed
• The Runtime commits the durable result while the live tail remains
  replaceable.▍

• Writing… (esc to interrupt)
› Ask a follow-up
```

The frame has four semantic regions: an exceptional-only `ContextLine`, the
conversation transcript, the persistent `Composer`, and an optional one-row
`HintLine`. In a healthy conversation the ContextLine has zero height; Session
identity is available through the terminal title and switcher instead of a
permanent toolbar. Transcript and Composer share the terminal's left axis.
A permanent navigation rail, bordered toolbar, conversation frame, centered
application island, and second status bar are prohibited. Session navigation,
commands, recovery, and decisions use bounded overlays. At wide sizes an
explicitly opened `Inspector` may share the work surface without changing
transcript truth.

Selection surfaces that originate from composition remain spatially attached
to the Composer. The command palette docks to the transcript's lower edge,
never covers a Composer that can be preserved, grows only with visible content,
and shows at most eight command rows per window. Filtering to one result must
shrink the surface instead of retaining an empty modal canvas. Garive may add
truthful unavailable-reason detail beyond the Codex baseline, but cannot buy
that detail with an unbounded catalogue or a second navigation axis. Only an
extreme-height fallback whose transcript cannot fit the minimum surface may
temporarily use the whole terminal.

The transcript composes `TurnBlock` values. A Turn owns one restrained User
request, its `ActivityStack`, one `LiveAnswer` or committed `MarkdownAnswer`,
and its terminal outcome. Durable, ephemeral, and local values remain separate
presentation types; a renderer never infers one from another. User content has
a role marker but no full-width card. Agent prose uses the terminal background.
Only the Composer, modal boundaries, and an explicitly opened Inspector keep
frames.

### Responsive modes

| Width | Mode | Regions |
|---|---|---|
| `<40` | minimum | safe minimum-size view; no raw IDs or content echo; the draft remains in memory |
| `40..=51` | linear | full-width transcript, collapsed activity summary, and at most one hint |
| `52..=79` | compact | full-width transcript with compact metadata; all secondary surfaces are overlays |
| `80..=128` | standard | full-width workbench with a shared transcript/Composer axis; Session and Inspector surfaces are overlays |
| `>=129` | wide | full-width workbench; opening Inspector explicitly reserves a 96-column transcript and one-cell gap |

Height below eight rows shows a safe minimum-size view. Height pressure removes
ambient context, secondary hints, and collapsed history before reducing the
Composer below two content rows or hiding an active decision. No layout
calculation may underflow or panic for any `u16` terminal size.

Markdown prose may impose an internal reading measure without moving the shell
axis; code, tables, Composer, overlays, and the detached-follow cue can use the
available terminal width. Extra width must not turn the entire TUI into a
centered narrow island.

## Focus and overlay model

```text
FocusTarget = Conversation | Composer | Inspector | Overlay

Overlay =
  CommandPalette
  | Help
  | SessionSwitcher
  | PromptHistory
  | Inspector
  | Suspension
  | UnknownCommand
  | ErrorDetails
  | QuitConfirmation
```

Only the top overlay owns input. Opening an overlay records the prior focus;
closing restores it when the target still exists. A blocking suspension or
unknown-command overlay cannot be dismissed in a way that silently discards
authority. It offers explicit defer, exact retry, or abandonment actions
allowed by the state.
Input ownership is also a presentation invariant. While any overlay is open,
the background `HintLine` is absent and the running-Turn rail never advertises
`Esc` cancellation. The rail retains only its passive phase label. The overlay
alone names the currently executable `Esc`, `Enter`, or decision action. Closing
the overlay restores the background cue without changing execution state.
While a Turn runs, this Composer-adjacent rail is the single lifecycle voice:
`Preparing…`, `Writing…`, `Finishing…`, `Saving…`, or an honest unavailability
state. Before the first presented text delta, `LiveAnswer` contributes no
transcript row. Once text exists, the Agent `•` shares its first Markdown line;
phase copy is never repeated above that answer and the interrupt binding never
floats as a separate row.
After cancellation is admitted, the same Turn-control component is the only
status voice adjacent to the retained draft. It replaces generic
frozen/running copy with `Cancelling…`; exact Host acceptance changes it to
`Stopping…`. Neither phase advertises another cancel action or repeats a
second durable-truth hint. Unknown outcome is the passive
`Cancel status unknown` behind its recovery overlay. A frozen draft is shown
from its beginning at compact heights instead of following its old caret to a
meaningless suffix. Fullscreen, minimum-width and linear presenters consume
this same semantic projection.
An unknown command bound to a Session becomes an actionable overlay only after
that exact Session is selected. Startup may announce that recovery exists, but
must not expose `Enter` before the controller can acquire the same pending
owner. A Session-less create recovery remains actionable after catalogue
refresh without inventing a Session context.
For action overlays, key identity, visual keycap, spoken key name, action label,
and semantic controller intent are one application-owned binding. Fullscreen
and linear presenters may format that binding differently but cannot invent an
action or alternate consequence. Explicit newlines in safe status text become
real layout rows, and popup geometry reserves the wrapped body plus every
action row.
Mouse events obey the same ownership. Wheel and click events inside a
selectable overlay move or activate only its rendered rows; events outside the
popup are consumed without scrolling the conversation or activating content
behind it.

Focus is visible without relying on color. The Composer retains one stable
frame and uses its caret, border token, and text marker to expose focus without
changing geometry. Inspector and overlay selections use a textual marker plus
terminal-native reverse style in monochrome. A modal dims the workspace,
preserves the rows above and below it as visible context, clears a same-height
full-width quiet band so clipped background fragments cannot compete for
focus, and gives its active row a background or terminal-native reverse style.
Background Host events never steal focus. A newly committed suspension
opens its prompt only when its Session is selected; otherwise SessionSwitcher
marks the Session as action-required and the terminal bell follows preference.

## Input normalization

Input handling converts terminal-specific events into semantic values before
the reducer sees them:

```text
InputEvent = Key(KeyChord) | Paste(String) | Mouse(MouseIntent)
             | Resize(Size) | FocusChanged(bool)

KeyChord { code, modifiers, kind }
MouseIntent = ActivateRegion | SelectRow | ScrollLines | ScrollPage
```

Only press and repeat key kinds edit text. Release events never submit or
activate actions. Bracketed paste is one atomic editor transaction. Carriage
returns normalize to line feeds. Invalid UTF-8 is rejected before it enters the
model.

If a terminal cannot distinguish `Shift+Enter`, `Ctrl+J` remains the portable
newline binding. Help and `HintLine` show only bindings supported by the
detected terminal capability set.

## Composer editor

The editor is authored in Garive rather than using `tui-textarea 0.7.0`, whose
published dependency is Ratatui `0.29` while this slice selects Ratatui
`0.30.2`. Its model is independent of Ratatui widgets.

```text
EditorState {
  text,
  cursor_grapheme,
  selection_anchor?,
  preferred_display_column?,
  undo: bounded stack<EditTransaction>,
  redo: bounded stack<EditTransaction>,
  viewport_line,
  paste_state,
}
```

Cursor movement and deletion operate on extended grapheme clusters. Vertical
movement preserves display column using Unicode width, not UTF-8 byte or scalar
count. The cursor never lands inside a grapheme. Selection ranges use grapheme
boundaries and convert to byte ranges only at the final edit or presentation
boundary. The composer renderer styles complete graphemes inside that range;
CJK, emoji ZWJ, and combining sequences cannot be partially selected. The
selection remains explicit in monochrome through reverse video and stays
aligned while the composer wraps or scrolls.

Soft wrapping, selected-text painting, cursor placement, and viewport scroll
consume one immutable layout result. Each explicit newline starts a logical
line. Within it, wrapping prefers the last whitespace boundary that fits and
otherwise hard-wraps at an extended-grapheme boundary. Sanitization happens
before terminal-cell measurement, so a visible safety marker and its width
cannot disagree. A cursor exactly after a full-width row advances to column
zero of a continuation row and is scrolled into view by the same layout.
The ComposerDock requests height from that visual result: content plus one
status/separator row, clamped to `2..=6`. Terminals whose content area is below
12 rows hold the dock at two rows and scroll internally. Thus a long single-line
draft expands like an explicit multiline draft when space exists, without
stealing the highest-priority hint or minimum conversation surface.

Up and Down consume those same visual rows, not logical newline ranges. The
first vertical move records the cursor's terminal-cell column; subsequent
moves retain that column and clamp to the nearest grapheme insertion point on
shorter rows. A non-vertical edit or movement clears the preferred column.
Exact-width continuation rows and double-cell graphemes obey the same rule.
The controller requests a target from the composer at the actual responsive
inner width, while `EditorState` alone applies selection and cursor mutation.
Home and End use the same row membership. On a non-final wrapped row, End lands
at the last insertion point that remains visibly on that row rather than the
ambiguous start of the next row; an exact-width cursor continuation is its own
empty visual row. `Ctrl+Home` and `Ctrl+End` remain document operations.

Unmodified Up recalls the newest local prompt only when the cursor is already
on the first visual row, the shared layout returns no earlier row, and no
selection exists. Repeated Up walks toward older entries and stops at the
oldest. Down remains ordinary visual movement unless history browsing is
active; from the last visual row it walks toward newer entries and, after the
newest, restores the exact pre-browse draft text and grapheme cursor. Shifted
vertical movement never enters history. Any admitted text mutation, paste,
submission, Session replacement, or explicit composer clear exits browsing;
thereafter Down cannot resurrect the saved draft. `Ctrl+R` search is a separate
overlay and does not share this sequential browse cursor.

| Operation | Required behavior |
|---|---|
| Insert | one transaction per typed cluster; adjacent typing may coalesce within a bounded interval |
| Paste | normalize line endings; one transaction; validate resulting byte bound before commit |
| Backspace/Delete | remove one grapheme or selected range |
| Word move/delete | Unicode word boundaries with punctuation-preserving behavior |
| Undo/redo | bounded by operation count and total retained bytes; paste undoes atomically |
| Kill/yank | single private in-memory span; logical-line or selected range; cleared on Session change |
| Home/End | visual line start/end; `Ctrl+Home/End` document start/end |
| Up/Down | visual line movement; history only at document boundary with no selection |
| Selection | Shift movement; `Alt+C` or `/copy selection` copies only visible selected text |

With an active selection, an unmodified Left or Right collapses to the start
or end edge and stops without an extra grapheme move. Directional word,
vertical, line-edge, and document-edge movement first collapses to its matching
edge and then performs that movement. This rule is independent of which end is
the anchor and prevents backwards selections from producing asymmetric cursor
jumps.

With mouse capture enabled, left-button down inside the composer's text
viewport places the cursor and anchors a drag. Left drag extends the same
grapheme selection used by Shift movement; release commits the endpoint and
ends capture. Drag events remain composer-owned after the pointer leaves the
viewport and clamp to its nearest visible insertion point. A new press or
terminal focus loss cancels transient drag ownership. Modal and inline-command
hit regions retain higher priority, and the ComposerDock status/separator row
never places a cursor. Mouse coordinates are interpreted by the composer's shared wrapped
layout rather than by controller-owned row math.

A single left click places the grapheme cursor and may begin a drag. A second
left click on the identical terminal cell within 500 ms selects the complete
Unicode word-class or punctuation run under that cell; whitespace only places
the cursor. A third selects the complete newline-delimited logical line,
including its trailing newline when present. The fourth begins a new cycle.
Typing, paste, resize, focus loss, or any non-composer click cancels the cycle.
Word/line selection reuses the normal semantic selection style and replacement
path; it never copies implicitly or exposes hidden buffer content.
`Alt+C` is the direct explicit copy gesture while a composer selection exists.
It emits exactly that selected source range through the bounded clipboard
component, retains the selection, and shows a safe notice. `/copy selection`
is the equivalent command-palette action and is unavailable when no composer
selection exists. Typing the command into the composer necessarily replaces
the prior selection, so discovery for an existing selection is `Ctrl+P` or
`Alt+C`, not implicit copy during typing or multi-click.

The editor accepts newline, tab-as-spaces, and printable Unicode. C0/C1 control
characters other than newline/tab are rejected. Bidi isolate characters may be
retained in the request but render with a visible safety marker; bidi override
characters are rejected because visual order could conceal command content.

Empty or whitespace-only input cannot submit. `HintLine` remains absent for an
ordinary valid draft. Approaching the Host command-byte maximum adds one
bounded warning; crossing it leaves the draft editable, disables send, and
names the excess byte count without truncating. A permanent byte counter is
not part of the default surface.

While the selected Turn is running, the Composer remains an explicit retained-
draft editor, not a submit or queue affordance. Its placeholder names that
state. `Enter` keeps the exact draft and emits a visible `Current Turn is
running · draft retained` notice; it never fails silently and does not claim a
durable queue. With no overlay, the Composer status row owns `Agent running`
plus the `Esc` cancel control, or only that control when live output or an
active Activity already exposes work. With an overlay, it follows the input-
ownership rule above. Cancellation therefore cannot monopolize `HintLine`;
byte-limit, selection, suggestion, recovery, and notice feedback remain
available by their normal priority.

Drafts are per Session. Switching Sessions swaps editor state without losing
the bounded draft. On committed start, that Session's editor clears and its
submitted text enters prompt history. On unknown response, the draft remains
frozen behind the pending command and cannot be edited into a different retry.

## Global key map

| Chord | Context | Action |
|---|---|---|
| `Enter` | visible command suggestions | complete selected catalog row; the next Enter submits the completed command |
| `Tab` / `Shift+Tab`, `Up` / `Down` | visible command suggestions | complete, move backward, or move with wrapping |
| `Esc` | visible command suggestions | dismiss for the unchanged draft without deleting text |
| `Enter` | composer | submit when admitted; while a Turn runs retain the draft and show explicit feedback; accept selected modal item otherwise |
| `Ctrl+J` | composer | insert newline |
| `Shift+Enter` | composer, when distinguishable | insert newline |
| `Alt+C` | composer selection | copy exactly the selected composer text through bounded OSC 52 |
| `Ctrl+U` / `Ctrl+K` | composer | kill selection, or to logical line start/end, into the private one-entry buffer |
| `Ctrl+Y` | composer | yank the private buffer, replacing a selection as one edit |
| `Ctrl+Z` / `Alt+Z` | composer | undo / portable redo |
| `Ctrl+A` / `Ctrl+E` | composer | move to newline-delimited logical line start/end |
| `Ctrl+B` / `Ctrl+F` | composer | move one grapheme left/right |
| `Alt+B` / `Alt+F` | composer | move one Unicode word left/right |
| `Ctrl+H` / `Ctrl+D` | composer | delete one grapheme backward/forward |
| `Ctrl+W` / `Alt+D` | composer | delete one Unicode word backward/forward |
| `Tab` / `Shift+Tab` | no suggestion or blocking overlay | move focus forward/backward |
| `Up` / `Down`, `Home` / `End`, `Enter` | SessionSwitcher or Inspector | move the stable selection, jump to an edge, or activate the visible item |
| `Up` / `Down`, `PageUp` / `PageDown`, `Home` / `End` | focused conversation | scroll one cell, scroll one viewport, jump oldest, or follow latest |
| `Ctrl+P` | any ready view | open command palette |
| `Ctrl+R` | composer | open prompt-history search |
| `Ctrl+N` | ready | create Session using selected/default definition |
| `Ctrl+S` | ready | open SessionSwitcher |
| `Ctrl+L` | conversation | redraw current frame; does not erase durable history |
| `Esc` | overlay | close/defer when allowed |
| `Esc` | running selected Turn | request cancel after `HintLine` cue; no terminal claim |
| `Ctrl+C` | running selected Turn | request cancel |
| `Ctrl+C` | idle with draft/selection | clear selection, then draft on next press |
| `Ctrl+C` twice within 1500 ms | idle empty composer | open quit confirmation |
| `Ctrl+Q` | any nonblocking state | open quit confirmation |
| `PageUp/PageDown` | conversation | scroll one viewport |
| `Ctrl+Home/End` | conversation | oldest loaded / follow latest |
| `?` | empty composer | open help |

The key router resolves overlay, editor, focused region, then global bindings
in that order. A key is consumed by at most one owner. `HintLine` exposes only
the highest-priority currently resolved binding or recovery action and may be
absent; it is never a permanent shortcut legend. Visual help, the running-Turn
rail, and the linear screen-reader projection derive the same active owner: an
open Help overlay says `Esc close guide`, never `Esc cancel Turn`, and the
linear composer status says that the active overlay owns input.
Kill ranges deliberately use newline-delimited logical lines, independent of
the composer's visual Home/End contract. At a logical line boundary, `Ctrl+K`
may consume the following newline and `Ctrl+U` the preceding newline so lines
can be joined. A nonempty selection has priority for either kill chord. A
no-op kill preserves the prior private value; yank over a selection replaces
it as one undoable edit. Kill/yank never reads or writes the system clipboard.
`Alt+Z` is the portable redo chord because legacy xterm input cannot reliably
distinguish `Ctrl+Shift+Z` from `Ctrl+Z`.
Ctrl+A/E are intentionally logical-line operations, matching their terminal
editing meaning and the Ctrl+U/K kill boundaries. Home/End remain Garive's
visual-row operations, so a user can choose between source-line and painted-row
navigation without hidden mode state. The typed key catalog is the single
source for controller resolution plus visual and spoken Help; a chord present
in only one of those surfaces fails its catalog test.
Typing a printable character outside an overlay explicitly transfers focus to
the composer before inserting it. Editing and deletion keys never mutate a
draft while the conversation, Inspector, or overlay owns focus.
SessionSwitcher derives painting, keyboard visibility, and mouse hit-testing
from one visible-window calculation. A pointer event outside an actually
rendered Session row cannot activate a hidden Session.
Every selectable overlay derives rendering, highlight position, and activation
from one filtered result set and a terminal-height-aware visible window. Moving
selection cannot leave the active row clipped below the popup.

The prompt-adjacent command component owns only navigation, completion, and
dismissal keys while it is visible. Modal ownership remains higher. Printable
editing, deletion, undo/redo, cursor movement, and paste stay composer-owned;
each resulting draft synchronously recomputes the bounded prefix result set.
The screen-reader event loop never gives keys to this visual-only component;
`Ctrl+P` remains its complete linear discovery path.

## Slash commands

Slash commands parse only when the first non-whitespace character is `/` and
the first logical line contains the complete command. Unknown commands remain
ordinary draft text until submit; submit then opens a command error without
sending the text to Host.

| Command | Arguments | Behavior |
|---|---|---|
| `/new` | optional definition ID from installed list | create and select a Session |
| `/sessions` | optional filter text | open SessionSwitcher |
| `/jump` | optional filter text | search loaded Turns and jump to a public start position |
| `/inspect` | optional `activity`, `recovery`, `details`, or `close` | open, switch, or close Inspector |
| `/help` | none | open contextual help |
| `/status` | none | open Inspector's safe Details projection |
| `/retry` | none | replay exact pending mutation when available |
| `/reconnect` | none | start explicit reconnect from saved cursor |
| `/cancel` | none | request cancellation of selected running Turn |
| `/theme` | `system`, `dark`, `light`, or `mono` | update local preference |
| `/mouse` | `auto`, `on`, or `off` | change mouse capture safely now and persist it |
| `/copy` | `last`, `selection`, or `session-id` | copy visible completion, active composer selection, or opaque ID |
| `/quit` | none | open quit confirmation |

Command names are ASCII lowercase and exact. Arguments support quoted UTF-8
with backslash escapes for quote and backslash only. Unknown options, missing
arguments, extra arguments, invalid escapes, or more than the command byte
bound produce a local validation error. Commands never fall back to Host text
after parse failure.

The command palette uses one typed registry for input text, help, and
availability requirements. Visual rendering, screen-reader output, and Enter
activation derive the same safe unavailable reason from the current model;
they cannot maintain separate predicates. Disabled commands remain
discoverable with that reason.

`/inspect` without an argument chooses Recovery when an actionable or degraded
safe recovery entry exists, otherwise Activity when public activity exists,
otherwise Details. Only the open/closed bit is persisted. Variant, selection,
and visible window are transient; selection is retained by stable internal
entry identity across resize. At `>=129` cells an open Inspector is a bordered
32-column region, including its border, beside a transcript whose measure is
at most 96 cells. At `40..=128` it is the top-level bounded overlay. Below 40
cells it remains open in model state but the minimum-size view takes priority.

Activity contains only H3 public labels and states. Recovery contains only
typed connection, pending-command, suspension, and execution consequences.
Details contains safe product labels, connection/execution wording, loaded
Turn count, and follow state. No variant renders opaque IDs, durable positions,
paths, tool arguments, provider payloads, hidden reasoning, or local filenames.
Activity activation navigates to the owning loaded Turn. Recovery activation
may open an already-admitted recovery or suspension action; Details activation
is inert. Keyboard, pointer, fullscreen, and screen-reader presentations
consume the same ordered `InspectorEntry` projection, visible window, and
typed activation. `Tab`/`Shift+Tab` retain their global focus-cycle meaning;
arrows and `Home`/`End` move selection, `Enter` activates, and `Esc` closes and
restores prior page focus. Variants change only through typed `/inspect`
commands. Visual entries use content-driven height: a repeated state already
present in the label is not emitted again as an empty two-row slot, while an
independent safe detail receives its own muted row. One row-layout component
owns the visible window and pointer cells, so compact and wide modes cannot
select a row the user did not see.

When a focused, single-line composer draft begins literally with `/`, is at
most 128 bytes, and is an exact case-folded prefix of a catalog input, a
nonmodal command menu appears directly above the composer. Leading whitespace
continues to parse on submit for compatibility but does not trigger discovery.
The inline menu searches command input only; `Ctrl+P` separately searches both
input and help text. Arguments and unmatched prose close the inline menu.

The menu exposes at most five rows from catalog order and keeps its selected
row visible. `Up`, `Down`, and `Shift+Tab` wrap. `Tab`, `Enter`, or a left click
copies the selected canonical input into the composer; `/new` and `/sessions`
also receive one trailing argument separator. Completion does not execute:
the menu dismisses for that exact draft, and a subsequent Enter traverses the
normal parser and availability path. Escape preserves the draft and dismisses
only until the draft changes. Wheel/click hit testing derives from the same
rendered window; modal, unfocused, multiline, smaller-than-`30x12`, and linear
screen-reader states expose no inline menu.

Each row reserves the command input before allocating width to secondary help
or unavailable detail. Secondary text truncates with an explicit ellipsis at a
Unicode display-cell boundary and never splits a grapheme. Rendering and hit
testing consume one shared inner rectangle: the horizontal breathing room and
border cannot activate a row, while every painted result row can.

## External draft editor

`Ctrl+G` and `/edit-prompt` route to one typed action for an unfrozen composer
with no blocking overlay. Resolution prefers `VISUAL`, then `EDITOR`; unset,
empty, malformed, overlong, or over-argument values name a safe reason. Garive
never guesses an editor and never evaluates a shell.

The child receives the exact draft in a private Markdown file. Exit zero
normalizes CRLF, strips at most one editor-added final newline when the original
lacked one, and applies one undoable replacement. Empty output clears the
draft. Spawn, I/O, non-zero exit, invalid UTF-8, unsafe controls, byte overflow,
or a changed draft/Session keeps the newer original. Return focuses Composer,
resets history browsing, recomputes suggestions, and redraws without submitting.

## Session navigation

Session rows show a bounded display label derived from committed public text,
latest Turn state, last activity time, and action-required marker. Until a
public title contract exists, SessionSwitcher uses the public Agent definition
label and a neutral ordinal; it does not fabricate a repeated title, expose
prompt text, or put an opaque Session suffix on the default surface. State
always has a non-color glyph (`✓`, `●`, `!`, `×`, `■`, or `○`) and text.
Full opaque IDs remain hidden until an explicit details or copy action.

SessionSwitcher supports case-folded substring filtering over public label and
opaque ID, keyboard/mouse selection, and incremental H2 page loading. Results
remain in Host order. Filtering never changes durable order or execution
priority. Rendering, selection movement, and `Enter` activation consume the
same filtered result set. When results exceed the bounded popup, the visible
window follows the selection without changing its result-relative index.
Creating a Session requires choosing an installed definition when more than
one exists.

Selecting a Session restores its draft and viewport, loads a fresh H2 timeline,
and starts H1 follow after the snapshot watermark. A running background Session
keeps its follow task within the configured active-follow bound; older inactive
Sessions reconnect when selected.

## Conversation rendering

```text
TurnBlock {
  user,
  activity_stack,
  answer: LiveAnswer | MarkdownAnswer?,
  terminal_outcome?
}
```

Each Turn has a stable internal key `(session_id, turn_id)`. Its durable child
values retain their public positions for ordering and replay but the ordinary
transcript does not display positions or opaque IDs. Updates replace the
intended Turn child without rebuilding unrelated history. Committed answers
are cached by key, width, theme, and content digest. Ephemeral live answers use
a separate bounded cache described below. The model retains bounded public
values, never terminal escape bytes.

### Text and Markdown

Completion text supports a safe CommonMark presentation subset: paragraphs,
emphasis, headings, lists, block quotes, fenced/indented code, thematic rules,
tables, and inline code. Raw HTML is rendered as text. Images become labeled
links only when a future public URL contract admits them. No Markdown content
can emit terminal escapes.

Nested strong, emphasis, strike, heading, and link styles compose as a stack;
closing an inner span restores its enclosing style. Ordered lists preserve the
declared starting index. Explicit links render as underlined semantic label
plus a sanitized, 120-character-bounded destination when it differs from the
label. They are visible text, not an active OSC 8 escape.

Code blocks preserve indentation inside a semantic frame, show the first
bounded fenced-language token, expand tabs to four display cells, and use
grapheme-aware horizontal clipping with an explicit `…` overflow marker.
Recognized labels select a bundled grammar by extension or case-insensitive
name. Highlighting is stateful across lines so multiline strings/comments
remain coherent, but no label is auto-detected. Parser scopes map through the
Garive semantic palette; raw syntax-theme colors and backgrounds cannot reach
the terminal. Unknown labels, parser errors, lines above 16 KiB, or blocks above
64 KiB fall back to plain semantic code; crossing either budget disables
highlighting for the rest of that block. Copy operates on source text, not
clipped cells, language labels, token spans, or border glyphs.

Tables are parsed into bounded header, row, cell, alignment, and styled-span
state before presentation. At usable widths they render as a content-aware
grid; if the column count cannot retain a six-cell minimum after separators,
they transpose into `Header: Value` records instead of horizontally clipping
or starving prose columns. The component admits at most 12 columns, 64 body
rows, and 4,096 characters per cell. Overflow is explicit, Unicode display
width and grapheme boundaries govern wrapping, and inline emphasis/link styles
survive both layouts. Resize may switch layouts but cannot mutate copied source.

Long unbroken graphemes clip safely. Wide and combining characters use the
same display-width implementation as editor cursor placement. Tabs expand to
four columns for display without changing copied text.

The Markdown renderer, not the outer transcript widget, owns physical reflow.
It reserves the Agent/quote/code gutters before wrapping styled grapheme spans,
repeats those gutters on every physical row, and subtracts a list marker's
display width from continuation rows. The outer paragraph may retain defensive
wrapping but conforming output already fits its assigned width. Top-level major
blocks receive one blank row; simple list siblings do not. Incremental live
rendering must equal monolithic rendering for the same source, width, and theme.

### Progressive output

`LiveAnswer` renders only H4 values admitted by
[`host-live-output-v1.md`](host-live-output-v1.md). It is keyed by exact
Session, Turn, Execution, generation, stream, and sequence identity and never
enters the durable timeline or local persistence. It maintains received text,
presented text, a monotonic stable Markdown block prefix, a mutable final
block, safe phase, and preview availability.

A snapshot atomically replaces the complete ephemeral projection. A contiguous
text delta appends exactly once. Phase changes affect only safe status copy.
Gap, overflow, malformed input, or disconnect clears any untrusted suffix and
shows one quiet preview-unavailable state while durable execution continues.
An H1 terminal event or terminal H2 snapshot atomically removes matching live
state and installs the committed answer. Late or older-Execution live values
are ignored and never move the durable cursor.

The event loop requests a draw when received text advances and coalesces values
that arrive before the next terminal frame. Presented text reaches received
text within two available render frames; a burst catches up in one frame
instead of preserving cosmetic per-character delay. The renderer does not
invent characters or replay a completed answer. Before the first visible
delta, it renders the safe phase row with neither an empty answer row nor a
caret. A subtle live caret becomes visible only after active, available preview
text exists and is absent under reduced motion. The
stable Markdown prefix is not reparsed for each delta; only the mutable final
block may reflow until it becomes stable. Resize may reflow the entire preview.

This cadence is source-backed frame coalescing, not simulated token typing.
The competitive evidence and explicit Adopt/Adapt/Reject boundary are recorded
in
[`tui-competitive-evidence-2026-09-01.md`](../source-audit/tui-competitive-evidence-2026-09-01.md#adopt--adapt--reject-decisions).
Competitor line animation or headless `stream-json` behavior cannot weaken the
exact H4 identity, gap, convergence, or two-frame requirements above.

### Scrolling

The viewport anchors to the newest content while `follow_latest` is true.
Manual upward scroll disables follow and shows `N newer updates`; new events do
not jump the viewport. `End` or activating that badge returns to latest. The
badge is one component that owns its projection, semantic spans, centered cell
geometry, and pointer hit target. When an overlay owns input, the component
retains only passive detached/update status and removes both the `End` action
and its hit target; closing the overlay restores them without moving the anchor.

While detached, both durable events and visible live-frame advances increment
the unseen update count without forcing follow mode. The current live answer
may remain below the visible window; the application never scrolls to it until
the user explicitly resumes latest-follow. Conversation navigation remains
available through keyboard scrolling, TurnNavigator, and the optional
Inspector. The default transcript has no permanent position rail or hover
preview competing with prose.

Reflow on resize preserves the top visible stable cell and its source-line
offset where possible. Loading older pages preserves the current visible
anchor. Render work is bounded to the visible window plus an overscan margin;
the TUI does not lay out the complete Session on every frame.

### Turn navigator

`/jump [filter]` opens a modal `TurnNavigator` only when a Session is selected
and its complete loaded H2 snapshot contains at least two public Turn
landmarks. Otherwise the shared command registry supplies a truthful disabled
reason. The optional argument seeds the filter; no argument starts with an
empty filter.

The navigator consumes a separate public projection built directly from typed
`TurnTimelineItem` values. Each oldest-first `ConversationLandmark` contains:

- the Turn's public `started_position`, used only as the jump coordinate;
- a one-based ordinal across the complete loaded snapshot; and
- a sanitized, single-row, display-width-bounded preview of public user text.

It never parses a rendered cell `stable_key`, displays an opaque Turn ID, or
requests/persists additional data. Filtering is case-insensitive over the
public prompt preview. An empty result renders an explicit no-match state and
has no activatable row.

On open, follow-latest selects the final matching Turn. A detached viewport
selects the matching Turn whose start is closest at or before the current
public anchor position, falling back to the first result. `Up`, `Down`,
`Home`, and `End` move a clamped selection and keep it inside the shared
visible window. Selection and filter changes do not mutate the conversation
viewport. `Enter` resolves the selected `started_position` to its exact User
cell, anchors it at the top where possible, disables follow-latest unless it is
the final Turn, and closes. `Escape` closes without changing the viewport.

The overlay owns all keys while open. Mouse wheel changes selection and a left
click activates only a row returned by the same rendered-window geometry;
background conversation and Inspector routes cannot observe those events.
Session selection, timeline replacement, terminal focus loss, and quit clear
the overlay/filter/selection. Resize recomputes only the visible window and keeps
the same public position selected when it still matches. Linear screen-reader
mode exposes the same filtered ordinal/preview rows and exact activation, with
no cursor-addressed popup.

### Activity and suspension

H3 public activities project into the owning Turn's `ActivityStack`. The active
safe activity occupies one row. The latest completed safe label remains
legible; older completed siblings collapse into a supplemental count and
expand only through an explicit Inspector or
transcript action. Unknown activity kinds use `Activity updated` and cannot
mutate Turn state. Default activity copy omits durable positions. Tool
arguments, raw paths, hidden reasoning, provider values, and internal facts are
absent.
Presentation consumes the admitted H3 `label_key` and state rather than
discarding them into one generic phrase. The currently admitted
`agent.activity.read_file` lifecycle renders `Reading file` while running and
`Read file` after completion. Unknown keys remain a generic safe action; the
client never guesses tool semantics from payload text.

An admitted H2 suspension overlay renders only the public title/message and
response schema. The current native controls cover strictly admitted strings,
booleans, enums, and bounded numbers; any unimplemented keyword, mixed type,
object, or array degrades to explicit read-only status instead of accepting an
unvalidated response. The response editor is separate from the Composer and is
retained only while Session, Turn, suspension identity, and schema digest all
match. Submission uses the exact H1 text or canonical JSON continuation variant
and binds the displayed schema digest.

Decision overlays project one typed specification into the visual popup,
linear screen-reader text, key bindings, and controller intents. A shared
display-cell row layout owns word/grapheme wrapping, selected choice rows, and
per-action pointer rectangles. On short terminals it may cover the Composer
visually while preserving its draft, but it must retain the title, active
control, primary action, and safe-leave action. If compact height cannot retain
every choice, the selected row must expose its one-based ordinal, total choice
count, and `↑↓` navigation; showing one choice without disclosing the remaining
selection model is forbidden. Linear presentation names the same choices,
selection, and Up/Down navigation. Unknown-result abandonment
requires a second confirmation and never implies that the durable outcome is
known. `Ctrl+Q` opens a reversible safe-quit confirmation; `Escape` returns to
the suspension only while that exact suspension still exists.

## Status and notification language

| State | Visible wording |
|---|---|
| submitting | `Committing turn…` |
| following without a visible active transcript row | `Working…` |
| live preview unavailable | `Live feedback unavailable` |
| cancel request pending | `Cancelling…` |
| cancel accepted; terminal pending | `Stopping…` |
| cancel outcome unknown | `Cancel status unknown` |
| disconnected | `Disconnected; Turn state unknown` |
| reconnecting | `Reconnecting (2/5)…` |
| unknown command | `Command result unknown; exact retry available` |
| suspended | `Action required` |
| completed | `Completed` |
| stopped | `Stopped` |
| failed | `Failed · <stable code>` |

Animated spinners are optional decoration beside text and stop under reduced
motion. The TUI may ring one bell for a background action-required or terminal
transition when enabled; repeated replay and reconnect events do not ring.
Terminal title contains `Garive`, safe Session label, and semantic state, never
user/Agent content or internal IDs. Its exact grammar is
`Garive · <Workspace|Session N|Session active> · <connection> · <execution>`.
`Session N` is the selected row's one-based ordinal in the currently admitted
Host page; an as-yet-unloaded selection is `Session active`. Title writes occur
only when that presentation changes and every restore path resets it to
neutral `Garive`.

## Theme and accessibility

Semantic tokens are `surface`, `text`, `muted`, `accent`, `success`, `warning`,
`danger`, `focus`, `border`, `code`, and `selection`. Dark, light, system, and
monochrome themes map tokens to terminal capabilities. No fixed RGB value is
required for meaning. `NO_COLOR` selects monochrome unless an explicit CLI
theme overrides it.

`System` is a terminal-background decision, not an alias for `Dark`. A
fullscreen launch performs one bounded OSC 10/11 query before Ratatui or the
terminal event reader can own stdin. Both foreground and background must be
returned under one 100 ms deadline; partial, malformed, or missing replies
select the conservative dark palette. BEL and ST terminators, `rgb`/`rgba`,
two- and four-digit components, and either reply order are accepted. The
background luminance selects light or dark; the foreground is retained as part
of the paired capability result. The resolved value is process-local and is
never persisted in place of the user's `System` preference.

The startup result is the sole resolver for initial rendering, resize reflow,
scroll caching, and a later `/theme system` command. Focus, resize, reconnect,
and external-editor resume do not repeat the query. Screen-reader mode emits no
palette query. Explicit `dark`, `light`, or `mono` remains authoritative;
explicit color themes override `NO_COLOR`, while an implicit `NO_COLOR`
selection remains monochrome.

Terminal rendering capability is frozen once before the first frame. Only
`COLORTERM=truecolor|24bit` admits RGB; `TERM=*256color` maps semantic RGB
tokens to the nearest xterm palette entry; other non-dumb terminals use named
ANSI colors; `TERM=dumb` admits no color. Rounded Unicode borders require a
UTF-8 `LC_ALL`, `LC_CTYPE`, or `LANG` value and otherwise degrade uniformly to
`+`, `-`, and `|`. Theme choice and capability are independent: an explicit
color theme may override `NO_COLOR`, but it cannot claim a color vocabulary
the terminal did not advertise.

Fullscreen clearing never requests a cursor report. Startup, explicit redraw,
and external-editor return clear through the already known fullscreen viewport,
reset Ratatui's previous buffer, and emit no `CSI 6n`; PTY tests synchronize on
the actual clear sequence and reject a cursor query.

- Every status has icon/text in addition to color.
- Focus and selection remain visible in monochrome; keycaps and selected rows
  use terminal-native reverse video rather than assuming a dark background.
- `ContextLine` leads with public Session identity and Agent label. Healthy
  connection is omitted; exceptional connection and active execution use
  separate semantic spans rather than one color-coded sentence.
- `HintLine` is contextual: recovery and cancellation outrank editing hints;
  it shows at most one action and renders its key separately from its verb. It
  is absent whenever an overlay owns input.
- Reduced motion disables the live caret, spinners, and transition frames but
  never disables progressive received content. Active connection/execution
  pulses are composed by the shared motion component; `--reduced-motion`
  replaces them with stable semantic glyph/text, and idle or linear
  screen-reader presentation schedules no decorative motion ticks.
- Screen-reader mode prints semantic blocks once and converts overlays to
  numbered prompts. Those prompts use the same filtered result ordering,
  bounded selection-following window, and activation index as the visual
  overlay. It may announce one live phase change, but never announces each H4
  text delta. The durable committed answer is emitted once after convergence.
- Help describes alternatives when function keys, Shift+Enter, mouse, OSC 8,
  or color are unavailable.
- Bidi controls, zero-width content, and terminal escapes cannot conceal
  action labels or status.

## Competitive quality gate

Comparable quality means the supported Garive workflows meet the observable
properties selected from the audited Codex, Claude Code, and Qoder CLI
evidence while preserving Garive's Host truth model:

| Dimension | Garive gate |
|---|---|
| Responsiveness | input and cancel remain serviceable during Host traffic; no network/file work in render |
| Editing | multiline Unicode, paste, selection, undo/redo, prompt history, byte limit |
| Navigation | durable SessionSwitcher, reopen, pagination, stable scroll/reflow, searchable public Turn jump |
| Recovery | reconnect, snapshot+follow, exact unknown-command retry, terminal restore |
| Presentation | responsive layout, Markdown/code, overlays, themes, semantic status |
| Accessibility | keyboard-only, monochrome, reduced motion, screen-reader linear mode |
| Testability | reducer/property/snapshot/PTY/Runtime E2E and latency baselines |

Provider-specific tools and commands are excluded from parity. A missing
Garive Host capability remains an explicit unavailable state, not a weaker
client-side imitation.

## Acceptance

- editor tests cover ASCII, CJK, emoji ZWJ, combining marks, bidi, wide cells,
  multiline paste, selection, undo/redo, and every byte-bound edge;
- key-routing matrices prove one owner per chord in every focus/overlay/state;
- command parser tests cover every command, quoting, Unicode, malformed input,
  availability, catalog/parser coverage, and no Host fallback;
- snapshots cover all responsive widths, minimum heights, themes, terminal
  capability sets, lifecycle states, overlays, Markdown blocks, and hostile
  control strings;
- scroll/reflow properties preserve stable anchors and bounded visible work;
- live-output reducer tests cover snapshot replacement, exact delta append,
  unavailable clearing, late-event rejection, H1/H2 terminal replacement,
  reduced motion, and Unicode Markdown boundaries. A full-workbench filmstrip
  reuses one model and one render cache across preparing, multiple presented
  deltas, an overlay round trip, finalizing, Ended, and durable takeover;
- Turn-navigator tests cover seeded filtering, no match, exact public-position
  activation, Escape immutability, reload teardown, hostile text, and shared
  keyboard/mouse/linear result ordering;
- screen-reader tests assert semantic line order, no per-delta speech, one
  durable final answer, and absence of animation, cursor addressing, mouse,
  and alternate-screen control;
- PTY tests cover typing, inline slash discovery/completion, paste, resize,
  SessionSwitcher, help, cancellation, reconnect notice, detached scrolling,
  at least two real H4 frames before durable replacement, and clean exit;
- syntax presentation stress-renders 64 labeled blocks / 384 code lines at
  100 cells with debug p95 below 150 ms; the parser bundle remains lazy until a
  recognized labeled fence is rendered.

## See also

- [`tui-visual-system.md`](tui-visual-system.md) — normative visual tokens and component states.
- [`tui-application-architecture.md`](tui-application-architecture.md) — state/effect and terminal ownership.
- [`host-read-model-v1.md`](host-read-model-v1.md) — Session/timeline/suspension public values.
- [`host-agent-activity-v1.md`](host-agent-activity-v1.md) — redacted activity semantics.
- [`host-live-output-v1.md`](host-live-output-v1.md) — ephemeral H4 identity, ordering, failure, and convergence.
- [`../../docs/tui-source-audit.md`](../../docs/tui-source-audit.md) — audited source evidence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
