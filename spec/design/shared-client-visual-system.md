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

## Source-to-capability workflow

Codex source is a design reference; Garive capability is the product model.
Every shared client change records three distinct layers:

1. **Source fact:** exact installed bundle, module, selector/token and value.
   Screenshots may validate composition but never replace source evidence.
2. **Capability mapping:** the Garive Session, Turn, Workspace, authority,
   memory or artifact behavior that needs the pattern. Unsupported Codex
   capabilities are not rendered as decorative or inert imitations.
3. **Garive decision:** any necessary adaptation, with its reason and truthful
   state semantics. It must use the same shared visual grammar.

Small adjustments are product work. A 4px control-size error, a mismatched
radius or a duplicate status label is fixed when it changes rhythm, focus or
comprehension. Each reusable correction must also lower the cost of the next
iteration:

- add or change a semantic token instead of scattering a raw value;
- keep Desktop and Web on the canonical React component and stylesheet;
- freeze source-backed geometry in the visual contract test;
- verify computed geometry at desktop and narrow widths with zero overflow;
- record source facts and Garive adaptations in the fidelity study.

Exceptions require a named platform constraint and a bounded selector or
adapter. Product names, model names, capacity, authority and execution state
always come from Garive facts, never from the reference UI.

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
| radius | `--radius-control` 8, `--radius-card` 12, `--radius-panel` 14, `--radius-composer` 24, `--radius-composer-single-line` 22, `--radius-composer-rail` 20, `--radius-pill` 999 px |
| depth | `--shadow-raised` = 8/16/-4, `--shadow-overlay` = 16/32/-8; no other shadow families |
| motion | `--motion-basic` 150 ms, `--motion-relaxed` 300 ms; exact basic, enter, exit and snappy curves |
| material | `--blur-lg` 16 px; floating transparency only |
| desktop chrome | `--height-toolbar` 46, `--height-toolbar-sm` 36, `--height-toolbar-pane` 40 px |
| conversation rhythm | `--conversation-item-gap` 16, `--conversation-grouped-item-gap` 4 px |
| thread entry | `--thread-content-top-inset` 32 px |
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

Docked workbench disclosure follows the installed panel contract, not an
opacity-only imitation. Existing panels transition `flex-grow` and `max-width`
for 300 ms with the basic curve, so the thread releases space while the output
pane acquires it. Panel content may add the shared 4px reveal, but cannot replace
the structural transition. Pointer resizing sets one `panel-dragging` state
that removes both transitions until capture ends; keyboard resizing remains
bounded and progressive. Reduced motion resolves the structural duration to
zero through the same token.

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
for the activity/progress surface. The goal rail uses the dedicated
`--surface-composer-action-bar`: 3% black in light mode and 8% white in dark
mode. It is an attached sibling of the Composer, never a transparent internal
row or a second message card.

## Layout contract

The installed shell defines a resizable rail clamped to 240–520 px with a
275 px preferred width, plus a fluid work canvas. Global navigation rows are
24–30 px, durable task rows are 26–34 px, and the main toolbar is 46 px. The
primary thread has a 672 px readable measure, equal to Codex's 42 rem at its
16 px root. Environment and evidence summaries are compact
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
   Environment floats above this layer and never resizes the work surface. At
   a 1096–1535 px viewport, opening it translates the conversation, Composer
   and Goal rail together by -158 px; below that range it overlays without a
   content shift, and above it the natural right gutter absorbs the panel.
   Its compact fact hierarchy is Runtime, optional Workspaces, then Activity.
   Each admitted section uses 34 px rows under a quiet 20 px label; sections
   with no durable fact disappear. Git changes, branches, sources and provider
   state must never be reproduced from the reference unless Garive owns that
   exact fact. Its source width is 300 px, not a generic inspector width.
   The goal rail belongs to the durable Turn, not to a transient network request:
   it remains attached to the Composer while work is running or suspended for
   approval, partial output, or external input. Suspension changes the admitted
   state to Needs input; it does not erase the goal or invent a new stage.
   In the ordinary active state it is one quiet summary line: neutral indicator,
   goal label, ellipsized objective, then edge controls. Activity state remains
   available to assistive technology and the Environment drill-in rather than
   repeating `Running` beside an already active indicator. Only an admitted
   attention state may add semantic color and visible state text. Elapsed time,
   token usage, or budget appears only when the Host owns that exact fact.
   An empty suspended assistant record remains an accessible status notification
   but is not painted as a duplicate pseudo-message above the same goal rail.
