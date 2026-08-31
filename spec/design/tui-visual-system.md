# TUI visual system

> Normative visual and interaction-component contract for every Garive TUI
> screen. Product semantics remain in the interaction and Host Specs.

## Principles

1. Hierarchy precedes decoration: workspace, active task, and next action must be recognizable from shape and text before color is applied.
2. Durable truth is visually distinct from local intent and transient notice. One component owns one interaction pattern.
3. Screens compose components; they do not invent local colors, borders, key-hint syntax, or selection.
4. Every state survives monochrome, reduced motion, resize, and replay.

## Tokens

`Palette` is the only source of terminal styles. Components consume semantic
tokens: `normal`, `muted`, `accent`, `title`, `border`, `focus`, `selection`,
`surface`, `success`, `warning`, `danger`, `notice`, and `keycap`. Dark, light,
system, and mono map these tokens; renderers must not select raw colors.

Meaning requires text or glyph plus style. Mono selection and keycaps use
terminal reverse video. RGB backgrounds are enhancement only. Contrast must
not depend on the terminal default being light or dark.

## Primitive components

| Primitive | Contract |
|---|---|
| `StatusChip` | padded icon/text span; semantic state style; never color-only |
| `KeyHint` | visually distinct keycap plus verb; ordered by current action priority |
| `SelectionRow` | full-row highlight plus stable cursor/marker; reverse video in mono |
| `FocusFrame` | stable Composer or modal boundary; focus style never changes geometry or moves on Host events |
| `CenteredColumn` | caps readable transcript width without changing model state |
| `ModalFrame` | dims retained workspace, clears popup bounds, rounded focus border, safe padding |
| `AnchoredMenu` | clears only its bounded area, attaches above its owner, preserves page context and owner cursor |
| `RoleMarker` | restrained non-color User or Agent identity; never a full-width card border |
| `LiveCaret` | single-cell active-output cue; hidden for reduced motion, unavailable preview, and terminal state |

Implementations live in `tui/src/view/primitives.rs` and `style.rs`.
Higher-level renderers must reuse these primitives for equivalent behavior.
The shared Session identity/state presentation lives in `view/session.rs`;
SessionSwitcher and Inspector may change density, but cannot invent separate
labels, glyphs, or state wording. Their visible windows also define pointer hit
boxes; controllers do not duplicate layout coordinates. `ContextLine` and
`HintLine` derive their copy from the same Session, execution, connection,
focus, and recovery state used by input routing. `ContextLine` has no frame or
background fill. `HintLine` renders at most one highest-priority action and may
be absent.
The composer lives in `view/composer.rs`. It consumes the editor's admitted
byte range, styles whole rendered graphemes, and owns its frame, viewport, and
cursor geometry. Dark/light selection uses the semantic selection surface;
mono uses reverse video. Selection may not be communicated by color alone.
Its private `EditorLayout` is the single source for rendered rows, selection
spans, cursor coordinates, and vertical scroll. It measures sanitized extended
graphemes in terminal cells, prefers whitespace wrap points, and hard-wraps an
oversized word without splitting a grapheme. Screens and controllers may not
recompute composer wrapping or cursor coordinates.
Visual Up/Down navigation also consumes this layout. Its sticky column is a
terminal-cell column, and destinations clamp only at complete grapheme
insertion points. Rendering, cursor placement, selection, scrolling, height,
pointer hit testing, and vertical navigation therefore cannot disagree about
where a row begins or ends.
Home/End targets are component-owned visual-row edges from the same result;
controllers may not reinterpret them as newline-delimited logical edges.
At non-tiny heights the composer frame grows from three to at most seven rows
using the layout's visual row count, including an exact-width cursor
continuation row. It does not grow from logical newline count. Below the
height breakpoint it remains three rows and follows the cursor so conversation
and the highest-priority hint survive.
When mouse capture is enabled, composer pointer placement and drag selection
must call the same component geometry. The border and padding are inert; CJK
double-cell glyphs expose stable before/after insertion points, and selection
remains the same semantic style used by keyboard selection.
Private kill/yank state has no persistent badge, counter, or preview. The
keyboard guide presents its supported chords through the same semantic keycap
primitive as all other actions; only a safe transient notice may report a
failed bounded yank. This avoids exposing killed draft content or inventing a
second clipboard surface.
The keyboard guide groups compatible aliases into compact semantic rows, but
the displayed and screen-reader labels must come from the typed key catalog
that the controller resolves. Compact grouping may omit secondary aliases only
when the complete set remains in the manual; it may not rename an action or
show an unbound chord.
All time-varying presentation lives in `view/motion.rs`. An active connection
or execution may use its calm single-cell pulse, and active H4 output may use a
`LiveCaret`. Reduced motion uses the same text and semantic style with a stable
glyph or no caret; it does not suppress newly received content. Screens cannot
invent local frame sequences or schedule their own redraw loops.

