# Shared client visual and interaction system

> Normative for the canonical React UI mounted by Desktop and Web. Native
> mobile clients preserve the semantic contract while using platform-native
> components. Values not backed by Host/account facts are absent, not guessed.

## Purpose

This contract prevents feature teams from inventing local colors, spacing,
status copy or interaction behavior. New surfaces compose the tokens and
patterns below before adding a variant. A mode may change its work canvas; the
application shell, task condition, authority, capacity and evidence patterns
remain shared.

## Two-gate quality model

Shared UI changes pass two gates in order:

1. **Codex fidelity:** reproduce the observed neutral hierarchy, typography,
   navigation rhythm, reading measure, composer quality and progressive
   disclosure recorded in `docs/desktop-web-codex-fidelity-study.md`.
2. **Garive advantage:** expose durable attention, committed outcomes,
   evidence, authority, recovery and honest capacity more clearly than the
   reference without increasing ordinary-screen noise.

Gate 2 cannot waive Gate 1. Functional additions do not excuse a visually
unfinished shell.

## Principles

1. Outcome before mechanism: lead with the user's deliverable and next action.
2. Truth before reassurance: distinguish committed, pending, unknown and
   unavailable states.
3. One meaning per semantic color: cobalt is interactive/current, green is
   verified/success, amber needs attention, red failed/destructive.
4. Text always accompanies state color and iconography.
5. Progressive disclosure: ordinary work stays quiet; evidence and technical
   identities remain reachable without occupying the primary canvas.
6. Same concept, same component: Desktop and Web share markup, tokens, copy and
   keyboard behavior. Platform integration changes adapters, not semantics.

## Token contract

CSS custom properties are the executable token source. Components consume only
semantic tokens; raw values are permitted solely in the token declaration and
bounded media-query fallbacks.

### Typography

| Token | Value | Use |
|---|---|---|
| `--font-sans` | system UI stack | all product text |
| `--font-mono` | system monospace stack | IDs, versions, code |
| `--text-2xs` | 11 px | bounded timestamps and counters only |
| `--text-xs` | 12 px | labels and metadata |
| `--text-sm` | 13 px | secondary copy and navigation |
| `--text-md` | 14 px | body and controls |
| `--text-lg` | 16 px | section title |
| `--text-xl` | 22 px | page title |
| `--text-display` | clamp(28 px, 3 vw, 38 px) | empty-state outcome |
| `--document-font-size` | 13 px | desktop file/artifact body |
| `--document-leading` | 1.625 | desktop file/artifact reading rhythm |
| `--document-space` | body size ÷ 4 | Markdown vertical rhythm |

No shipping text is below 11 px. At 200% text zoom, content reflows and no
essential label is clipped or replaced by an icon.

### Space, shape, depth and motion

| Family | Tokens |
|---|---|
| space | `--space-1` 4, `--space-2` 8, `--space-3` 12, `--space-4` 16, `--space-5` 24, `--space-6` 32 px |
| radius | `--radius-control` 8, `--radius-card` 12, `--radius-panel` 14, `--radius-composer` 20/25, `--radius-pill` 999 px |
| depth | `--shadow-raised` = 8/16/-4, `--shadow-overlay` = 16/32/-8; no other shadow families |
| motion | `--motion-basic` 150 ms, `--motion-relaxed` 300 ms; exact basic, enter, exit and snappy curves |
| material | `--blur-lg` 16 px; floating transparency only |
| desktop chrome | `--height-window-bar` 34, `--height-file-toolbar` 30 px |
| conversation rhythm | `--conversation-item-gap` 16, `--conversation-grouped-item-gap` 4 px |
| navigation fade | `--sidebar-scroll-fade-distance` 40 px |

Desktop controls are at least 32 px high; primary actions and touch-capable
surfaces provide 44×44 px targets. Reduced-motion makes nonessential animation
instant. Reduced-transparency removes backdrop blur without losing separation.