4. **Output layer.** Files, diffs, terminals and governed artifacts open as a
   resizable sibling pane with their own tab, location/action toolbar and
   independent scroll. This layer may replace the work layer at narrow widths,
   but it never becomes a generic Inspector card.

The four layers share one 4 px spacing basis, one neutral ramp, a 46/40 px
window/pane rhythm and structural 1 px separators. Depth is used only for surfaces
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

- The production macOS window is a transparent native surface backed by the
  semantic `menu` material. Its material follows the active-window state; the
  HTML root and shell remain transparent, the canvas stays opaque, and only
  the navigation rail exposes the native material. Web keeps an opaque shell
  and never simulates macOS vibrancy with a full-window blur.
- The native rail uses the installed shell's 70% surface-tertiary mix. This
  resolves to 28% `#ededed` in light mode and 70% `#212121` in dark mode before
  native compositing. Reduced transparency replaces the mix with the existing
  opaque sidebar token without changing geometry or state hierarchy.
- The first-run native frame is 1280×820 px with a 480×600 px minimum. Restored
  monitor-valid bounds still outrank these defaults after the first launch.
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
- The whole 46 px title row participates in native window dragging except
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
- Live output follows the tail while the reader remains within the shared 24 px
  attachment threshold. Only wheel, direct pointer/touch scrolling, or platform
  scrolling keys may change that follow state; layout and scripted scroll
  events are system events, not reader intent.
- Every thread scroll owner disables browser `overflow-anchor`. Before measured
  content or Composer geometry changes, it records scroll height and distance
  from the tail. On the next animation frame it stays attached or restores that
  distance. A detached reader is never moved by new output.
- Return-to-latest uses a real smooth scroll unless the OS requests reduced
  motion. Activating it deliberately returns the controller to follow mode and
  clears the unread marker while the viewport moves to the exact tail.
- The return control is anchored to the variable-height Composer wrapper, 25 px
  above its leading edge. It must not use a fixed viewport-bottom offset, so
  approval, suspension and attached-workspace rows cannot overlap it.

### File-document typography

Rendered Markdown in the conversation and output layers uses the installed
Codex desktop density as the Gate 1 baseline: 13 px body, 1.625 leading and a
3.25 px rhythm.
Heading scales are 1.5×, 1.25× and 1.125× for h1–h3. The first heading begins
4 px below the 40 px file toolbar and 24 px from the pane edge. Lists, quotes,
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
672 px measure those facts remain on one line and the rail stays under 100 px;
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

New Work begins with one adaptive row and grows only when entered/restored
content fails measured single-line admission. An existing thread always uses a
separate input row and footer, including when its pane narrows beside an
Artifact. Its 672 px measure, 28 px actions and compact live-status rail form
one surface. Running work replaces Send with exactly one circular Stop action;
it never shows a second text Stop beside a busy Send control. The durability
note remains programmatically associated with the field but does not consume
the ordinary visual baseline.

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
672 px work axis, canvas, title hierarchy, 34–36 px controls and quiet
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
| rail | 240–520 px, 275 px preferred and persisted | 72 px collapsed |
| title bar | 46 px main / 36 px compact chrome | unchanged |
| global navigation row | 24–30 px | unchanged |
| durable task row | 26–34 px | unchanged |
| readable turn measure | 42 rem / 672 px at the Codex 16 px root | unchanged |
| composer | 672 px | pane width in split mode |
| Environment overlay | 300 px source width; -158 px content shift at 1096–1535 px | 300 px bounded viewport overlay below 1096 px |
| artifact/file workbench | 320–520 px conversation split, 352 px default | full overlay below 1120 px |

Localization and 200% text zoom may grow rows and must never clip content.

The desktop multiline Composer uses a 24px radius and the
`superellipse(1.5)` corner shape where supported. It has no physical border. Light
depth is `0 0 0 1px / 4%`, `0 2px 8px / 4%` and `0 4px 80px 8px / 2.4%`;
dark depth is one 20% white inset pixel. Composer depth is a dedicated token and
must not inherit the stronger card or overlay shadow family.

