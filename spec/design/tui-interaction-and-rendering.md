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
5. observe running and typed activity state;
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
+--------------------------------------------------------------------------+
| Garive  <Agent>  <Session title/fallback>       online | running | 12:34 |
+----------------------+---------------------------------------------------+
| Sessions             | Conversation                                      |
| > current            | User                                               |
|   recent             |   Explain the recovery path.                      |
|   older              |                                                   |
|                      | Agent                                              |
| [n] New  [/] Filter  |   The Runtime commits ...                         |
|                      |                                                   |
|                      | Activity: model request completed                  |
+----------------------+---------------------------------------------------+
| > multiline composer                                                     |
|   draft text                                                              |
+--------------------------------------------------------------------------+
| Enter send  Ctrl+J newline  Esc cancel  Ctrl+P commands  ? help   42/4096 |
+--------------------------------------------------------------------------+
```

The frame has five semantic regions: header, navigation, conversation,
composer, and footer. At standard and wide widths the Session rail spans the
full workspace height while conversation, composer, and footer share one main
column. The main column is capped at 114 cells and centered inside excess wide
space, so prose and the composer keep the same readable measure. Overlays are
centered or full-frame views above those regions; they never replace
application state.

### Responsive modes

| Width | Mode | Regions |
|---|---|---|
| `<20` | unsupported | safe message naming minimum `20x8`; no raw IDs or content echo |
| `20..=59` | tiny | conversation, one-line header, composer, compact footer; navigation is an overlay |
| `60..=99` | compact | conversation plus composer; header/status expanded; navigation overlay |
| `100..=159` | standard | `24..32` column Session rail plus conversation |
| `>=160` | wide | `28..36` column rail, readable conversation max width `110`, optional activity inspector |

Height below eight rows shows a safe minimum-size view. Height `8..=15` hides
nonessential help/status rows before reducing composer or active prompt space.
No layout calculation may underflow or panic for any `u16` terminal size.

The conversation text column has a maximum width. Extra wide space belongs to
margins or the activity inspector, not longer prose lines.

## Focus and overlay model

```text
FocusTarget = Navigation | Conversation | Composer | Overlay

Overlay =
  CommandPalette
  | Help
  | SessionPicker
  | PromptHistory
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
For action overlays, key identity, visual keycap, spoken key name, action label,
and semantic controller intent are one application-owned binding. Fullscreen
and linear presenters may format that binding differently but cannot invent an
action or alternate consequence. Explicit newlines in safe status text become
real layout rows, and popup geometry reserves the wrapped body plus every
action row.
Mouse events obey the same ownership. Wheel and click events inside a
selectable overlay move or activate only its rendered rows; events outside the
popup are consumed without scrolling the conversation or activating the rail.

Focus is visible without relying on color. The focused composer uses a double
border; navigation and conversation use an accent border plus a textual
selection marker. A modal dims the workspace, preserves it as visible context,
and gives its active row a background or terminal-native reverse style.
Background Host events never steal focus. A newly committed suspension
opens its prompt only when its Session is selected; otherwise the Session row
shows an action-required badge and the terminal bell follows preference.

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
newline binding. The help/footer show only bindings supported by the detected
terminal capability set.

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

| Operation | Required behavior |
|---|---|
| Insert | one transaction per typed cluster; adjacent typing may coalesce within a bounded interval |
| Paste | normalize line endings; one transaction; validate resulting byte bound before commit |
| Backspace/Delete | remove one grapheme or selected range |
| Word move/delete | Unicode word boundaries with punctuation-preserving behavior |
| Undo/redo | bounded by operation count and total retained bytes; paste undoes atomically |
| Home/End | visual line start/end; `Ctrl+Home/End` document start/end |
| Up/Down | visual line movement; history only at document boundary with no selection |
| Selection | Shift movement; copy only visible selected text on explicit gesture |

With an active selection, an unmodified Left or Right collapses to the start
or end edge and stops without an extra grapheme move. Directional word,
vertical, line-edge, and document-edge movement first collapses to its matching
edge and then performs that movement. This rule is independent of which end is
the anchor and prevents backwards selections from producing asymmetric cursor
jumps.

The editor accepts newline, tab-as-spaces, and printable Unicode. C0/C1 control
characters other than newline/tab are rejected. Bidi isolate characters may be
retained in the request but render with a visible safety marker; bidi override
characters are rejected because visual order could conceal command content.

Empty or whitespace-only input cannot submit. The footer shows UTF-8 bytes
used and the Host command-byte maximum. Crossing the bound leaves the draft
editable, disables send, and names the excess byte count without truncating.

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
| `Enter` | composer | submit when valid; accept selected modal item otherwise |
| `Ctrl+J` | composer | insert newline |
| `Shift+Enter` | composer, when distinguishable | insert newline |
| `Tab` / `Shift+Tab` | no suggestion or blocking overlay | move focus forward/backward |
| `Up` / `Down`, `Home` / `End`, `Enter` | focused Session rail | move the stable rail selection, jump to an edge, or open the visibly selected Session |
| `Up` / `Down`, `PageUp` / `PageDown`, `Home` / `End` | focused conversation | scroll one cell, scroll one viewport, jump oldest, or follow latest |
| `Ctrl+P` | any ready view | open command palette |
| `Ctrl+R` | composer | open prompt-history search |
| `Ctrl+N` | ready | create Session using selected/default definition |
| `Ctrl+S` | ready | open Session picker |
| `Ctrl+L` | conversation | redraw current frame; does not erase durable history |
| `Esc` | overlay | close/defer when allowed |
| `Esc` | running selected Turn | request cancel after footer hint; no terminal claim |
| `Ctrl+C` | running selected Turn | request cancel |
| `Ctrl+C` | idle with draft/selection | clear selection, then draft on next press |
| `Ctrl+C` twice within 1500 ms | idle empty composer | open quit confirmation |
| `Ctrl+Q` | any nonblocking state | open quit confirmation |
| `PageUp/PageDown` | conversation | scroll one viewport |
| `Ctrl+Home/End` | conversation | oldest loaded / follow latest |
| `?` | empty composer | open help |