Motion follows the installed Codex desktop vocabulary: ordinary fades,
scrollbar disclosure and floating overlays use 150 ms; structural panel and
drawer movement may use 300 ms. Enter is `cubic-bezier(.19, 1, .22, 1)`, exit
is `cubic-bezier(.8, 0, .4, 1)`, and compact floating surfaces use the snappy
`cubic-bezier(.23, 1, .32, 1)`. Reduced motion sets both duration sources to
zero, so compatibility aliases cannot accidentally retain animation.

Floating depth follows the installed desktop scale rather than a generic web
modal shadow. Raised controls use the 8/16/-4 profile; Environment, menus and
selectors use the 16/32/-8 profile. Both resolve to 12–19% black instead of
becoming heavier in dark mode. Translucent title and compact floating controls
use one 16px blur, including the WebKit-prefixed property required by native
macOS WebViews. Opaque docked surfaces never blur.

### Color and surface aliases

Required tokens are `--surface-canvas`, `--surface-raised`, `--surface-overlay`,
`--surface-sidebar`, `--surface-subtle`, `--surface-user-message`, `--border-subtle`, `--border-strong`, `--text-primary`,
`--text-secondary`, `--text-tertiary`, `--action-primary`,
`--action-primary-hover`, `--state-info`, `--state-success`,
`--state-attention`, `--state-danger`, and matching state surfaces.

Light, dark, increased-contrast and forced-color modes provide all aliases.
Components do not test theme names or encode their own dark palette.

The dark Desktop reference freezes effective rendered surfaces, not merely
source-layer alpha: canvas `#181818`, native sidebar `#2b2527`, selected sidebar
row `#3c3638`, Composer `#2a2a2a` and Environment/command overlay `#2d2d2d`.
A translucent source token is insufficient evidence when its actual composite
differs from these pixels.

User work is its own semantic surface: the installed desktop definition mixes
the current text color at 5% over a transparent background. It is not an alias
for the activity/progress surface. The goal rail is part of the Composer and
therefore inherits the Composer material; one subtle separator communicates
the boundary without creating a dark card stacked on another card.

## Layout contract

The supplied reference shell contains a 206 px rail at 1280 px, expanding only
to 240 px on wider windows, plus a fluid work canvas. Global navigation rows are
24–30 px, durable task rows are 26–34 px, and the title bar is 34 px. The primary thread has
a 39 rem readable measure. Environment and evidence summaries are compact
dismissible overlays; opening an artifact, file, diff or terminal creates a
separately scrollable tabbed workbench beside the thread. A generic fixed-width
Inspector is not a canonical desktop layout.

At narrower widths the workbench overlays the thread, then becomes a full
surface with an explicit Back action; the rail becomes a navigation sheet.

## Desktop composition contract

Desktop is not a browser page placed inside a window. It is one continuous
workbench made from four persistent spatial layers:

1. **Native frame layer.** The operating-system window owns traffic lights,
   resizing, full screen, system menus, focus and drag regions. Garive content
   begins in the titlebar and reserves the macOS control safe zone; it never
   paints a second branded header beneath native chrome.
2. **Navigation layer.** The rail is the quietest persistent material and owns
   global destinations, durable tasks and host identity. It uses a distinct
   neutral surface, compact rows and one selected fill. Logos, hero branding,
   large avatars and duplicate connection badges are not part of this layer.
   A chevron beside the product name is a real Runtime menu, never a disabled
   imitation of a workspace switcher. It exposes the admitted Host status and
   routes to Runtime, Workspace and general settings; unavailable routes are
   omitted. Product and Work menus share dismissal, focus restoration and
   Arrow/Home/End keyboard behavior.
   The rail footer is one 30–34 px utility row, not stacked Settings and Host
   cards. It keeps the truthful local identity and lifecycle marker together,
   opens Runtime settings from that identity, and reserves one quiet Settings
   action at the far edge. Detailed Host copy remains in the product menu.
   Its task area is a semantic stack rather than one undifferentiated feed:
   truthful needs-input, running and failed Sessions form a bounded Priority
   group ahead of completed/idle Recents. A Session appears once, empty groups
   disappear, and no pin/project hierarchy is fabricated without durable data.
   Each row ends in a 10 px lifecycle ring rather than a generic color dot:
   needs-input has a center point, running an open rotating ring, failure a
   minus, and completion a check. Accessible state copy remains authoritative;
   reduced motion freezes the running ring without hiding its open shape.
   When durable tasks overflow, only the task region scrolls beneath 40px edge
   fades. The top fade appears only after leaving the first task and the bottom
   fade disappears at the actual tail; product controls and Host identity never
   enter the mask. Forced colors removes the mask so task text stays exact.