Composer submit, stop and circular context actions are 28px. The multiline
input has a 12px inline inset; its footer uses an 8px inline and bottom inset
with a 5px gap between inline controls. These values come from the installed
Codex desktop bundle's Composer layout and action tokens.

The New Work Composer uses `--height-composer-single-line` 44px and
`--radius-composer-single-line` 22px. Its explicit mode is
`auto-single-line`; it admits the compact row only when all of the following
are true:

- no running Turn rail, suspension, approval, Workspace attachment or selected
  next-Turn context requires vertical disclosure;
- the draft contains one semantic line;
- measured draft width plus a 32px reserve fits the measured input lane.

An existing thread has explicit `multiline` mode regardless of empty draft or
pane width. Leading authority/context controls, the input and the trailing
primary action occupy one adaptive row only on New Work. Any failed auto-mode
condition restores the multiline layout. The transition uses the
relaxed/snappy token and resolves to zero under reduced motion. Desktop and Web
must use the same pure admission function and DOM; platform adapters may not
decide layout independently.

The Composer is a named `composer-footer` inline-size container. Secondary
execution-scope text remains visible at ordinary width, then is visually
clipped at 440 px on Web and 480 px on Desktop while its icon and accessible
text remain. The rule responds to the Composer itself, not the viewport, so a
resized Artifact split receives the same progression. It must not use
`display:none`, remove the status from the accessibility tree, or abbreviate
the admitted fact into an invented label.

The running/suspended Turn uses a source-backed attached Utility Rail. Its
container is inset 13px from both Composer edges, tucks 4px beneath the
Composer, and uses 20px top corners with square hidden lower corners. The rail
has a 32px content row and one-line ellipsis for the Goal. Its visible facts are
only Garive's durable Goal, admitted Activity state and needs-input condition;
elapsed time, model selection and execution controls remain absent until a
trusted product capability supplies them. Reduced motion sets its attachment
transition to zero through `--motion-relaxed`.

The unified Desktop/Web main toolbar is 46px; compact chrome uses 36px. Opening
a workspace pane switches both the thread header and file tab row to the shared
40px pane rhythm; the semantic `--height-active-thread-toolbar` token owns the
transition so resize handles and responsive overlays cannot drift. Thread
actions remain fully labelled to assistive technology but become 30px uniform
icon controls in the constrained split. The file location toolbar is 40px including its
separator, and the first rendered heading begins after a 4px document inset.
An existing Work exposes one 24px title-proximate overflow action before the
far-edge actions. It opens a compact desktop menu containing only real routes
or commands; it is not a decorative ellipsis. The menu takes initial focus,
closes on Escape or outside pointer input, and returns Escape focus to its
trigger without changing the active Work.
Disclosure buttons in the window bar use `aria-expanded` and `aria-controls`;
the visible panel is the opened-state feedback, so the button does not retain a
selected fill or pointer focus ring. Keyboard focus remains explicitly visible.
The document retains a left-aligned 40rem Garive measure rather than recentring inside
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
The installed module resolves the inline content margin to 16px and block
padding to 10px. Desktop and Web consume `--user-chat-width`,
`--thread-content-margin` and `--radius-user-message` from the same stylesheet;
narrow split panes keep the ratio rather than inventing a mobile bubble.

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
| Tooltip | hidden, delayed-hover, immediate-focus | shared 11 px overlay material; names the action, keeps an optional shortcut separate and never relies on native `title` chrome |
| Field | empty, filled, focus, invalid, disabled | label persists; placeholder is never the label |
| Badge | neutral, info, success, attention, danger | short noun/adjective plus semantic color; never action-shaped |
| Task row | ready, active, needs-input, failed, completed, selected | same status copy and order in rail/search/command center |
| Notice | info, success, attention, error | states consequence and next action; alert only when interruption is required |
| Modal | entering, active, exiting | traps focus, Escape closes when safe, returns focus to invoker |
| Artifact card | preparing, ready, unverified, verified, unavailable | name/type/revision/verification visible; authority actions explicit |
| Workspace tab | opening, active, background, changed, unavailable | stable title, close action, independent scroll and focus restoration |
| Environment overlay | closed, open, attention, unavailable | compact summary only; never resizes the work surface; overlay/shift/gutter modes use the source 1096/1536 px thresholds |
| Capacity | unavailable, normal, watch, critical, exhausted | scope, period, attribution, remaining/reset and continuation policy travel together |
| Turn activity | active, completed, attention | uses only admitted per-Turn Activity; active starts expanded, completed starts collapsed |
| Result actions | hidden, hover, focus, attention | 20 px row, 6 px top gap, 2 px action gap; ordinary actions disclose on turn hover/focus |
| Workbench tab close | hidden, hover, focus-within, focused | absolutely overlaid at inline end; pointer-inert and opacity zero at rest; title uses a 1 rem logical-end fade while disclosed |
| User Turn | short, measured-long, expanded, hover, focus | 70% bubble, 22 px radius, 16 px inset, 10 px vertical padding; actions belong to the full-width right-aligned Turn |
| Thread scroll owner | attached, detached, unread, returning | 24 px threshold; only user intent detaches; layout changes preserve distance; browser anchoring is off |