The key router resolves overlay, editor, focused region, then global bindings
in that order. A key is consumed by at most one owner. Footer hints reflect the
current resolved bindings rather than a static list.
Typing a printable character outside an overlay explicitly transfers focus to
the composer before inserting it. Editing and deletion keys never mutate a
draft while the Session rail or conversation owns focus.
The Session rail derives painting, keyboard visibility, and mouse hit-testing
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
| `/sessions` | optional filter text | open Session picker |
| `/help` | none | open contextual help |
| `/status` | none | open safe connection/Session details |
| `/retry` | none | replay exact pending mutation when available |
| `/reconnect` | none | start explicit reconnect from saved cursor |
| `/cancel` | none | request cancellation of selected running Turn |
| `/theme` | `system`, `dark`, `light`, or `mono` | update local preference |
| `/mouse` | `on` or `off` | change mouse capture safely |
| `/copy` | `last` or `session-id` | copy visible completion or opaque ID |
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

## Session navigation

Session rows show a bounded display label derived from committed public text,
latest Turn state, last activity time, and action-required marker. Until a
public title contract exists, the rail uses the public Agent definition label
plus a short opaque Session suffix; it does not fabricate a repeated title or
expose prompt text. State always has a non-color glyph (`✓`, `●`, `!`, `×`,
`■`, or `○`) and text. Full opaque IDs remain hidden until the details action.

The picker supports case-folded substring filtering over public label and
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
TimelineCell = User | Agent | Activity | Suspension | Terminal | Notice
```

Each cell has a stable key `(session_id, turn_id?, durable_position?, kind)` so
updates replace the intended cell without rebuilding unrelated history.
Rendered cells are cached by key, width, theme, and content digest. The model
retains bounded public values, never terminal escape bytes.

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

### Scrolling

The viewport anchors to the newest content while `follow_latest` is true.
Manual upward scroll disables follow and shows `N newer updates`; new events do
not jump the viewport. `End` or activating that badge returns to latest.

Reflow on resize preserves the top visible stable cell and its source-line
offset where possible. Loading older pages preserves the current visible
anchor. Render work is bounded to the visible window plus an overscan margin;
the TUI does not lay out the complete Session on every frame.

### Activity and suspension

H3 public activities render semantic icon/text, status, Turn, and durable
position. Unknown activity kinds use `Activity updated` and cannot mutate Turn
state. Tool arguments, raw paths, hidden reasoning, provider values, and
internal facts are absent.

An admitted H2 suspension overlay renders only the public title/message and
response schema. String, boolean, enum, bounded number, and bounded object
fields receive native terminal controls. Unsupported schemas show read-only
status and no fabricated input. Submission uses the exact H1 text or canonical
JSON continuation variant and binds the displayed schema digest.

## Status and notification language

| State | Visible wording |
|---|---|
| submitting | `Committing turn…` |
| following | `Agent running` |
| cancelling | `Cancellation requested…` |
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

- Every status has icon/text in addition to color.
- Focus and selection remain visible in monochrome; keycaps and selected rows
  use terminal-native reverse video rather than assuming a dark background.
- Header connection and execution chips are separate semantic spans, not one
  color-coded status sentence.
- The footer is contextual: notices and cancellation outrank editing hints;
  hints collapse by width and render keys separately from their descriptions.
- Reduced motion disables spinners and transition frames. Active
  connection/execution pulses are composed by the shared motion component;
  `--reduced-motion` replaces them with stable semantic glyph/text, and idle or
  linear screen-reader presentation schedules no motion ticks.
- Screen-reader mode prints semantic blocks once and converts overlays to
  numbered prompts. Those prompts use the same filtered result ordering,
  bounded selection-following window, and activation index as the visual
  overlay.
- Help describes alternatives when function keys, Shift+Enter, mouse, OSC 8,
  or color are unavailable.
- Bidi controls, zero-width content, and terminal escapes cannot conceal
  action labels or status.

## Competitive quality gate

Comparable quality means the supported Garive workflows meet these observable
properties also present in the audited Codex/Grok Build implementations:

| Dimension | Garive gate |
|---|---|
| Responsiveness | input and cancel remain serviceable during Host traffic; no network/file work in render |
| Editing | multiline Unicode, paste, selection, undo/redo, prompt history, byte limit |
| Navigation | durable Session picker, reopen, pagination, stable scroll/reflow |
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
- screen-reader tests assert semantic line order and absence of animation,
  cursor addressing, mouse, and alternate-screen control;
- PTY tests cover typing, inline slash discovery/completion, paste, resize,
  Session picker, help, cancellation, reconnect notice, and clean exit.
- syntax presentation stress-renders 64 labeled blocks / 384 code lines at
  100 cells with debug p95 below 150 ms; the parser bundle remains lazy until a
  recognized labeled fence is rendered.

## See also

- [`tui-visual-system.md`](tui-visual-system.md) — normative visual tokens and component states.
- [`tui-application-architecture.md`](tui-application-architecture.md) — state/effect and terminal ownership.
- [`host-read-model-v1.md`](host-read-model-v1.md) — Session/timeline/suspension public values.
- [`host-agent-activity-v1.md`](host-agent-activity-v1.md) — redacted activity semantics.
- [`../../docs/tui-source-audit.md`](../../docs/tui-source-audit.md) — audited source evidence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
