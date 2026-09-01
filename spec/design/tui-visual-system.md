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
| `SelectionRow` | explicit marker-only or full-area policy plus stable cursor; reverse video in mono |
| `ComposerDock` | low-contrast input body, responsive top boundary/status row, shared cursor/hit geometry |
| `ComposerBoundary` | one top rule at `>=52`; optional left-aligned status title; compact text fallback; no sides/bottom |
| `AmbientFooter` | one muted identity row below the Composer; displaced by actionable feedback |
| `CenteredColumn` | caps readable transcript width without changing model state |
| `BottomPane` | one Composer-aligned top rule; content-driven height; no backdrop dimming or side/bottom border |
| `ModalFrame` | dims retained workspace, clears popup bounds, rounded focus border, safe padding |
| `AnchoredMenu` | clears only its bounded area, attaches above its owner, preserves page context and owner cursor |
| `RoleMarker` | restrained non-color User or Agent identity; never a full-width card border |
| `LiveCaret` | single-cell active-output cue; hidden for reduced motion, unavailable preview, and terminal state |

`ComposerDock` reuses the submitted-request visual grammar without copying its
identity: dark/light use the low-contrast `request_surface`, mono uses the
terminal background, and focus changes only the non-color `›` lead and terminal
caret. At widths `>=52`, its reserved row is one muted top rule and carries
`Draft locked`, `Action response`, or the running Turn rail as a left-aligned
title only when that state is true. Widths `40..=51` omit the rule and render
only real status text. Before transcript work is visible, the running rail combines the nearby
work state and cancel control and starts on the shared left axis as
`• phase · esc to interrupt`. Once a non-terminal live answer or selected active
Activity owns that signal, the rail reduces to the muted `esc to interrupt`
control; an overlay suppresses even that background action. It never strands
the primary state at the terminal's right edge. No Composer variant adds side
or bottom borders. `RequestSurface` renders User input as an
unbordered, low-contrast terminal-width group with the same non-color `›`
identity and a two-cell hanging indent. At widths `>=80`, one same-surface blank
row above and below the content makes the authored Turn read as one calm block
instead of a thin color stripe. Widths `52..=79` and `40..=51` omit those
optional rows. Its wrapper measures sanitized grapheme display width, so CJK
and combining sequences cannot split or shift later components.

Implementations live in `tui/src/view/primitives.rs` and `style.rs`.
Higher-level renderers must reuse these primitives for equivalent behavior.
`SelectionRow` requires an explicit extent. CommandPalette uses `MarkerOnly`;
single-line lists and anchored menus use `FullArea`; Inspector applies
`FullArea` to its content-driven one- or two-row entry rectangle.
`RoleMarker` is the only source for the User `›` and Agent `•` identity, so
committed and live answers cannot diverge. A Markdown heading already carries
its own leading structure and suppresses the Agent marker; `• ##` is
prohibited. `LiveCaret` alone combines
availability, terminal state, and reduced motion before emitting its one-cell
cue; answer renderers cannot append a private caret glyph.
`ModalFrame` alone computes modal inner padding and paints the retained-workspace
backdrop, full-width same-height quiet band, cleared popup, title, and rounded
border. The quiet band removes horizontally clipped transcript fragments while
leaving the rows above and below the modal available as dimmed context.
The reference Help surface alone consumes `ModalFrame`. DecisionSheet,
recovery confirmations, CommandPalette, navigation selectors, and the
narrow/standard Inspector consume `BottomPane` and cannot reproduce modal
chrome locally. Decision blocking is expressed by typed input ownership,
explicit primary/safe-exit actions, and a visible tone marker—not backdrop
dimming or a four-sided card.
Help is content-driven rather than a fixed document card. At admitted desktop
width it is an action-only two-column grid derived from the typed keymap; when
the two columns do not fit it reflows by measured display width. Its height is
the resulting action rows plus `ModalFrame` chrome. Architecture, Host
durability, terminal capability, color, mouse, and clipboard notes do not
belong in this reference surface.
The shared Session identity/state presentation lives in `view/session.rs`;
SessionSwitcher and Inspector may change density, but cannot invent separate
labels, glyphs, or state wording. Their visible windows also define pointer hit
boxes; controllers do not duplicate layout coordinates. `ContextLine` and
`HintLine` derive their copy from the same Session, execution, connection,
focus, and recovery state used by input routing. `ContextLine` has no frame or
background fill. `HintLine` renders at most one highest-priority action and may
be absent. Its reserved slot then renders the muted `AmbientFooter` with public
Agent/Session identity, never a permanent shortcut legend. When exceptional
state makes `ContextLine` visible, it owns that identity and the reserved footer
slot stays blank unless an actionable `HintLine` displaces it; identity cannot
appear at both edges of the workspace. Before a selected durable Session
exists, the Composer invitation owns the new-work identity and the ambient
footer remains blank instead of repeating `Agent · New conversation`. At supported heights
of nine rows or more, the slot remains allocated so selection, notices, and Host events never
move the Composer hit geometry; tiny layouts below nine rows remove the slot as
part of their explicit degradation.
The composer lives in `view/composer.rs`. It consumes the editor's admitted
byte range, styles whole rendered graphemes, and owns its dock, viewport, and
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
At non-tiny heights the ComposerDock grows from two to at most six rows
using the layout's visual row count, including an exact-width cursor
continuation row. It does not grow from logical newline count. Below the
height breakpoint it remains two rows and follows the cursor so conversation
and the highest-priority hint survive.
When mouse capture is enabled, composer pointer placement and drag selection
must call the same component geometry. The status/separator row is inert; CJK
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
All time-varying presentation lives in `view/motion.rs`. Active execution uses
one width-stable `•`/`◦` pulse on the Composer run rail; each phase lasts four
160 ms motion ticks, so the cue reads as calm state rather than a fast spinner.
An unknown outcome stays static because no work progress is known. Active H4
output uses a `LiveCaret` on the same four-tick pulse after it replaces the
generic rail cue. Reduced motion keeps the same text and semantic style with a
stable `•` and no caret; it does not suppress newly received content. Screens
cannot invent local frame sequences or schedule their own redraw loops.