The full-width User Turn wrapper is always transparent. Only
`.user-message-bubble` may consume `--surface-user-message`; dark and light
themes must not paint the alignment/action wrapper as a card.

High-frequency icon actions use the shared Tooltip primitive. Pointer hover has
a 350ms disclosure delay to avoid noise while crossing a toolbar; keyboard
focus discloses immediately. The trigger keeps its independent `aria-label` and
references the semantic `role=tooltip` description, including an optional
shortcut. Top, bottom, right and start/center/end placement belong to the
primitive so pane code never reimplements edge collision by hand. Reduced
motion resolves its transition duration through the global motion tokens. A
disabled action enters the Tab order only when the primitive is explicitly
given a truthful unavailability explanation; the inner disabled control then
leaves the accessibility tree and the wrapper owns `aria-disabled` semantics.

Shell controls, action-menu triggers, code actions and Home starter descriptions
use that same primitive. A menu trigger suppresses its Tooltip while
`aria-expanded=true`; the open menu is the only disclosure surface. Starter
descriptions may wrap within the 260px Tooltip bound and open above the row so a
full-width starter cannot push the disclosure beyond the viewport edge.

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
- The global command center is a 520px desktop palette, not a generic modal.
  Its list is capped by `min(440px, max(120px, 90vh - 64px))`, leaving one
  64px chrome budget. The root owns a 4px inset, compact 32px action rows and
  40px two-line Work rows. Icons occupy one neutral 20px lane without colored
  tiles; items rest at 75% opacity and become fully legible on hover/focus.
  Arrow Up/Down wrap through commands, Home/End jump to boundaries, Escape
  closes, returns focus to the invoker and Tab remains trapped. Shortcuts stay on their owning command;
  there is no persistent instructional footer.
- Enter activates the focused action; Space toggles controls; arrow keys move
  within menus, tabs and segmented controls according to platform convention.
- Background events do not steal focus. Required approval focuses its first
  safe action; destructive confirmation never receives default focus.
- Optimistic styling cannot imply durable success. Busy disables duplicate
  submission while keeping cancellation/reconnection actions reachable.
- Empty, loading, error, offline, partial and permission-denied states use the
  same component geometry to prevent layout jumps.
- Routine Host recovery is not an onboarding or access splash. It uses one
  centered live-status line with a 6 px progress pulse, keeps the native drag
  surface available, and renders no product mark, promotional headline or
  card. The descriptive title remains in the accessibility tree. Reduced
  motion disables the pulse animation.
- Home outcome starters use the shared suggestion-list contract: 40 px rows,
  20 px semantic-icon lane, one truncated 13 px task label, 6 px gap, rounded
  hover/focus disclosure and 0.99 pressed scale. Category wording belongs in
  the accessible name/tooltip when the visible task title is sufficient; rows
  do not become a separator table or add a decorative trailing arrow.
- A completed Turn compresses Activity into a turn-local label summary. Its
  completion label is visually hidden but announced; failure, interruption and
  required-input labels remain visible. Expanded Activity enters from −8 px at
  220 ms and respects reduced motion.