## Composite components

| Component | Required variants |
|---|---|
| `ContextLine` | compact/full; safe Session and Agent identity; exceptional connection; active execution |
| `TurnBlock` | User request; activity; live/committed answer; suspended/terminal outcome |
| `ActivityStack` | active row; collapsed completed summary; explicit expanded Inspector detail |
| `LiveAnswer` | empty/phase/streaming/unavailable/ended; stable prefix; mutable tail; optional caret |
| `MarkdownAnswer` | nested inline styles; numbered/unordered lists; transparent links; labeled/clipped and syntax-aware code; responsive table grid/records |
| Composer | idle/focused/frozen/action response; placeholder/draft/over-limit; visible grapheme selection |
| `HintLine` | absent/action/notice/recovery; one highest-priority item |
| `SessionSwitcher` | empty/filtered/selected/terminal/running/action/failed; overflow |
| `Inspector` | closed/activity/recovery/details; optional wide column or overlay |
| Command palette | empty/filtered/selected/disabled; keyboard-owned selection |
| Turn navigator | empty/filtered/selected; public ordinal and prompt preview; keyboard/mouse/linear parity |
| Command suggestions | prefix/selected/disabled/dismissed; composer-anchored, nonmodal, at most five rows |
| `DecisionSheet` | suspension/confirmation; safe consequence; primary and escape actions; no implicit acceptance |

Session and activity state glyphs are closed semantic vocabulary: `✓`
completed, `●` running, `!` action required, `×` failed, `■` stopped, and `○`
unknown/new. Unknown public codes use neutral wording and never borrow success.

An explicitly opened Inspector is exactly 32 cells wide at `>=120`, including
its single border. It never expands the transcript beyond its 96-cell maximum.
At `40..=119` the same Activity, Recovery, or Details projection is a bounded
top-level overlay; below 40 its open state is retained behind the safe minimum
view. The variant title, stable selected marker, empty state, entry labels, and
safe details remain visible without color. Fullscreen, pointer, and linear
screen-reader variants share one ordered entry projection and activation.

`ContextLine` is exactly one unbordered row. Public Session identity leads,
followed by the Agent label. Healthy connection is absent; reconnecting,
disconnected, and unavailable states appear only while exceptional. Active
execution may add one compact semantic phrase. Brand background fills, padded
status chips, clocks, raw IDs, and a second persistent status row are
prohibited.

Spacing is the transcript's primary structure. Borders are reserved for the
Composer, modal boundaries, and an explicitly opened Inspector. The Composer
is the only persistent framed surface; focus changes its semantic border or
caret styling without switching border shape or moving content. Accent is
limited to the insertion caret, current selection, active work cue, and primary
decision. Large accent fills and repeated nested boxes are prohibited.

`TurnNavigator` reuses `ModalFrame`, `SelectionRow`, and the shared filtered
list geometry rather than inventing a second picker surface. Its title is
`Jump to a Turn`; the search row remains visible above the results; each row
uses a right-aligned ordinal gutter and one sanitized prompt line. The selected
row is always visible and remains identifiable in mono through reverse video
and a marker. Empty search results retain the title and filter and render
`No matching Turns` in the normal muted text role.

