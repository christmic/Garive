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
| `FocusFrame` | double composer border or accent region border; never moves focus on Host events |
| `CenteredColumn` | caps readable main content at 114 cells without changing model state |
| `ModalFrame` | dims retained workspace, clears popup bounds, rounded focus border, safe padding |
| `AnchoredMenu` | clears only its bounded area, attaches above its owner, preserves page context and owner cursor |

Implementations live in `tui/src/view/primitives.rs` and `style.rs`.
Higher-level renderers must reuse these primitives for equivalent behavior.
The shared Session identity/state presentation lives in `view/session.rs`; the
rail and picker may change density, but cannot invent separate labels, glyphs,
or state wording. The rail's row cadence and visible window also define its
pointer hit boxes; controllers do not duplicate layout coordinates.
The context footer lives in `view/footer.rs` and derives its hints from the
same focus, execution, and responsive state used by input routing.
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
and footer affordances survive.
When mouse capture is enabled, composer pointer placement and drag selection
must call the same component geometry. The border and padding are inert; CJK
double-cell glyphs expose stable before/after insertion points, and selection
remains the same semantic style used by keyboard selection.
All time-varying presentation lives in `view/motion.rs`. An active connection
or execution may use its calm single-cell pulse; reduced motion uses the same
text and semantic style with a stable glyph. Screens cannot invent local frame
sequences or schedule their own redraw loops.

## Composite components

| Component | Required variants |
|---|---|
| Header | compact/full; connection chip; execution chip; safe identity |
| Session rail | empty/populated; selected; terminal/running/action/failed; overflow |
| Conversation | empty/live/scrolled/newer updates; user/Agent/activity/notice cells |
| Markdown cell | nested inline styles; numbered/unordered lists; transparent links; labeled/clipped and syntax-aware code; responsive table grid/records |
| Composer | idle/focused/frozen/action response; placeholder/draft/over-limit; visible grapheme selection |
| Context footer | idle/running/notice/recovery; tiny/full width collapse |
| Picker/palette | empty/filtered/selected/disabled; keyboard-owned selection |
| Command suggestions | prefix/selected/disabled/dismissed; composer-anchored, nonmodal, at most five rows |
| Confirmation | safe consequence, primary action, escape action; no implicit acceptance |

Session and activity state glyphs are closed semantic vocabulary: `✓`
completed, `●` running, `!` action required, `×` failed, `■` stopped, and `○`
unknown/new. Unknown public codes use neutral wording and never borrow success.

## Layout and degradation

At 100 cells and above, the Session rail spans the workspace and the main
column owns conversation, composer, and footer. At 160 and above, the main
column is centered and capped. Below 100, navigation becomes an overlay.
Below 60, footer hints collapse to the highest-priority action and help.
Below `20x8`, render only the safe minimum-size message.

Degradation order is ambient context, secondary hints, decoration, then
nonessential metadata. Never remove the active action, semantic state,
composer cursor, recovery consequence, or selected-row marker to make space.

## Interaction consistency

Only the focused component owns editing/navigation input. Modal ownership
outranks page focus. Selection persists by stable model identity, not screen
row. `Enter` activates the visible primary action; `Esc` closes, defers, or
requests cancellation only as admitted by the current state. Footer hints are
derived from that same routing decision.
Bounded lists keep the selected item inside their visible window. The visual
filter and the activation result set are the same ordered collection. Pointer
hit boxes come from the rendered component geometry and never penetrate a
modal backdrop.
Modal geometry reserves the semantic header and composer/footer boundary on
standard terminals. The reservation contracts responsively on short terminals
so at least the selected row and its actions remain visible. A modal may clear
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
variant. Idle, suspended, failed, disconnected, and screen-reader views do not
run an animation timer. Terminal titles never animate.

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

## Conformance

Every new component or variant requires: a semantic buffer test, dark/light/mono snapshot coverage at its responsive boundary, keyboard ownership tests,
and a real macOS PTY review when it changes terminal behavior. Anchored menus
also require geometry-derived mouse hit tests and proof that modal and
screen-reader paths retain higher ownership. Reviews reject
raw colors outside the palette, duplicated key-hint formatting, color-only
state, content-dependent layout identity, and screenshots without executable
snapshot or PTY evidence.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-31
- Status: accepted