- Assistant actions use one stable 20 px turn-control row with 6 px top space
  and the Electron −4 px optical offset. The action group, not terminal
  evidence, owns progressive opacity. Copy is first; admitted product actions
  follow. Timestamp stays absent until a durable sent-time fact exists.
- The thread footer exposes `data-thread-scroll-footer` and is measured with a
  resize observer. Timeline bottom padding is the current footer height plus a
  16 px safe gap, never a state-specific constant. System resize may preserve
  tail attachment or reading distance; the first user movement away cancels a
  pending footer correction so layout cannot steal scroll ownership.
- Collapse state may be local to the mounted Turn. Clients must not claim
  cross-session persistence until Runtime or preferences admit that contract.
- User requests collapse only after rendered measurement exceeds 20 lines.
  Collapsed content reserves 19 lines plus a separate ellipsis line; the
  in-bubble toggle exposes `aria-expanded`. Copy is disclosed on Turn
  hover/focus. Timestamp and Edit stay absent without admitted product facts
  and mutation semantics.
- Composer attachments use one shared presence rail. Each item exposes
  `present|exiting`, placement and variant attributes; a concealed item remains
  mounted for its grid-row transition but is `aria-hidden`, inert and exactly
  zero-height afterward. Goal progress uses the default variant and the static
  source target glyph. The multiline input floor is 44 px, its maximum is
  `25dvh`, and the action row remains a separate 36 px footer.
- A split workbench uses Codex AppShell's adaptive content-pane calculation
  until the operator resizes it. The regular main surface reserves at least
  352 px and prefers 500 px; the content pane keeps a 320 px minimum and uses
  the source 600 px / 640 px / 1.6×-height candidates. Pointer or keyboard
  resize persists an explicit bounded value, while double-click restores
  `adaptive`. At 480 px the selected Artifact is a full-width single panel;
  the covered Work surface is inert and `aria-hidden`, never a clipped sliver.
- A thread with fewer than four user messages has no navigation rail. At four
  or more, Desktop and Web render one `User messages` landmark against real
  Timeline anchors. Direct buttons are 36×10 px; their 26×2 px markers rest at
  progress 0.2308 and expand through 1/0.7/0.4/0.2 focus, hover and scrub
  proximity. Current state comes from visible message intersections, not a
  decorative scroll percentage. Activation moves the message to block start,
  highlights its bubble, exits tail following and leaves return-to-latest
  reachable. Native button semantics own Enter/Space; focus exposes the same
  aligned preview as pointer hover.

## Content rules

Use sentence case and verbs for actions. “Usage”, “capacity”, “credits”,
“tokens” and “task budget” are not synonyms. Never display “unlimited” unless
the trusted source also names fair-use and task-budget boundaries. Never claim
an exact task price from prompt length, selected model, or prior averages.

## Acceptance

- Token declarations are the only source of palette, type scale, canonical
  radii, depth and motion values for new shared components.
- Shared typography uses only semantic Normal 400, Medium 500, Semibold 600 and
  Bold 700 tokens. Component CSS must not introduce an arbitrary numeric
  `font-weight`; exceptional source utilities require a separate evidence and
  contract change. Chat copy remains 13 px at 1.625 leading.
- Desktop and Web render the same Capacity component from the same view value.
- Unit tests cover normal, critical, exhausted and absent capacity plus
  accessible meter text.
- Visual evidence covers light/dark at 1440 px and a 720 px narrow window.
- Native macOS uses the real decorated window with an overlay/hidden titlebar,
  traffic lights at `(16, 16)`, first-mouse acceptance, transparent WebView,
  active-state `menu` material and a 58 px protected leading zone before
  sidebar history controls. Web uses the same content and interaction geometry
  without reserving that native chrome zone or applying native material.
- Review evidence records Gate 1 fidelity and Gate 2 advantage; “looks modern”
  is not an acceptance result.
- Keyboard-only, reduced-motion, increased-contrast and 200% text matrices stay
  usable. Web captures prove shared presentation only; native macOS integration
  retains its separate evidence matrix.
- Long-thread evidence covers the hidden-under-four threshold, exact rail
  geometry and opacity, pointer proximity, focus preview, anchor landing,
  drag scrubbing, reduced motion and return-to-latest recovery.