Wide and compact layouts bound the modal to readable conversation width and
terminal height; previews truncate on grapheme/display-cell boundaries with a
visible ellipsis. Tiny layout uses the full safe content rectangle. Linear
screen-reader presentation emits the title, filter, result count, selected
marker, ordinal, preview, and available actions in semantic order. No variant
shows a Turn ID, stable key, hidden activity, or full prompt in popup chrome.

`TurnBlock` uses spacing as its primary separator: one blank row between Turns
and none between tightly related activity and answer rows. User content has a
left role marker and restrained emphasis without a surrounding full-width
card. Agent prose remains on the terminal background. Public positions,
stable keys, opaque IDs, and repeated `Conversation` titles do not appear in
ordinary transcript chrome.

`ActivityStack` paints at most one active safe row plus one collapsed completed
summary. Expanding details opens Inspector or an overlay; it does not insert a
dashboard pane or expose tool arguments, raw paths, provider values, or hidden
reasoning. State always has a semantic word or glyph in addition to color.

`LiveAnswer` shares the Agent answer measure but does not masquerade as a
durable cell. Its received source is partitioned into a monotonic stable
Markdown block prefix and one mutable tail. The stable prefix keeps its parsed
presentation; only the tail reparses as H4 deltas arrive. An active available
preview ends with `LiveCaret`. Unavailable preview shows one muted line and no
partial suffix. The H1/H2 committed answer atomically replaces the complete
live component without a transition card or duplicate answer.

## Layout and degradation

At 120 cells and above, the bounded transcript is centered and an explicitly
opened 32-column Inspector may share the work surface. From 80 through 119,
the transcript remains centered and Inspector becomes an overlay. From 52
through 79, the transcript uses the available width and metadata collapses to
compact labels. From 40 through 51, the transcript becomes linear,
`ActivityStack` becomes one summary row, and `HintLine` shows at most one
action. Below 40 cells, render the safe minimum-size view while retaining the
Composer draft in model state. Below `20x8`, the same view may reduce to only
its bounded size message.

Degradation order is ambient context, secondary hints, decoration, then
nonessential metadata. Never remove the active action, semantic state,
composer cursor, live received content, recovery consequence, or selected-row
marker to make space.

## Interaction consistency

Only the focused component owns editing/navigation input. Modal ownership
outranks page focus. Selection persists by stable model identity, not screen
row. `Enter` activates the visible primary action; `Esc` closes, defers, or
requests cancellation only as admitted by the current state. Hints derive from
that same routing decision and reduce to the single `HintLine` priority winner.
Bounded lists keep the selected item inside their visible window. The visual
filter and the activation result set are the same ordered collection. Pointer
hit boxes come from the rendered component geometry and never penetrate a
modal backdrop.
Conversation scrolling uses stable Turn identity. Manual upward scroll detaches
the viewport; durable updates and visible H4 frame advances increment the
unseen count without stealing focus or follow mode. `End` resumes latest-follow.
Inspector and TurnNavigator may expose direct navigation, but the default
transcript has no permanent position rail or hover preview.
Modal geometry reserves the semantic ContextLine and Composer/HintLine
boundary on standard terminals. The reservation contracts responsively on
short terminals so at least the selected row and its actions remain visible. A modal may clear
only its rectangle plus a two-cell same-height horizontal halo; it must not
erase rows above or below, splice the composer frame, or hide the command
palette action row. The command palette uses compact vertical chrome and a
fixed command column so all admitted rows fit at `160x28`; unavailable detail
is one grapheme-safe display-width-truncated line.
An anchored command menu is not a modal. It uses the overlay border and shared
selection row, clears only its own rectangle, aligns its left edge with the
composer, caps width at 76 cells, and grows upward by two border rows plus at
most five result rows. It never shifts conversation, composer, or cursor
geometry. It is suppressed below `30x12`, for multiline/argument drafts, when
the composer is not focused, and whenever a modal owns input.
Its rows retain one cell of breathing room inside each vertical border. The
command input is the identity and remains visible; help or unavailable detail
uses only the remaining display-cell budget and ends with `…` when truncated.
Truncation follows grapheme and Unicode display width, including CJK and
combining sequences. Border and padding cells are outside pointer hit regions.
The linear screen-reader component uses that same filtered order and marks the
selected numbered row explicitly; it cannot maintain a separate list model.
Command rows also consume the registry's shared availability result. A
disabled visual or linear row names the same safe reason that blocks Enter.
Confirmation, status, ephemeral-mode, and unknown-result cards consume one
typed action-binding table for visible keycaps, spoken key names, action copy,
and controller intent. Their height is derived from the sanitized multiline
body at the actual popup width; wrapping must never hide the primary or escape
action. Durable uncertainty is titled `Command result unknown`, not the
ambiguous `Unknown command`.