3. **Work layer.** Conversation and Composer form one bounded work column on a
   continuous canvas. Assistant output is document content, not a card stack.
   Environment may float above this layer; it does not permanently shrink an
   idle canvas.
   Its compact fact hierarchy is Runtime, optional Workspaces, then Activity.
   Each admitted section uses 34 px rows under a quiet 20 px label; sections
   with no durable fact disappear. Git changes, branches, sources and provider
   state must never be reproduced from the reference unless Garive owns that
   exact fact. The reference-width overlay is 224 px, not a generic inspector.
   The goal rail belongs to the durable Turn, not to a transient network request:
   it remains attached to the Composer while work is running or suspended for
   approval, partial output, or external input. Suspension changes the admitted
   state to Needs input; it does not erase the goal or invent a new stage.
   An empty suspended assistant record remains an accessible status notification
   but is not painted as a duplicate pseudo-message above the same goal rail.
4. **Output layer.** Files, diffs, terminals and governed artifacts open as a
   resizable sibling pane with their own tab, location/action toolbar and
   independent scroll. This layer may replace the work layer at narrow widths,
   but it never becomes a generic Inspector card.

The four layers share one 4 px spacing basis, one neutral ramp, one 34 px
window rhythm and structural 1 px separators. Depth is used only for surfaces
that actually float: Composer, Environment, menus and modal selectors. Docked
rails, title rows and file panes use surface contrast and separators, not
shadows or rounded outer cards.

Conversation is a compact desktop document, not a feed of independent web
cards. Durable Turns are separated by 16 px; actions, terminal state and other
content that belongs to the same Turn use the 4 px grouped rhythm. Assistant
output remains flush with the reading axis, while a user request alone may use
the bounded neutral bubble. Compact density may reduce inter-Turn rhythm to the
grouped token, but it must not change document typography or hide state.

### Desktop material and window behavior

- The canvas is opaque and stable while content scrolls. The title row may use
  a restrained translucent mix only when its text contrast remains unchanged;
  reduced-transparency resolves it to the opaque canvas token.
- The sidebar remains visually behind the canvas in light and dark themes. A
  selected row changes surface, never text size, layout or accent saturation.
- Window resize preserves the active task, draft, scroll owner and selected
  output. Pane resizing is independent of browser zoom and persists only a
  bounded, non-sensitive width preference.
- Desktop restores the main native window's monitor-valid position, size,
  maximized state and full-screen state. It is created hidden and shown after
  restoration to avoid a centered-to-restored flash. Visibility is used only
  for that startup handoff; decorations remain immutable product configuration,
  and no window-state command is delegated to the Web client.
- The whole 34 px title row participates in native window dragging except
  admitted controls, tabs and the resizer. Tauri 2.11 Desktop uses its deep
  drag-region mode so non-interactive title descendants retain the native hit
  target while buttons remain interactive. Double-click follows the platform
  titlebar convention rather than triggering a product action.
- Native macOS traffic-light clearance is a platform adapter concern. Web uses
  the same React tree, geometry and semantics without reserving that inset.
- System focus, increased contrast, reduced motion and reduced transparency are
  first-class desktop states, not optional themes.
- Environment follows the desktop workbench hierarchy observed in the supplied
  reference: its trailing header action is Add context when Workspace support
  is admitted, not a duplicate close glyph. It invokes the same governed
  Workspace picker as Composer and disables while a Turn or suspension freezes
  next-Turn context. The persistent title-bar toggle remains the close action.

### Scroll surfaces

- Every independent scroll owner uses one overlay grammar across Desktop and
  Web: the track and thumb are transparent at rest, then the thumb appears on
  pointer hover or keyboard focus within that owner.