## Composite components

| Component | Required variants |
|---|---|
| `ContextLine` | compact/full; safe Session and Agent identity; exceptional connection |
| `TurnBlock` | User request; activity; live/committed answer; suspended/terminal outcome |
| `ActivityStack` | active row; collapsed completed summary; explicit expanded Inspector detail |
| `LiveAnswer` | empty/phase/streaming/unavailable/ended; stable prefix; mutable tail; optional caret |
| `MarkdownAnswer` | nested inline styles; numbered/unordered lists; transparent links; labeled/clipped and syntax-aware code; responsive table grid/records |
| Composer | idle/focused/running/frozen/action response; truthful placeholder/draft/over-limit; visible grapheme selection |
| `HintLine` | absent/action/notice/recovery; one highest-priority item |
| `SessionSwitcher` | empty/filtered/selected/terminal/running/action/failed; overflow |
| `Inspector` | closed/activity/recovery/details; optional wide column or overlay |
| Command palette | empty/filtered/selected/disabled; keyboard-owned selection |
| Turn navigator | empty/filtered/selected; public ordinal and prompt preview; keyboard/mouse/linear parity |
| Command suggestions | prefix/selected/disabled/dismissed; composer-anchored, nonmodal, at most five rows |
| `DecisionSheet` | suspension/confirmation; safe consequence; primary and escape actions; no implicit acceptance |

The master `5d1babef` component checkpoint is bound to executable visual,
linear, and pointer evidence. CommandPalette owns its `160x28` and `40x8`
window/overflow geometry (`db5b2de6`). Its top-rule title carries the compact
visible range (`1–8/21`), so a separate prose count cannot consume a result row;
the search row, real results, and action row are the only interior owners.
TurnNavigator, Inspector, DecisionSheet,
and Recovery keep one selected row plus safe actions at `40x8` (`d7af6a4f`);
public Session/Agent labels replace opaque identifiers (`60021509`); and
connection hint, title, Inspector, and linear text share one truthful recovery
state (`65e1d8d4`). Accent is reserved for the active selection in dark, light,
and mono (`c843bae5`). The executable contracts are
[`view.rs`](../../tui/tests/view.rs),
[`view_snapshots.rs`](../../tui/tests/view_snapshots.rs),
[`overlay_accessibility.rs`](../../tui/tests/overlay_accessibility.rs),
[`identity_labels.rs`](../../tui/tests/identity_labels.rs),
[`connection_truth.rs`](../../tui/tests/connection_truth.rs), and
[`accent_hierarchy.rs`](../../tui/tests/accent_hierarchy.rs). They prove cells,
styles, ordering, and hit geometry, not physical-terminal rasterization.

Session and activity state glyphs are closed semantic vocabulary: `✓`
completed, `●` running, `!` action required, `×` failed, `■` stopped, and `○`
unknown/new. Unknown public codes use neutral wording and never borrow success.