Motion ticks are scheduled only while a typed state has a visible animated
variant. H4 content draws use a separate bounded frame governor: adjacent
deltas may coalesce before a frame, presented text catches received text within
two available frames, and bursts catch up rather than preserving fake typing.
Idle, suspended, failed, disconnected, reduced-motion caret, and screen-reader
views do not run a decorative animation timer. Screen-reader presentation does
not emit each delta; it may emit one phase change and emits the durable answer
once after convergence. Terminal titles never animate.

Markdown styling is compositional rather than a single mutable flag: ending an
inner emphasis cannot erase an enclosing strong, link, or heading style.
Headings return to normal body style at their boundary. Explicit links render
their label and a bounded sanitized destination without emitting OSC 8.
Fenced code uses one semantic frame, retains its first bounded language token,
expands tabs to four cells for display, and clips by grapheme/display width with
an explicit `…`; source text remains unchanged for copy.

Syntax color is a component contract, not a theme passthrough. Recognized
fenced languages map parser scopes to `normal`, `comment`, `string`,
`constant`, `keyword`, `type`, `function`, and `punctuation` semantic roles.
Those roles consume only palette styles: dark and light use polarity-safe
colors, while mono distinguishes roles with weight, italic, underline, and
muted punctuation. No state may be color-only. Unlabeled or unknown languages
render plain code and remain visually framed; Garive never guesses a language
from content.

Markdown tables are one component with two presentations. When every column
can retain at least six display cells, a compact content-aware grid uses bold
headers, a semantic accent rule, muted separators, and declared left/center/
right alignment. Below that boundary, each body row becomes a labeled record;
`Header: Value` preserves cell emphasis and records are separated by muted
`···`. Header labels truncate with `…`, never silently. Prefix and block-quote
gutters are deducted before layout, and neither presentation may exceed its
assigned display width.

## External-editor presentation

No modal chrome remains over a child-owned terminal. Before handoff, Help and
the command row name `Ctrl+G edit`; after return, the composer is fully repainted
with a short semantic result notice. No spinner implies Garive still owns the
screen. Linear mode emits one bounded handoff line and one result line.

## Conformance

Every new component or variant requires a semantic buffer test,
dark/light/mono snapshot coverage at its responsive boundary, keyboard
ownership tests, and a real macOS PTY review when it changes terminal behavior.
Anchored menus also require geometry-derived mouse hit tests and proof that
modal and screen-reader paths retain higher ownership. Reviews reject raw
colors outside the palette, duplicated key-hint formatting, color-only state,
content-dependent layout identity, and screenshots without executable snapshot
or PTY evidence.

## See also

- [`tui-interaction-and-rendering.md`](tui-interaction-and-rendering.md) — product behavior, input ownership, and responsive composition.
- [`host-live-output-v1.md`](host-live-output-v1.md) — H4 live-answer state,
  frame, failure, and convergence rules.
- [`../../docs/tui-product-redesign.md`](../../docs/tui-product-redesign.md) —
  accepted conversation-first product decision.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