- The interaction lane is 10 px. A 3 px transparent thumb border leaves a quiet
  4 px rounded visible thumb using the strong border token; the track never
  becomes an opaque rail.
- Layouts that require a stable gutter may retain it, but the reserved space
  must use the surrounding surface. Keyboard scrolling, focus order and scroll
  semantics never depend on the visual thumb being present.
- Live output follows the tail only while the reader remains within the shared
  attachment threshold. Leaving that threshold preserves the reading position
  and reveals one explicit return control; new output changes its label and
  unread marker without moving the document.
- Return-to-latest uses a real smooth scroll unless the OS requests reduced
  motion. The control remains visible until scroll events confirm attachment;
  clicking it never fabricates the attached state before the viewport arrives.
- The return control is anchored to the variable-height Composer wrapper, 25 px
  above its leading edge. It must not use a fixed viewport-bottom offset, so
  approval, suspension and attached-workspace rows cannot overlap it.

### File-document typography

Rendered Markdown in the conversation and output layers uses the installed
Codex desktop density as the Gate 1 baseline: 13 px body, 1.625 leading and a
3.25 px rhythm.
Heading scales are 1.5×, 1.25× and 1.125× for h1–h3. The first heading begins
4 px below the 30 px file toolbar and 24 px from the pane edge. Lists, quotes,
tables and code align to the same reading edge; they do not introduce a nested
card measure.

Assistant Markdown uses the same body, leading, heading ratios and block rhythm
as the file document. Its measure and ownership differ, not its typography.
User prompts remain bounded work-prompt surfaces and use their independently
observed 14 px message text; navigation and Composer controls retain the shared
UI type scale. This prevents the thread from drifting into a looser web-article
system while the adjacent file uses compact desktop typography.

Code in a rendered file remains part of the document canvas. Its language sits
at the upper trailing edge, exact-source Copy appears on hover or keyboard
focus, and the source itself scrolls horizontally when required. Conversation
code remains a bounded code workbench because it has a different ownership
boundary. Inline code uses the document monospace step and a subtle neutral
surface. These two code treatments must not be conflated.

The persistent hierarchy is rail → top bar → work canvas → composer/action
area. Blocking approval appears immediately above the action area. Connection
and capacity warnings never cover a blocking approval.

Conversation content has no permanent top shadow. Once its scroll owner moves
more than 1 px from the top, a pointer-transparent 16 px surface-to-transparent
fade appears directly beneath the title row; it disappears again at the top.
The state is shared by Desktop and Web, carries no accessibility semantics and
respects the global reduced-motion contract.

A blocking approval is a compact permission rail attached to the Composer, not
a saturated warning card or a second dialog. It uses a neutral raised surface,
one 2 px attention edge and a 24 px authority glyph. Exact scope, duration and
overwrite behavior precede the consequence copy and one-shot actions. At the
39 rem measure those facts remain on one line and the rail stays under 100 px;
at narrow widths consequence and actions stack without horizontal overflow.
The first safe action receives focus, but the Composer must not add a second
action-colored focus ring around that focused button.

Window chrome follows permanence, not feature ownership. The top bar contains
only the current document/task identity and admitted global actions. Live
execution state appears once, attached to the composer/status rail. Durable
task state remains in its rail row. Local host identity and readiness occupy a
non-interactive rail-footer block unless a real account or host menu is
available. Placeholder initials, logo buttons and inert account menus are
forbidden. A completed or active thread exposes one quiet 30px text action for
real Markdown export; it does not label a local download as cloud sharing.
Below 760px the visible label collapses while the accessible name remains.
Web preserves this desktop composition without reserving the native
traffic-light safe zone.

When a file is open, the workbench has two quiet chrome rows: the first owns one
selected file tab and the second owns bounded location plus file actions. The
active file tab is capped at 120 px, ellipsizes its name, and owns its adjacent
close action; it does not send preview closure to an unrelated far-edge panel
button. It
must not retain an unrelated Activity tab. Rendered and Source views consume the
same immutable preview payload, preserve revision evidence, and never imply
filesystem mutability. Closing the preview returns to deliverables; closing the
workbench returns to the undisturbed thread.