An explicitly opened Inspector is exactly 32 cells wide at `>=129`, including
its single border. The breakpoint is structural: 96 transcript cells, one gap,
and 32 Inspector cells must all fit before the surfaces become side by side.
At `40..=128` the same Activity, Recovery, or Details projection is a bounded,
Composer-aligned `BottomPane` with one top rule and no backdrop dimming; below
40 its open state is retained behind the safe minimum view. The variant title,
stable selected marker, empty state, entry labels, and
safe details remain visible without color. Fullscreen, pointer, and linear
screen-reader variants share one ordered entry projection and activation.
Inspector density follows content, not a fixed card rhythm: entries without an
independent detail use one row; entries with safe explanatory detail use two.
The footer remains reserved and is never a selectable entry row.

`ContextLine` is exactly one unbordered row. Public Session identity leads,
followed by the Agent label. Healthy connection is absent; reconnecting,
disconnected, and unavailable states appear only while exceptional. Running
execution belongs to the Composer run rail instead of a distant second status
owner. Brand background fills, padded status chips, clocks, raw IDs, and a
second persistent status row are prohibited.

An ordinary empty transcript is intentionally silent. `ContextLine` owns
loading and exceptional connection truth, `ComposerDock` owns the invitation
to act, and `AmbientFooter` owns an existing selected Session's identity only. The transcript
must not repeat the brand, invitation, shortcut discovery, or loading state.
Only blocking empty states render body copy: missing configuration names the
Agent-install path and degraded Host access names `/status` as the recovery
path.

Spacing is the transcript's primary structure. Enclosing borders are reserved
for modal boundaries and an explicitly opened Inspector. `ComposerBoundary`
owns one top rule without sides or a bottom; focus changes the Composer lead/
caret styling without moving content. Accent is
limited to the insertion caret, current selection, active work cue, and primary
decision. Large accent fills and repeated nested boxes are prohibited.

Composer-derived lists are the exception to modal framing: `BottomPane` owns a
single top rule, Composer-aligned width, a shared leading-marker gutter,
selected-row surface, and content-driven height. Its selected `›` occupies the
same column as the Composer marker. It never centers, dims the transcript, or
draws left, right, or bottom borders. CommandPalette and CommandSuggestions
consume this same primitive so mouse, keyboard, visual, and compact geometry
cannot drift.

`SessionSwitcher`, `TurnNavigator`, and `PromptHistory` reuse `BottomPane`,
`SelectionRow`, and the shared filtered-list geometry rather than inventing a
second picker surface. They stay on the Composer axis and never dim retained
conversation context. `TurnNavigator`'s title is
`Jump to a Turn`; the search row remains visible above the results; each row
uses a right-aligned ordinal gutter and one sanitized prompt line. The selected
row is always visible and remains identifiable in mono through reverse video
and a marker. Empty search results retain the title and filter and render
`No matching Turns` in the normal muted text role.

`Help` is a bounded reference surface rather than a selection list. Standard
width packs the canonical key catalogue into bounded rows and retains the safe
terminal notes. Its complete spoken labels and notes remain available through
the linear screen-reader projection when height pressure clips visual notes.

Wide and compact layouts bound the pane to available terminal height; previews
truncate on grapheme/display-cell boundaries with a
visible ellipsis. Tiny layout uses the full safe content rectangle. Linear
screen-reader presentation emits the title, filter, result count, selected
marker, ordinal, preview, and available actions in semantic order. No variant
shows a Turn ID, stable key, hidden activity, or full prompt in popup chrome.

`TurnBlock` uses spacing as its primary separator: one blank row between Turns.
At widths `>=52`, one additional rhythm row separates an `ActivityStack` from
the live or committed answer so operation metadata cannot merge visually with
Agent prose; the 40–51 column linear mode keeps that boundary tight. User
content uses the responsive `RequestSurface`: one low-contrast grouped fill, a
left role marker, and no separate `You` header or surrounding border. Standard
and wide surfaces own one blank fill row above and below; compact and linear
surfaces stay tight. Agent prose remains on the terminal background. Public positions,
stable keys, opaque IDs, and repeated `Conversation` titles do not appear in
ordinary transcript chrome.

`ActivityStack` paints at most one active safe row plus the latest completed
safe label and a supplemental completed count. Compact width retains the active
label first, then a display-width-budgeted `✓N` suffix; CJK and emoji cannot
silently consume that counter. The Composer run rail omits its generic
execution phrase whenever a live answer or active Activity already owns the
work signal, while retaining its cancel control.
Expanding details opens Inspector or an overlay; it does not insert a
dashboard pane or expose tool arguments, raw paths, provider values, or hidden
reasoning. State always has a semantic word or glyph in addition to color.

Detached conversation state uses one borderless `FollowCue` above the viewport.
Its arrow and unseen count use the semantic badge accent, `End` uses the shared
keycap primitive, and `follow latest` stays muted. Monochrome therefore retains
arrow/text identity plus a reverse-video keycap. An active overlay suppresses
the action while preserving passive unseen status, so background chrome never
advertises an input it cannot receive.