The file toolbar exposes only implemented capabilities. Source/Rendered changes
presentation of the verified payload. Export copy consumes the existing native
one-shot destination capability and never overwrites. Its success or error
receipt appears progressively in the preview layer. `revealable` alone does not
permit a Finder/Open affordance until a backend reveal command is implemented.

Completed-result actions sit at the assistant content edge. Their visual form
is a 30 px icon control; their accessible name and tooltip retain the full verb.
The terminal state remains text-visible and is not replaced by color or an icon.

The resting composer begins with one text row and grows only with entered or
restored content. Its 39 rem measure, 32 px actions and compact live-status rail
form one surface. Running work replaces Send with exactly one circular Stop
action; it never shows a second text Stop beside a busy Send control. The
durability note remains programmatically associated with the field but does not
consume the ordinary visual baseline.

Fenced output is a workbench block, not an anonymous tinted rectangle. It owns
one 32px header with the admitted language or “Plain text”, one accessible Copy
action, and a separately scrollable source body. Copy uses the exact rendered
code text and gives bounded success feedback. Completed-result controls remain
quiet until the Turn is hovered or contains keyboard focus; terminal text stays
available at rest.

Neutral surface changes precede borders. A region may have one structural
separator; rows inside it use spacing or hover surfaces unless a semantic
boundary requires a rule. Navigation groups use sentence case. Decorative
gradients are forbidden in the application shell.

First-run and reconfiguration remain inside that shell. They use the same
39 rem work axis, canvas, title hierarchy, 34–36 px controls and quiet
separators as Work. A setup route must not introduce a logo, marketing eyebrow,
ambient glow, decorative gradient, floating hero card or a second radius/shadow
system. Connect, Review and Restart are one compact progressive sequence;
review facts are continuous rows rather than a dashboard of cards. At narrow
widths the fields become one column while every label and action remains text
visible.

The Workspace file picker is a compact desktop selector, not a branded hero
modal. At the wide reference it is at most 620 px with a 48 px title row, 34 px
location row, 40–42 px entries and a 54 px authority/action footer. The header
contains only document identity and close. File/folder glyphs are quiet 24 px
line icons without colored tiles; the Workspace path, 8-item selection bound
and UTF-8 authority note remain text-visible. Selection and directory navigation
use neutral hover surfaces. At 480 px the selector keeps a 12 px viewport inset
and exact document width without becoming a separate mobile visual language.

### Fidelity geometry

| Element | Comfortable target | Compact target |
|---|---:|---:|
| rail | 240–275 px | 72 px collapsed |
| title bar | 34 px reference shell | unchanged |
| global navigation row | 24–30 px | unchanged |
| durable task row | 26–34 px | unchanged |
| readable turn measure | 39 rem / 546 px at the 14 px root | unchanged |
| composer | 39 rem / 546 px at the 14 px root | pane width in split mode |
| Environment overlay | 224 px reference | 224 px viewport overlay below 1120 px |
| artifact/file workbench | 320–520 px conversation split, 352 px default | full overlay below 1120 px |

Localization and 200% text zoom may grow rows and must never clip content.

The multiline Composer uses a 20px fallback radius and a 25px
`superellipse(1.5)` radius where supported. It has no physical border. Light
depth is `0 0 0 1px / 4%`, `0 2px 8px / 4%` and `0 4px 80px 8px / 2.4%`;
dark depth is one 20% white inset pixel. Composer depth is a dedicated token and
must not inherit the stronger card or overlay shadow family.

The unified Desktop/Web window bar is 34px at the supplied 1280px reference
size. The file tab shares that row, its location toolbar is 30px including its
separator, and the first rendered heading begins after a 4px document inset.
An existing Work exposes one 24px title-proximate overflow action before the
far-edge actions. It opens a compact desktop menu containing only real routes
or commands; it is not a decorative ellipsis. The menu takes initial focus,
closes on Escape or outside pointer input, and returns Escape focus to its
trigger without changing the active Work.
Disclosure buttons in the window bar use `aria-expanded` and `aria-controls`;
the visible panel is the opened-state feedback, so the button does not retain a
selected fill or pointer focus ring. Keyboard focus remains explicitly visible.
The document retains a left-aligned 46rem measure rather than recentring inside
the pane. Opening Environment uses a bounded top-right fade/scale; file contents use a 4px lateral reveal after the
grid track exists. Both animations are removed by reduced-motion preference.
Only one rail destination is selected: Work on an empty canvas, otherwise the
active durable task. A parent Work route and its child task never highlight
together. New work, Work, Search, Agents and admitted Memory occupy one compact
global navigation group; durable task groups follow. A generic “Library” group
must not split those global routes or create empty visual hierarchy.

The file workbench divider is a native-feeling desktop affordance, not a fixed
layout accident. Pointer drag and Left/Right/Home/End keys change the bounded
conversation split, expose separator semantics and persist only the resulting
non-sensitive appearance value; double-click restores 352px. Content and Composer are clipped to that split;
neither may paint beneath the document. In the reference-sized split, Turn and
Composer share a 10px pane inset; the rendered document aligns to the
workbench's 24px content edge instead of recentering a second nested measure.

Settings is a desktop preference workbench, not a scrolling dashboard. Wide
windows use one 164px category rail and one independently scrolling detail
surface; below 760px the category rail becomes a horizontal, keyboard-reachable
strip. Only the selected category is mounted. Settings and Command-comma open
General; a truthful Capacity trigger opens Usage directly and disappears while
Settings is visible. Appearance and language may share General, while Usage,
Workspace, Runtime, Updates and Privacy remain separate categories when their
backing capability exists. Unavailable categories are absent rather than empty.

New Work is a work entry, not a marketing landing page. Heading, guidance,
Composer and starters share the 39rem work axis. The heading is 28px; guidance
is one operational line; starters are at most three 40px command rows and never
large cards. An Environment action is absent until a durable Turn exists.
Typing or choosing a starter removes the entire starter list and returns focus
to the Composer without moving it. At every window width the Composer and first
starter row have at least 12px visual separation; no overlay may satisfy the
viewport-width gate while occluding interactive content.

Committed user requests are bounded work prompts, not messenger bubbles. On
the 39rem thread they occupy at most 70%, use four continuous 22px corners and
never add a speech-tail corner. Their text remains the shared 14px base size;
long requests wrap within the bound instead of widening the reading measure.

Durable Search is a desktop work finder, not a second landing page. It keeps the
39rem work axis, a 22px orientation heading, one compact field/filter surface
and 44px result rows. State is visible at the row edge and color remains
secondary. Search never introduces hero copy, card stacks or a second command
vocabulary. On native macOS, hiding the rail or entering the narrow navigation
sheet reserves the titlebar traffic-light safe zone; Web renders the identical
work finder without that platform inset.

Agents is a catalogue workbench, not a gallery of personas or marketing cards.
Its left pane lists only Host-reported immutable Agent definitions, default
identity and durable Session usage; selecting one replaces the right detail
pane without navigation or page reload. Revision and default status remain
visible, while exact capability names are progressively disclosed. The client
never invents display names, expertise, quality claims or availability from a
definition ID. Loading, empty and unavailable catalogues remain distinct.
Wide windows use a continuous 224px list/detail split with one structural
separator; below 760px the catalogue becomes a horizontal strip above the
detail surface. Rounded full-height dashboard cards are forbidden.

Secondary surfaces use one desktop-document entry grammar. Search, Agents and
Settings begin with the direct task title on the same vertical work axis;
descriptive copy is optional and appears once. Marketing eyebrows such as
platform labels, install provenance or product slogans are forbidden above
page titles. Facts such as definition count stay adjacent to the content they
qualify instead of becoming decoration.

An empty state remains on the continuous canvas unless a card represents a
real semantic boundary, such as a control group, permission boundary or
independently navigable object. Empty-state glyph, title and consequence copy
must not acquire a tinted panel, independent radius or shadow merely to occupy
space. The same rule applies in light and dark themes and at narrow widths.