`LiveAnswer` shares the Agent answer measure but does not masquerade as a
durable cell. Its received source is partitioned before the final top-level
Markdown block, leaving one structurally stable prefix and one mutable tail.
Loose lists, block quotes, tables, setext headings, and fences therefore remain
whole while they grow. A reference definition disables the split because it
may change links in earlier source. The stable prefix keeps its parsed
presentation; only the tail reparses otherwise. An active available preview
with visible text ends with `LiveCaret`. Before the first visible delta, the
component shows only its phase row and Turn gap: it cannot paint an empty Agent
bubble or orphan caret. Unavailable preview shows one muted line and no partial
suffix. The H1/H2 committed answer atomically replaces the complete live
component without a transition card or duplicate answer.

## Layout and degradation

At 129 cells and above, the bounded transcript is centered and an explicitly
opened 32-column Inspector may share the work surface. From 80 through 128,
the transcript remains centered and Inspector becomes a bottom pane. From 52
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
When the usable transcript has eight rows or fewer, its decorative top inset
collapses before semantic content. The latest Agent identity row must remain
visible when it can fit; an empty breathing row may not scroll it away.

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

`DecisionSheet` uses one Composer-aligned top rule and one display-cell row plan
for visual rendering, dynamic height, choice hit testing, and action hit
testing. It has no side/bottom border or dimmed backdrop. Tone glyphs share the first
consequence row and consume its width budget. Normal height keeps one blank row
before actions; compact height removes that spacer before it removes the
selected control or either primary/safe-exit action. When compact height folds
unselected choices, the selected row reserves a bounded `N/T ↑↓` suffix before
Unicode-safe label truncation; the suffix is not a synthetic choice hit target.
Scalar input shows a
grapheme-safe visible caret, and enum labels are sanitized and clipped without
changing the submitted value. Linear presentation consumes the same typed
title, body, response state, and action bindings.

Conversation scrolling uses stable Turn identity. Manual upward scroll detaches
the viewport; durable updates and visible H4 frame advances increment the
unseen count without stealing focus or follow mode. `End` resumes latest-follow.
Inspector and TurnNavigator may expose direct navigation, but the default
transcript has no permanent position rail or hover preview.
Modal geometry reserves the semantic ContextLine and Composer/HintLine
boundary on standard terminals. The reservation contracts responsively on
short terminals so at least the selected row and its actions remain visible. A
modal clears its full retained-workspace width only for the rows occupied by
its rectangle. It must not erase rows above or below, splice the ComposerDock,
or hide the command palette action row. This same-height quiet band prevents
partial words or glyphs from surviving beside the focused surface. The command
palette uses compact vertical chrome and a
fixed command column so all admitted rows fit at `160x28`. Its selected state
uses only the shared two-cell `SelectionMarker`; the command and detail remain
neutral, while every query-matching grapheme is bold. Unavailable detail keeps
its warning tone independently of selection and matching. Mono reverses the
marker, not the entire row. Truncation and fixed-column padding use terminal
display width, including CJK, emoji, and combining sequences.
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
Fenced code uses a quiet block: an optional muted language label aligned to the
content and one muted left guide, with no generic `CODE` heading, top rule, or
bottom rule. It retains the first bounded language token, expands tabs to four
cells for display, and clips by grapheme/display width with an explicit `…`;
source text remains unchanged for copy.

Agent Markdown owns physical wrapping before the outer transcript paragraph.
Its two-cell Agent gutter repeats on every physical row, including CJK, emoji,
combining text, block quotes, and code. List markers use hanging indentation:
the marker appears only on the first physical row and every continuation begins
after the complete display-width marker. Top-level heading, paragraph, list,
quote, table, and code blocks have exactly one blank rhythm row between them;
simple sibling list items remain tight. Blank rhythm rows never receive a role
gutter or caret. The live stable-prefix cache must render identically to the
same complete Markdown source, so streaming cannot change indentation or block
rhythm when a tail becomes stable.

Syntax color is a component contract, not a theme passthrough. Recognized
fenced languages map parser scopes to `normal`, `comment`, `string`,
`constant`, `keyword`, `type`, `function`, and `punctuation` semantic roles.
Those roles consume only palette styles: dark and light use polarity-safe
colors, while mono distinguishes roles with weight, italic, underline, and
muted punctuation. No state may be color-only. Unlabeled or unknown languages
render plain code and retain the left guide; unlabeled blocks omit the metadata
row. Garive never guesses a language from content.

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
H4 changes also require a continuous full-workbench snapshot with a shared
render cache; isolated live-line previews cannot establish convergence or the
absence of overlay and takeover residue.
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
- Last reviewed: 2026-09-01
- Status: accepted