## Component grammar

| Component | Required states | Invariants |
|---|---|---|
| Button | default, hover, focus, pressed, disabled, busy | one primary action per decision region; icon-only requires accessible name |
| Field | empty, filled, focus, invalid, disabled | label persists; placeholder is never the label |
| Badge | neutral, info, success, attention, danger | short noun/adjective plus semantic color; never action-shaped |
| Task row | ready, active, needs-input, failed, completed, selected | same status copy and order in rail/search/command center |
| Notice | info, success, attention, error | states consequence and next action; alert only when interruption is required |
| Modal | entering, active, exiting | traps focus, Escape closes when safe, returns focus to invoker |
| Artifact card | preparing, ready, unverified, verified, unavailable | name/type/revision/verification visible; authority actions explicit |
| Workspace tab | opening, active, background, changed, unavailable | stable title, close action, independent scroll and focus restoration |
| Environment overlay | closed, open, attention, unavailable | compact summary only; never reserves an empty permanent column |
| Capacity | unavailable, normal, watch, critical, exhausted | scope, period, attribution, remaining/reset and continuation policy travel together |

## Capacity view contract

`UsageBudgetView` is a presentation value, not a billing calculation:

```text
UsageBudgetView {
  source: included_plan | workspace_credits | provider_api | execution
  state: normal | watch | critical | exhausted
  scope_label
  period_label
  remaining_percent?       // integer 0..100, trusted source only
  resets_at_label?
  attribution: reported | estimated
  model_posture_label?     // qualitative unless price facts are supplied
  active_turn_may_finish
  detail_destination?      // admitted internal route or safe external URL
}
```

An unavailable value is represented by absence of `UsageBudgetView`; the app
does not render a zero balance. Estimated values say “Estimated” next to the
number. A meter has an accessible name and text equivalent. `watch`, `critical`
and `exhausted` come from the trusted source; clients do not infer billing
policy from arbitrary local thresholds.

Capacity is independent of durable execution state. Reaching a plan limit does
not mark a running task failed, stopped or cancelled. If
`active_turn_may_finish` is true, the notice explicitly says current work may
finish and applies restrictions only to starting subsequent work. Recovery
actions can include switching to an efficient model, using credits, opening
usage details, or selecting provider API billing, but only when the backing
product admits those actions.

## Interaction rules

- `Command-K` opens the global command center; `Command-F` opens durable work
  search; `Command-,` opens Settings.
- Enter activates the focused action; Space toggles controls; arrow keys move
  within menus, tabs and segmented controls according to platform convention.
- Background events do not steal focus. Required approval focuses its first
  safe action; destructive confirmation never receives default focus.
- Optimistic styling cannot imply durable success. Busy disables duplicate
  submission while keeping cancellation/reconnection actions reachable.
- Empty, loading, error, offline, partial and permission-denied states use the
  same component geometry to prevent layout jumps.

## Content rules

Use sentence case and verbs for actions. “Usage”, “capacity”, “credits”,
“tokens” and “task budget” are not synonyms. Never display “unlimited” unless
the trusted source also names fair-use and task-budget boundaries. Never claim
an exact task price from prompt length, selected model, or prior averages.

## Acceptance

- Token declarations are the only source of palette, type scale, canonical
  radii, depth and motion values for new shared components.
- Desktop and Web render the same Capacity component from the same view value.
- Unit tests cover normal, critical, exhausted and absent capacity plus
  accessible meter text.
- Visual evidence covers light/dark at 1440 px and a 720 px narrow window.
- Native macOS uses the real decorated window with an overlay/hidden titlebar,
  traffic lights at `(16, 16)`, first-mouse acceptance, and a 58 px protected
  leading zone before sidebar history controls. Web uses the same content and
  interaction geometry without reserving that native chrome zone.
- Review evidence records Gate 1 fidelity and Gate 2 advantage; “looks modern”
  is not an acceptance result.
- Keyboard-only, reduced-motion, increased-contrast and 200% text matrices stay
  usable. Web captures prove shared presentation only; native macOS integration
  retains its separate evidence matrix.
