# Desktop and Web Codex fidelity study

> Reviewed 2026-09-01. This is evidence and design reasoning. The normative
> contract lives in `spec/design/shared-client-visual-system.md`.

## Objective

Garive's first visual gate is fidelity, not originality. The shared Desktop and
Web UI must reproduce the calm density, hierarchy, navigation rhythm, composer
quality and progressive disclosure visible in the current ChatGPT/Codex work
surface. Only after that gate passes may Garive add a stronger outcome and
evidence model.

“Fidelity” does not mean copying source code, trademarks or private assets. It
means independently implementing observable layout and interaction qualities
from recorded evidence. Garive keeps its own name, icon, semantics and Runtime
truth model.

## Evidence set

### A. Current installed product

The inspected signed local application is:

- bundle name: `ChatGPT`
- bundle identifier: `com.openai.codex`
- version: `26.825.51511` (`7377`)
- packaged `app.asar` SHA-256:
  `f56ac8d5254a10fc4a04e7417fa787d135c3bbca49bad7d668d4ae65833d40c7`

The application safety boundary does not permit automated capture of the Codex
window itself. No private account content was inspected. Read-only static token
and bundled-module inspection of the installed package established these facts:

| Family | Observed values |
|---|---|
| spacing | 4 px base |
| type | 11, 12, 14, 16 and 28 px core steps |
| radii | 4, 6, 8, 10, 12, 16 and 20 px |
| navigation rows | 30 and 36 px variants |
| desktop toolbar | 46 px; 36 px compact variant |
| sidebar | `clamp(240px, 275px preferred, min(520px, 100vw - 320px))` |
| thread measure | 40 rem default, with 42/48 rem and 480/500 px variants |
| composer | thread measure plus 24 px inline overhang; 4 px base spacing |
| conversation rhythm | 16 px between conversation items; 4 px inside grouped items |
| floating material | 16 px large blur; 8/16/-4 raised and 16/32/-8 overlay shadows |
| motion | 150 ms basic, 300 ms relaxed; `.19,1,.22,1` enter, `.8,0,.4,1` exit, `.23,1,.32,1` snappy |
| neutral ramp | `#fff`, `#f9f9f9`, `#ededed`, `#cdcdcd`, `#afafaf`, `#414141`, `#303030`, `#212121`, `#181818`, `#0d0d0d` |

The bundle also separates modules for the local conversation thread and turn
entries, scroll layout and virtualizer, application chrome, message navigation
rail, composer utility bar, task suggestions, terminal panel, worktree
environment, text-file tabs and artifact tabs. This confirms that Codex is a
desktop workbench, not one chat column plus a generic inspector. These values
and module boundaries are evidence, not a license to import code or assets.

Its executable root tokens expose `--conversation-item-gap: 16px` and
`--conversation-grouped-item-gap: 4px`. Before this audit, Garive's final CSS
override used a 30 px bottom margin on every message, including the last item,
which made long work read like a sparse web feed. The shared Desktop/Web shell
now applies 16 px only between durable Turns and reuses 4 px for state/actions
that belong to one Turn.

The supplied Environment reference uses its trailing header slot for `+`, not
for a second close button. Garive maps that visual hierarchy to the existing
governed Workspace picker: it is enabled only when next-Turn context may be
changed, while the persistent title-bar inspector toggle owns dismissal. This
copies the desktop composition without fabricating an add-source capability.

### B. Official OpenAI UI material

The official Features page renders a Codex sidebar and Add menu with real
component structure rather than a marketing-only illustration. It establishes:

- one quiet neutral rail for New chat, Scheduled, Plugins, Sites and PRs;
- selected rows expressed by a subtle surface, not an accent block;
- Pinned, Projects and Chats as short semantic groups;
- a contextual Add surface that combines files, browser, goals, plan mode and
  plugins without permanently crowding navigation;
- restrained icon scale, short labels and descriptions only where they help a
  choice.

The official Web and Projects documentation establishes the product hierarchy:
Chat is conversational exploration, Work carries a task to a reviewable result,
and Codex exposes developer execution detail. Projects retain related chats,
files and sources. The shell therefore cannot treat every object as an
undifferentiated chat bubble.

Primary references:

- [Features](https://learn.chatgpt.com/docs/features)
- [Use ChatGPT](https://learn.chatgpt.com/docs/use-chatgpt)
- [ChatGPT on the web](https://learn.chatgpt.com/docs/web)
- [Projects and chats](https://learn.chatgpt.com/docs/projects)
- [What's new](https://developers.openai.com/codex/whats-new)

### C. Garive live baseline

`docs/evidence/desktop-M16-real-api-restart-2026-08-31.png` is the live native
Desktop baseline. A 1440 × 900 shared-UI visual fixture was also reviewed. The
same defects appear on Desktop and Web because both mount the same React UI:

1. 8–10 px metadata dominates important surfaces and becomes visual noise.
2. Warm yellow-gray surfaces make the app look muddy beside neutral system UI.
3. Borders divide almost every region, row and card, flattening hierarchy.
4. The rail, work canvas and inspector use different density systems.
5. Empty canvas area is large, but outcome, progress and next action are weak.
6. The composer is physically large while its controls and authority copy are
   tiny, so it feels heavy and low-confidence at the same time.
7. Blue acts as decoration, selection, information and action rather than one
   controlled interaction signal.
8. The useful artifact inspector visually reads like a debug sidebar.

### D. Current signed-in Codex desktop screenshots

Two user-supplied 1280 × 800 screenshots were reviewed at original resolution.
They contain private task names and therefore are not copied into repository
evidence. Observable product facts are recorded without that content:

1. The ordinary thread is a continuous document. Assistant Markdown has no
   surrounding card; turn actions stay quiet at the content edge.
2. The composer is a bottom-floating desktop surface. Its live goal/status rail,
   input, authority, model/run-location controls and stop action form one unit.
3. Environment is a compact floating panel over the thread, not a permanently
   allocated evidence column.
4. Opening a file creates a real split workbench: conversation narrows on the
   left and the file owns a larger tabbed reading surface on the right.
5. The workbench has tab chrome, breadcrumbs, view-source/open actions and
   independent scrolling. It is not an expanded artifact card.
6. Long-running immersion comes from continuous content, anchored turn actions,
   live composer status and adjacent outputs—not a large progress dashboard.
7. Desktop chrome allocates information by permanence: the title bar carries
   document identity and global actions, execution state stays attached to the
   composer, and account/host identity sits at the rail footer.
8. The macOS window reads as one workbench because native controls, rail,
   title/tab chrome, overlay panels and the bottom composer share one compact
   density. A browser header with a logo, status badge and avatar does not.

### E. 2026-09-01 workbench gap closure

The supplied 1280 × 800 references and an equal-size Garive Web render were
placed side by side. The comparison produced concrete component changes rather
than a general “more polished” direction:

| Surface | Reference evidence | Rejected Garive baseline | Required shared behavior |
|---|---|---|---|
| window/file chrome | 34 px unified window/tab row, 30 px location row, first document heading at y≈69 | 42 px window row + 38 px location row + 28 px inset placed the first heading at y=108 | 34 px window row, 30 px location row, 4 px document inset and left-aligned 46 rem measure |
| rail hierarchy | one selected task under compact global routes | Work and its current task selected together; Agents/Memory isolated beneath a generic Library label | one selected route, real global destinations together, durable tasks below |
| narrow thread title | one line in the split pane | wrapped to three lines | preserve one-line orientation and elide before status/actions |
| terminal actions | quiet glyphs at the content edge | wide `Export .md` and `Copy` labels | 30 px icon controls with persistent accessible names |
| composer | approximately 545 px wide, one-line input start and one circular stop action | 490 px wide, two-line empty input, duplicate text stop plus spinner and a visible disclaimer | 39 rem / 546 px measure at the 14 px root, one-row start, 28 px actions and one truthful stop control |
| fenced output | bounded code card with a language header and copy action | undifferentiated bordered `pre` block | labeled code workbench, exact-source copy and persistent accessible feedback |
| split workbench geometry | conversation, Composer and file divider share one bounded column; document starts 24 px after its edge | 352 px grid column whose child painted at 586 px, sending the 538 px Composer 210 px beneath the file surface; document added a second centered 40 px inset | explicit zero-min grid track, clipped conversation surface, 352 px default/320–520 px persisted separator and one 24 px document inset |
| Environment | named floating surface | Activity/Artifacts tabs inside a small card | one Environment heading and a dismiss action; evidence rows only |
| desktop identity | account identity anchored at the rail footer; no avatar in the title bar | duplicate Local/Working badge plus an inert G account button | truthful host identity in the rail footer; no fake account or duplicated execution state |
| file chrome | selected file tab plus a second location/action row | generic Inspector tabs plus an expanded artifact card | one selected file tab, breadcrumb row, source/render action and independent scroll |
| source inspection | available beside rendered document | no source presentation | reversible Rendered/Source switch over the same immutable preview bytes |

Garive deliberately keeps verified revision evidence and the explicit close
action. Those are Gate 2 additions: they do not add ordinary-screen noise and
they preserve the Runtime truth boundary.

### F. Desktop preferences audit

The original Settings surface remained a generic scrolling web page after the
Work shell had reached the reference geometry. At 1280×800 it exposed every
appearance, language, usage, update, Runtime, Workspace and privacy card at
once, repeated Capacity in the title bar and the page body, and forced users to
scan unrelated backend detail to change one preference.

The shared client now applies the desktop workbench grammar to preferences: a
164px category rail and one independently scrolling detail surface on wide
windows, horizontal category navigation below 760px, and direct routing from
the Capacity trigger to its single truthful view. Settings/Command-comma opens
General instead. This is a Garive consistency correction based on the observed
Codex density and progressive-disclosure model; the supplied Codex screenshots
do not show its Settings screen, so this section does not claim pixel evidence
for an unseen reference surface.

### G. New Work entry audit

The original empty canvas still read as a generic AI landing page: a 38px
heading occupied a 760px measure, three starter prompts spread into marketing
cards across that width, the Composer used a different 546px axis, and an empty
Environment button was visible before a Session existed. At 720px and 480px,
an intermediate layout placed the Composer 20px over the first starter row;
viewport-width checks alone did not catch that occlusion.

New Work now uses one 39rem desktop work axis for heading, one-line guidance,
Composer and three command-style starter rows. The heading is 28px, starters
are 40px rows instead of cards, and Environment remains absent until durable
work exists. Typing or choosing a starter immediately removes all starters and
focuses the Composer. Measured at 1280×800, heading/Composer/starters share
x=470 and width=546; at 720px and 480px the Composer and starters retain a
12px vertical gap with zero document overflow.

### H. Native shell and durable Search audit

The shared shell previously declared macOS traffic-light insets in CSS without
marking the Desktop document as the native client. The signed app therefore did
not admit its own platform branch, and collapsing the rail could place the
restore control beneath native window controls. Desktop now declares its client
identity at the entry point, reserves the 70px titlebar safe zone when the rail
is absent, and gives compact rails or narrow navigation sheets a traffic-light
row with no competing controls. Web keeps the same work geometry without the
native inset.

Search also no longer reads as a generic page of cards. It is a 39rem desktop
finder with one compact field/filter surface and 44px state-first result rows.
At 1280, 820, 720 and 480px it retains zero document overflow; state copy
remains visible until the narrowest layout, where the task rail and row dot
still preserve condition without crowding the work title.

### I. Installed Agents workbench audit

The prior Agents destination exposed one static capability ID inside a large
generic card and ignored the durable multi-Agent catalogue already projected by
the product controller. It could neither distinguish the default Agent nor
show immutable revision, exact capabilities or actual Session usage.

Agents now consumes only the Host-reported definition and Session projections.
At 1280px it uses a continuous 224px catalogue/detail split rather than a
stretched dashboard card; at 720px and 480px the catalogue becomes a horizontal
strip above an independently scrolling detail surface. Exact revision and
default status stay visible, capabilities remain behind one disclosure, and no
persona name or expertise claim is inferred. All three measured widths retain
zero document overflow.

### J. Running-work immersion and desktop chrome audit

The running fixture still kept a permanent Command-K search capsule in the
title row and disabled the Composer textarea. The first choice made global
navigation compete with document identity; the second forced a desktop user to
wait before even drafting the next instruction.

The title row now carries only a quiet route/task icon, one-line identity,
truthful capacity when available and the contextual inspector action. Command-K
remains global and Search remains in the rail, but neither occupies permanent
work chrome. The admitted Activity is compressed into the Composer status rail
with an icon-only details action and one stop control. While a Turn runs, users
may edit a persisted next-instruction draft; Return cannot submit it until the
current Turn reaches an admitted terminal state. This is drafting, not an
invented queue, and it does not mutate the active Runtime Turn.

### K. Measured desktop geometry and file-workbench lifecycle

The supplied screenshots and Garive fixtures were measured at the same
1280 × 800 viewport. With Environment open, Garive's 546 px conversation axis
starts at x=352 and its 224 px panel starts at x=1044; the reference starts at
approximately x=352 and x=1045. In the file surface, Garive's divider starts at
x=558 while the reference starts at approximately x=556. These measurements
rule out a permanent left-shift of the ordinary thread: the apparent shift is
the intended response to an open overlay or split workbench.

A read-only audit of the installed Codex desktop bundle also confirms the
underlying density model: a 4 px spacing unit, 30–36 px navigation rows, 36–46
px toolbars, 150 ms basic transitions, a 40 rem content measure, `#181818` dark
surface and low-contrast 5–12% borders. Garive's shared tokens already occupy
that range, so the correction is structural rather than a decorative retheme.

The remaining structural defect was two visually identical close actions in
one file workbench. Garive now exposes one progressive action: while a file is
open it closes the preview back to Deliverables; only at that parent layer does
the same header position close the inspector. The secondary toolbar close was
removed. This preserves both operations while making the current layer
unambiguous.

### L. First-run desktop continuity audit

The Work surface had reached the reference density while first run still
opened a 750 px floating card with a multicolor top rule, two ambient radial
gradients, a 24 px radius, a separate shadow family and a blue marketing
eyebrow. At 1280 × 800 its content started at x=368 instead of the established
x=470 work axis. The result looked like a setup website embedded inside a
desktop application.

Setup now stays in the same rail/title shell and uses the 39 rem axis, neutral
canvas, 24 px heading, 36 px controls, quiet three-step rail and continuous
review rows. The logo/eyebrow treatment, gradient light field, colored top rule,
hero card, backdrop blur and separate shadow/radius system are absent. The
Connect, Review and Restart states were rendered and inspected; 720 px and
480 px stable states retain exact viewport width and reflow the fields to one
column. This is a consistency correction based on the observed desktop grammar,
not a claim that the supplied screenshots showed Codex onboarding.

### M. Approval-at-the-action-boundary audit

The approval fixture exposed a high-value Garive surface that still used the
old dashboard language. A full amber card occupied 136 px inside the Composer;
its three authority facts competed with two inline actions, and autofocus drew
both a button ring and a second blue ring around the entire Composer. The
permission boundary was truthful but visually louder and harder to scan than
the task it protected.

The approval is now a two-column permission rail with a 24 px authority glyph,
neutral raised surface, one 2 px attention edge and a full-width fact region.
Scope, duration and overwrite behavior stay on one line at the 39 rem measure;
consequence copy precedes Decline and Approve once. Its 720 px fixture is 96 px
high instead of 136 px. At 480 px the consequence/actions stack to 159 px,
retain exact text and keep document width equal to the viewport. Only the safe
Decline action receives the explicit focus ring; the surrounding Composer uses
a quiet neutral focus border. This is the Gate 2 authority advantage without
adding ordinary-screen noise.

### N. Workspace picker desktop-density audit

The platform-only Workspace picker remained a 680 px hero modal with an 82 px
header, blue marketing eyebrow, 37 px colored Workspace tile, 31 px colored
file tiles, 51 px entries, 70 px footer, 20 px radius, inset highlight and an
independent 100 px shadow. An 8 px full-window blur made the underlying work
feel like another application rather than retained context.

The picker is now a neutral 620 px desktop selector using the shared panel
radius and overlay shadow. Its title/location/entry/footer rows are
48/34/40–42/54 px; the header contains one title and close action, while file
glyphs are quiet 24 px line icons without tiles. Directory enter/back, the
0–8 count, UTF-8 authority copy and confirm actions remain continuously
visible. A real interaction entered `Research notes`, returned to the root,
selected `Launch brief.md`, updated the count to 1/8 and confirmed the exact
context back into the Composer. At 720 px the sheet remains 620 px wide; at
480 px it uses a 12 px inset and 456 px width. Both stable states have zero
document overflow. This improves desktop density without weakening Workspace
authority or pretending the Web client owns the native picker capability.

### O. Secondary-surface continuity audit

Search, Agents and Settings had drifted into three unrelated page grammars.
Search placed a large tinted rectangle behind an otherwise small empty state;
Agents and Settings used product-marketing eyebrows above their task titles;
their first content baselines therefore started at different heights. None of
those decorations communicated Runtime state, authority or a next action.

The three surfaces now share a quiet desktop-document entry: the direct title
starts at the 74 px baseline in the 800 px reference viewport, optional
description follows once, and structural content begins with one separator.
Search keeps its field/filter region as the only semantic control surface, but
its zero-result state and dark results canvas are transparent. Agents and
Settings no longer render `Installed locally` or `Desktop` as marketing
eyebrows; definition count and settings categories remain where they carry
meaning. Browser verification at 1280, 720 and 480 px found zero document or
main-content horizontal overflow on all three screens. This audit applies the
desktop grammar inferred from the supplied Codex workbench references; those
references do not depict these exact secondary screens.

### P. Goal-first running rail audit

The reference running Composer leads with the goal being pursued and keeps
operational controls at the edge. Garive's equivalent geometry already matched
the reference, but its visible copy said only `Working on your task` followed
by the latest tool label. That made implementation activity more prominent than
the user's outcome and repeated the same tool fact available in Environment.

The 32 px rail now leads with `Pursuing goal`, projects the latest admitted user
request as a single ellipsized line, and keeps the details action at the edge.
Exact tool labels and the current Activity state remain available to assistive
technology and in the Environment drill-in. With Environment open at the
reference viewport, the rail remains x=353,
544 px wide and free of internal overflow. At 720 and 480 px the Composer is
546 and 432 px wide respectively, with zero rail or document overflow.

The installed `26.825.51511` source makes the hierarchy explicit. Its active
summary string is `composer.threadGoal.summary.active` (`Pursuing goal`); the
row icon is `icon-2xs text-text-tertiary/70`; the status title is `text-default`;
the objective is a single `truncate-text text-codex-description` continuation;
and trusted time or token usage follows as tabular `text-codex-description`.
The row is mounted through the generic Composer rail and uses
`--color-background-composer-action-bar`, defined as 3% black in light mode and
8% white in dark mode. Garive therefore keeps its active indicator neutral and
does not repeat `Running` as a competing visual label. It does not invent the
Codex timer or token budget because Garive's Host projection currently exposes
neither fact. `Needs input` remains a visible attention exception.

### Q. Quiet titlebar disclosure audit

Opening Environment previously left a prominent blue focus outline around the
titlebar toggle in pointer-driven screenshots and also applied a persistent
selected background. The panel itself already communicates the open state, so
the duplicate emphasis made one utility action compete with document identity.

The toggle now exposes the real `aria-expanded` and `aria-controls` relationship
without an `active` class. Pointer release removes incidental button focus; once
the pointer leaves, the open toggle has a fully transparent background while
Environment remains visible. Keyboard focus still renders the shared 2 px
focus-visible outline. At 720 and 480 px the 224 px panel remains present with
the same expanded semantics and zero document overflow. This follows the
reference titlebar's quiet utility-action hierarchy without hiding state from
assistive technology.

### R. Installed Codex user-request geometry audit

The installed ChatGPT/Codex bundle, version `26.825.51511` build `7377`, was
inspected directly from its signed local `app.asar`; no screenshot inference or
network copy was used for this measurement. Its conversation-block stylesheet
sets user chat width to 70%, spacing to 0.25rem, and the temporary bubble radius
to `5.5 × spacing`—22 px—with a continuously round corner shape. Its shared
type tokens also confirm 11/12/14/16 px for xs/sm/base/lg.

Garive still allowed user requests to occupy 82% of the 39rem thread and used a
5 px lower-right tail corner. That produced a conventional messenger bubble
inside an otherwise document-like work surface. User requests now use the
resource-backed 70% bound and four continuous 22 px corners while retaining
Garive's existing 14 px base text and outcome-first alignment. The extracted
bundle is evidence only and is not redistributed in the repository.

### S. Installed Codex Composer depth audit

The same installed bundle exposes the full Composer surface contract. In
`app-initial-NNCUNt29.css`, `_ComposerLayoutRoot_ut334_2` maps a multiline
default Composer to `radius-3xl`; the Electron token override resolves that
radius to 24 px. `corner-shape: superellipse(1.5)` changes the curve, not the
radius. The root explicitly forces `border: 0`. Light elevation is a 4% one-pixel ring, a
4% 2×8 px lift and a 2.4% 4×80 px ambient shadow; dark elevation is only a 20%
white one-pixel inset.

Garive previously used a 16 px round corner, a 15% dark border and a 28% black
18×50 px dark shadow. It now uses the resource-backed 24 px Electron radius,
superellipse where supported, zero physical border and independent Composer
elevation tokens. Computed dark output is a 24 px superellipse with a
20% white inset; computed light output matches all three source shadow layers.
After the control correction, measured New Work is 66 px high and Running is
99 px at 1280×720. The 1280×720 Desktop/Web fixtures and the 480 px Web fixture
have zero internal and document overflow.

The same module defines `--spacing-token-button-composer` as seven 4 px spacing
units, and the submit/stop implementation applies `size-token-button-composer`
with `rounded-full`. Garive therefore uses 28 px circular Composer actions. Its
multiline input and footer follow the source layout's 12 px input inset, 8 px
footer inset/bottom space and 5 px inline-control gap. These values are bundle
facts; Garive's textarea implementation remains a product-specific native
control rather than a claim of matching Codex's ProseMirror internals.

#### Progressive single-line admission

The installed `app-initial-B6Gk5KCN.js` contains the complete admission rule,
not merely an animation. `auto-single-line` is rejected when attachments are
visible, the editor contains more than one block or a newline, or voice layout
is active. Otherwise the measured text must fit the measured input width with
a 32px reserve. Its paired CSS resolves the compact root to 44px high, the
input row to 36px and the single-line radius token to 22px; the adaptive footer
places leading controls, input and trailing controls in one grid row.

Garive maps “expanded capability” to facts it actually owns: an executing Turn
and its progress rail, a suspension/approval, an attached Workspace or selected
next-Turn context. With none of those present, the shared client measures the
rendered draft against the real remaining input width and applies the same 32px
reserve. Newline and capability checks remain semantic; no character-count
heuristic is used. This is an independent textarea implementation of the source
state machine, not a claim that Garive embeds Codex's ProseMirror editor.

Measured Desktop and Web output at 1280×720 is identical: empty and short-draft
states resolve to 672×44px, 22px radius and 28px actions; a long draft expands
to 66px, while Running remains 99px and Approval 253.34px. At 480px the compact
Composer is 432×44px with a 235.45px measured input lane; a draft that no longer
fits expands to 66px. Every state has zero document overflow.

#### Attached Utility Rail

The installed source also shows why an internally divided Composer still felt
flatter than Codex. In `app-initial-NNCUNt29.css`, `_rail_e8rl5_1` applies the
13px `--home-composer-inline-inset`; `_attached_e8rl5_26` tucks a present item
4px beneath the Composer and gives an above-placed rail its own top corners.
`_ComposerHomeUtilityBar_dqhd9_4` uses the dedicated Composer action-bar
surface and 4px inline padding. In `app-initial-B6Gk5KCN.js`, `Xro` identifies
the item as a `controls` Utility Bar with an approximately 32px content row.
The pinned bundle resolves the Electron top radius to 20px and the action-bar
surface to 3% black in light mode or 8% white in dark mode.

Garive previously put `TurnProgress` inside `.composer` as a transparent row
with a bottom border. It now shares a `.composer-stack` on Desktop and Web:
the rail is the Composer's sibling, has the source insets/material/tuck, and
passes placement/variant state through explicit data attributes. Garive keeps
its own admitted Goal, Activity and needs-input facts. It does not invent the
reference's elapsed timer, model control or execution actions because the Host
projection does not supply those capabilities.

### T. Installed Codex desktop document-system audit

The user clarified that fidelity must cover desktop composition, not only UX
flows. The second supplied reference and the installed bundle were therefore
audited as a desktop file workbench: native window rhythm, rail material,
docked pane chrome, document canvas and Markdown typography were treated as one
system rather than independent component styling.

The installed Markdown module sets the desktop host fallback to 13 px body and
12 px code, uses 1.625 leading, and derives a 3.25 px spacing unit from one
quarter of the body size. Heading scales are 1.5×/1.25×/1.125× with 26 px and
22.75 px line boxes for h1 and h2/h3. Paragraphs, lists, quotes, rules, tables,
inline code and fenced code all consume that same rhythm. This is direct
resource evidence from version `26.825.51511`, not a visual estimate.

Garive's file pane previously used a 14 px/1.7 body, a 25 px h1 and sparse
26/10 px h2 margins. A fenced block also became a large tinted rounded card,
which made a rendered source file look like an embedded webpage. The shared
Desktop/Web tree now uses the resource-backed 13 px document scale, formulaic
heading and block rhythm, a 24 px left reading edge and a plain document code
region with the language at its trailing edge. Exact-source Copy remains
keyboard reachable and discloses visually on hover/focus.

The chrome values are executable tokens rather than scattered constants:
34 px window/tab row and 30 px file toolbar. They sit beside explicit document
tokens in the shared visual system, while native traffic-light clearance and
window dragging remain desktop-adapter behavior. At 1280 px the live fixture
measures x=558 for the pane divider, x=583 for document content, y=68 for h1,
19.5 px h1 text, 13 px body and 21.125 px body line height, with zero document
overflow. The same tree remains the Web implementation; only native window
insets differ.

### U. Installed Codex dark workbench and thread-density audit

A real dark Garive fixture was rendered at the supplied reference width for
both split-file and running-with-Environment states. Its structural geometry is
now within the reference: 546 px ordinary conversation/Composer axis, 224 px
Environment, x=558 file divider and a split Composer that starts at x=216 and
reaches the divider at 342 px wide. The split visual fixture now combines a
committed artifact with a real running follow-up, exercising the 97 px goal
rail/Composer rather than comparing an easier terminal state to the supplied
running reference.

The installed Electron theme establishes `#181818` canvas, `#dfdfdf` primary
text, 70% white secondary text, 50% white tertiary text, 8%/4%/16% border
levels and a 3% white dark Composer surface with the previously audited 20%
inset. Garive now uses those source values for the canvas, text hierarchy and
Composer while retaining the observed native sidebar material from the user
reference. Computed output is `rgb(24,24,24)`, `rgb(223,223,223)`, exact 70% and
50% text alpha, and 3% Composer surface alpha.

The same bundle shows that Electron conversation Markdown, not only file
Markdown, resolves through the 13 px/1.625 root. Garive's assistant output was
still 14 px/1.72 with 24/18 px headings, creating a web-article rhythm beside
the newly aligned file. Assistant Markdown now shares the resource-backed
13 px body, formulaic 19.5/16.25/14.625 px headings, paragraph/list spacing and
16.25 px fenced-block rhythm. User work prompts keep the separately observed
14 px message scale and 70%/22 px bubble geometry.

### V. Installed Codex scroll-surface audit

The earlier Garive build exposed a persistent, thick black WebView rail at the
conversation edge. Neither supplied Codex reference shows that rail. The
installed bundle defines normal scrollbar sliders from the 8% border token,
uses the 16% strong border for hover/active state, and includes a hover-only
utility whose track and thumb are transparent at rest. Its terminal fixes the
interaction lane at 10 px, while compact tables opt into a thin scrollbar.

Garive applies that source-backed behavior to conversation, file, settings,
Environment, command, setup and navigation scroll owners. Each uses a 10 px
transparent lane and a thumb with 3 px transparent borders, yielding a 4 px
visible rounded slider only on hover or focus-within. Existing stable-gutter
layouts keep their geometry without reintroducing an opaque track.

### VI. Native title-row hit testing

The visual geometry alone did not prove a native titlebar. Tauri's official
window-customization contract states that a normal `data-tauri-drag-region`
applies only to the element carrying it, not to descendants. Garive's title
container fills most of the 34 px row, so the former marker on the outer header
left only incidental gaps draggable. The shipping Tauri 2.11.5 runtime supports
`data-tauri-drag-region="deep"`: both sidebar and work title rows now admit all
non-interactive descendants while preserving their nested buttons. The unused,
hidden 28 px drag element was removed rather than kept as false evidence.

### VII. Native window continuity

The former Desktop always opened a centered 1180×780 frame regardless of the
operator's previous workspace. The official Tauri window-state contract saves
and restores native frame state after creation, and recommends creating the
window hidden to prevent a visible centered-to-restored jump. Garive now tracks
only the `main` window and admits size, monitor-valid position, maximized,
full-screen and startup-visible flags. `DECORATIONS` is intentionally excluded:
overlay titlebar and traffic-light composition remain immutable product
configuration. No window-state permission is granted to the frontend; the Rust
host owns automatic restore/save across normal application lifecycle events.

### VIII. Progressive conversation edge

The installed Codex stylesheet defines a 16 px main-content top fade from the
canvas surface to transparent, with opacity transitioning from zero to one only
when the owning scroll surface reports the visible state. Garive previously
kept a hard title-row/content seam for the whole task. Conversation now derives
that state from its real scroll position: more than 1 px from the top shows the
pointer-transparent fade, returning to the top removes it. The same React/CSS
state is used by Desktop and Web and adds no focusable or announced element.

### IX. Supplied-reference effective surface audit

Exact pixel extraction from both supplied 1280×802 references resolves the
same blank sidebar region to `rgb(43,37,39)` (`#2b2527`) and the continuous
canvas to `rgb(24,24,24)` (`#181818`). The selected sidebar row resolves to the
stable `#3c3638` cluster. In the running reference, Composer interior is exactly
`rgb(42,42,42)` (`#2a2a2a`) and Environment interior is exactly
`rgb(45,45,45)` (`#2d2d2d`).

Garive's former `#211f20` sidebar was materially too black and cold. More
importantly, applying the installed bundle's 3% white Composer source layer
directly over the canvas produced roughly `#1f1f1f`; that source alpha was not
the final reference composite. The shared tokens now freeze the measured
effective surfaces, while Environment/command use a dedicated neutral overlay
token rather than inheriting the warm sidebar material.

### X. Desktop task-rail grouping

Both supplied references use multiple labeled rail groups rather than one long
undifferentiated history. Garive cannot truthfully reproduce `Pinned` or
`Projects` until those facts exist in its Host contract, but it can preserve the
same scanning grammar with admitted lifecycle data. The shared rail therefore
places needs-input, running and failed Sessions in a bounded `Priority` group,
then completed and idle Sessions in `Recents`. Empty groups disappear and the
same Session cannot be repeated across groups. This keeps the Codex desktop
hierarchy while making Garive's durable attention state more useful.

### XI. Active file-tab geometry and closure

The supplied split-workbench reference places the active file tab in the first
approximately 110–120 px of the output title row, ellipsizes the long filename,
and keeps preview closure inside that tab. Garive formerly used a 166–260 px
file tab while sending the close action to the far edge of the output pane.
The shared workbench now uses a measured 120 px active tab with a 22 px close
slot. Closing it returns to Deliverables; only that parent surface restores the
far-edge workbench close. Real Browser measurements put the tab at x=575–695
and its close action at x=672–694 in the 1280 px fixture, with no body overflow
at 1280, 1120, 720 or 480 px.

### XII. Truthful top-bar thread action

Both supplied references reserve a low-noise text action in the current thread
title row. Garive has no admitted cloud-share or immutable-link service, so a
literal `Share` label would be false. The shared title row now occupies the
same visual role with a real 30 px `Export` action: it downloads an ordered
Markdown transcript containing the visible user and Garive turns while omitting
internal Session metadata. At 1280 px in the split fixture it resolves to
x=434–505 before the Environment control. At 720 px and below the label hides,
leaving a 34 px icon target and the full accessible name. Dark/light
1280/720/480 browser runs retain zero overflow.

### XIII. File-workbench action cluster

The supplied split-workbench reference keeps file actions in the preview title
row, beside the source/rendered control, instead of making users rediscover an
action on the originating card. Garive now promotes its real one-shot `Export
copy…` capability into that row and returns the chosen filename as an inline
success receipt. Source/rendered and export remain labeled at 1120 px and
above, then collapse to two 29 px icon controls at 720 px and below without
losing their accessible names.

This cluster follows a strict capability rule: `revealable` artifact metadata
does not create a Finder/Open action by itself. That control is admitted only
after a native reveal command and its failure contract exist. Browser evidence
at 1280, 1120, 720 and 480 px records zero horizontal overflow; both dark and
light themes complete the export and expose the receipt through `role=status`.

### XIV. Durable task-state rings

The supplied desktop references end task rows with a quiet outlined circle,
not a bright generic status badge. Garive already owns stronger durable facts,
so its shared rail uses the same 10 px ring grammar while retaining truthful
state: center point for needs-input, open rotating ring for running, minus for
failure and check for completion. The marker stays secondary to the accessible
text state, and reduced motion freezes rather than removes the running shape.

### XV. Goal continuity through suspension

Codex keeps the current work identity attached to the bottom Composer rather
than treating approval as an unrelated modal. Garive now binds its goal rail to
the durable Turn lifecycle: it remains present across running, approval,
partial-output and external-input suspension. Suspended work replaces the live
pulse with a static attention marker and truthful `Needs input` state while
preserving the exact goal and Activity drill-in. An empty suspended assistant
record remains available to assistive technology but no longer creates a
second visible `Needs input` message above the same Composer state.

### XVI. Environment fact hierarchy

The supplied running reference places Environment at x=1045 in a 1280 px
window and gives it an approximately 224 px width. Its density comes from
grouped desktop facts, not introductory prose. Garive formerly spent 72 px on
an explanatory header before showing only Activity, which made the surface feel
like a web card and hid the workbench hierarchy.

The shared overlay now orders only admitted facts: Runtime, optional
Workspaces, then Activity. Runtime readiness comes from the negotiated Host
capabilities, Workspace name/access/attachment comes from the real Session,
and Activity remains the durable Turn ledger. It does not invent the
reference's Git Changes, branch or Sources rows. At 1280×802 the approval
fixture measures x=1044, y=42, 224×283 px with all three sections; the running
fixture truthfully omits Workspaces and contracts to 224×214 px. Both preserve
the same 34 px row rhythm and leave the conversation/Composer axis aligned with
the supplied screenshots.

### XVII. Title-proximate Work actions

Both supplied references reserve a quiet ellipsis immediately after the active
Work title. Garive previously omitted that desktop hierarchy entirely while
placing every available action at the far edge. The shared title row now adds
a 24px `Work actions` trigger only after a durable Turn exists. Its menu exposes
real New work, Search, Environment and Settings commands with their existing
keyboard shortcuts; unavailable Search is omitted rather than simulated.

The menu is a 204×138px neutral overlay in the reference-sized split fixture,
takes focus on open, closes on Escape or outside pointer input, and restores
focus to its trigger. Real Browser measurement retains zero document overflow,
and the same DOM is mounted by Desktop and Web. Export remains the truthful
far-edge analogue to the unavailable cloud Share action instead of being
duplicated inside the menu.

### XVIII. Truthful product/Runtime menu

The supplied desktop references make the product name and chevron an active
top-left hierarchy. Garive previously copied that appearance with a disabled
button, creating a false affordance. The shared rail now opens a real Runtime
menu: its status row is derived from negotiated Host configuration, followed
by admitted Local Runtime, Workspace access and Settings routes. An unavailable
Workspace route disappears rather than rendering a dead row.

The product and Work menus now use one component for initial focus, outside
pointer dismissal, Escape focus restoration and wrapping Arrow navigation with
Home/End bounds. In the 1280px sidebar fixture the dark/light menu measures
x=8, y=62, 220×156px, resolves to the same overlay surfaces as Environment,
and retains zero document overflow. This exceeds the reference's visual cue by
making the switcher operational without inventing another Host or account.

### XIX. Single-row rail identity footer

The supplied references end the navigation rail with one compact identity and
utility row. Garive previously stacked a full Settings navigation row above a
38px two-line Host card, repeating the Runtime detail already admitted by the
new product menu and consuming roughly twice the reference height.

The shared rail footer is now one 32px row: Local identity and its real state
marker open Local Runtime settings, while a 30px far-edge Settings action opens
General. The verbose Runtime state remains in accessible naming and the product
menu rather than adding a second visible line. In the 1280×720 running fixture
the footer measures x=8, y=678, 189×32px with a 10px bottom safe inset and zero
document overflow. Compact rail mode reduces both actions to icons; the 480px
navigation drawer restores the Local label without creating a second footer.

### XX. Progressive reading and Composer anchoring

The supplied desktop references keep the conversation as a document surface:
the reader can leave the live tail without the interface pulling them back, and
the compact return affordance belongs spatially to the Composer. Garive's prior
handler requested a smooth scroll and then immediately assigned `scrollTop`,
cancelling the animation. Its fixed bottom offset also assumed one Composer
height, so approval and attached-workspace states could collide with the control.

Desktop and Web now share one tail policy. A normal return is genuinely smooth,
reduced motion moves directly, and the control stays present until the actual
scroll position crosses the attachment threshold. It is a child of the Composer
wrapper and sits 25px above that wrapper rather than above the viewport bottom.
The long running fixture exercises the same document density as the supplied
reference; unit contracts cover both motion branches and variable Composer
ownership without turning development fixture text into production state.

### XXI. Desktop material depth

The installed stylesheet provides one 16px large blur, an 8/16/-4 raised
shadow and a 16/32/-8 overlay shadow. Garive's previous material system mixed
10, 12 and 18px blur with 22–80px shadows; in dark mode Environment used 42%
black over 80px, making a compact desktop fact panel read like a web modal.

Desktop and Web now share the source-backed material scale. The 1280×720
running fixture resolves Environment to `rgba(0,0,0,.19) 0 16px 32px -8px`,
retains its exact x=1044/y=42/224px geometry and keeps document overflow at
zero. The translucent 34px title row and return-to-tail control use the same
16px blur with a WebKit-prefixed declaration for the native macOS WebView.
Docked rail, conversation and file surfaces remain opaque and shadowless.

### XXII. Progressive navigation edges

The installed Codex stylesheet gives the navigation scroll owner a 40px edge
fade instead of clipping long task lists against fixed chrome. Garive's task
region already owned scrolling, but its first and last rows met the fixed
navigation and Host footer with a hard WebView edge.

The shared rail now derives top and bottom disclosure independently from real
scroll geometry. At the 160px short-window fixture, the 208px task stack
resolves to bottom-only fade at scrollTop 0, both fades at 45 and top-only fade
at its exact 48px tail. The fixed Garive controls and Local identity remain
fully opaque, horizontal overflow stays zero, and forced colors removes both
masks. Production grouping remains bounded to three Priority and three Recent
Sessions; the constrained fixture changes only the available rail height.

### XXIII. Conversation and Composer surface hierarchy

The installed theme defines user-message background as a 5% oklab mix of the
current text color over transparency. Its attached goal rail contributes
border/tuck behavior but no independent filled surface. Garive previously
mapped both user messages and the goal rail to the same opaque `#242424`, while
the Composer beneath used `#2a2a2a`; the result looked like three unrelated
dark cards.

The shared clients now give user messages the source-backed semantic mix and
let the goal rail inherit Composer material. Its existing 8% separator remains
the only internal boundary. User work, assistant document and execution state
therefore keep distinct roles without increasing the neutral palette.
At 1280×720 the effective user surface is `rgb(41,42,39)`, Composer remains
`rgb(42,42,42)`, and the rail itself is transparent. The ordinary and split
Composer widths remain 546/342px, the file boundary stays x=556.8 and both
scenes retain zero horizontal overflow.

### XXIV. User-message geometry

The installed conversation module defines `--user-chat-width: 70%`, a 16 px
thread-content margin and a 22 px radius; its utility composition adds 10 px
vertical padding. This corrects an earlier visual estimate: the reference does
not use a 24 px inline inset. Garive already matched width, vertical padding
and radius, but its 14 px inline padding made wrapped prompts visibly denser.

Desktop and Web now resolve the source values through shared semantic tokens.
Before the change the measured ordinary/split content lanes were 546/332px,
with 357.56/232.40px bubbles and no horizontal overflow. The tokenized geometry
preserves those outer widths while increasing only the readable inset to the
exact 16 px reference value.

### XXV. Source-resolved desktop shell geometry

Direct inspection of the installed shell tokens supersedes earlier measurements
taken from the downscaled reference attachment. Codex defines a resizable
sidebar as `clamp(240px, preferred 275px, min(520px, 100vw - 320px))`, main,
compact and pane toolbars at 46/36/40px, and the primary thread at 42rem. Because
Garive intentionally uses a 14px root, copying `42rem` literally produced only
588px; the shared token therefore freezes the equivalent rendered width of
672px rather than preserving a misleading source string.

Garive now persists only the bounded 240–520px sidebar preference, migrates
older appearance records without losing admitted fields, supports pointer,
Home/End/arrow-key resizing and resets to 275px on double click. Real Browser
proves 240 and 520px endpoints survive layout with a 672px thread and zero
overflow. At the 275px default, artifact mode retains its independent 352px
thread pane, 332px content rail, 342px Composer, 40px file toolbar and zero
overflow. Desktop and Web consume the same DOM, preference and CSS contract.

### XXVI. Structural panel disclosure

The installed Codex stylesheet applies 300ms basic-curve transitions to
`flex-grow` and `max-width` on every panel, removes them under `panel-dragging`,
and disables them for reduced motion. Garive previously changed its Grid track
count synchronously and animated only the new pane's opacity/4px translation.
That made the most important spatial change feel like a web route replacement.

An initial three-track attempt was rejected by live measurement: `1fr → 352px`
is a discrete Grid change, so the conversation snapped while only the final
15px of the third track visibly animated. The admitted implementation mirrors
the source Flex contract instead. The existing thread owns animated grow and
maximum-width values, the new workbench begins at zero available width, and
both meet at 352/653px without overflow. Browser measurement observes the
closed thread at 1005px, an intermediate opening frame at 367.81px and the
352px terminal width; computed transitions remain 300ms. Component evidence
proves pointer drag adds/removes `panel-dragging`, making the same transition
duration zero only during direct manipulation.

### XXVII. Native macOS window material

The supplied references show more than React layout: the navigation rail is a
native window material beneath content, while the document canvas remains an
opaque reading surface. Inspection of installed Codex
`/Applications/ChatGPT.app` version `26.825.51511` (bundle `7377`) confirms the
main window uses a transparent background, macOS `menu` vibrancy, a hidden
inset titlebar, first-mouse acceptance and traffic lights at `(16, 16)`. Its
source default is 1280×820 px with a 480×600 px minimum.

The installed renderer keeps Electron's body transparent and composites the
left panel from 70% surface-tertiary material. The nested source tokens resolve
to 28% `#ededed` in light mode and 70% `#212121` in dark mode; the primary canvas
remains opaque `#fff`/`#181818`. This explains why an opaque tinted Garive rail
could match a screenshot color but still fail to feel like the desktop app as
the window focus and wallpaper changed.

Garive now maps the same contract onto Tauri: the native window and WebView are
transparent, Tauri applies semantic `menu` material that follows window active
state, and Desktop-only CSS exposes it through the rail while preserving an
opaque main surface. Reduced transparency restores the solid sidebar token.
The Web client retains the same DOM and panel geometry but stays fully opaque.
Browser evidence at 1280×720 resolves Desktop root/shell to transparent, its
rail to 70% `#212121`, its canvas to `#181818`, and Web rail/canvas to their
solid tokens; both remain at zero horizontal overflow.

### XXVIII. File-first workbench disclosure

The remaining pre-preview surface still behaved like a Web dashboard: one
large bordered card per file, repeated action labels, and a second large card
for every response snapshot. That intermediate page delayed the document the
user had explicitly asked to open and broke the continuous tab/file/document
hierarchy visible in the supplied split-workbench reference.

The installed source directly confirms a 40 px `h-toolbar-pane` breadcrumb
row, surface background, bottom divider, 8 px outer padding, 12 px path text,
medium active segment, and separate toolbar-sized `View source`/open actions.
Those facts—not an inferred mockup—set Garive's file chrome. The supplied
reference screenshot, rather than a claim about Codex internals, sets Garive's
narrower rendered-document wrap.

The concrete bundle evidence is in
`review-file-source-breadcrumb-BdBSz0Rs.js`,
`text-file-editor-tab-content.electron-DxCiFr1w.js`, and
`artifact-tab-content.electron-NO4CTIoa.js`; the recorded package hash above
pins the evidence to one installed build.

As a separately labelled Garive enhancement, Desktop and Web share a
file-first rule. When a Turn has exactly one
verified text artifact, opening Deliverables enters its tab and digest-checked
preview immediately. Closing the tab returns to a compact, flat file index;
multiple artifacts remain in that index so the client never guesses which file
the user intended. File rows use the same hover, type and 28 px icon grammar as
the surrounding desktop shell, while export remains a distinct action. Garive
uses a 40 rem document measure because it reproduces the supplied reference's
line wrapping at the audited viewport; this is not asserted as Codex source.

### XXIX. Source-backed split-pane toolbar rhythm

The installed `thread-app-shell-chrome-DQMbICmE.js` defines the thread workspace
toolbar as `h-toolbar-pane`, labels it “Chat toolbar”, keeps the title in a
`min-w-0 flex-1` region and registers compact trailing actions. This is direct
source evidence that a split thread uses pane chrome, not the 46px standalone
window toolbar. The supplied second reference independently shows the chat and
file pane headers aligned on one horizontal rhythm.

Garive therefore keeps 46px for an unsplit work window and resolves a shared
`--height-active-thread-toolbar` to 40px whenever the artifact workspace is
open. The same value owns the chat grid row, file tab row, divider origin and
responsive overlay offset. Export and inspector actions keep their behaviour
and accessible names while their split presentation becomes a uniform 30px
icon control. This is a Garive composition decision grounded in the cited
source geometry; it does not copy unavailable Codex action logic.

### XXVI. Turn-local progressive disclosure and result actions

Installed source evidence is explicit. `local-conversation-turn-BPH7L6Kk.js`
places a completion Activity collapsible before the assistant item and passes
persisted/forced/disabled collapse inputs. `conversation-blocks-Bqf2uxPH.js`
gives its disclosure body a `translateY(-8px)`/opacity entrance over 220 ms and
150 ms exit. The same module gives Electron assistant actions a 20 px row,
6 px top margin, 2 px gap and −4 px horizontal offset. Ordinary actions and
timestamps are fully transparent until the enclosing turn is hovered or
focus enters it.

Garive maps those facts only onto admitted product data. Each completed Turn's
real Activity list is collapsed to an exact label summary before its committed
result; an active list starts expanded. Copy, export and real artifact actions
use the compact hover/focus row. Completed state remains available to assistive
technology but is not repeated visually; failed, stopped or suspended terminal
states stay visible. Garive does not fabricate reasoning prose, elapsed time,
model choice or tool calls. Collapse state is component-local until a durable
product preference is admitted, so it is not described as persisted behavior.

### XXVII. User Turn rhythm and measured request collapse

Installed source separates the user bubble from its Turn group. In
`conversation-blocks-Bqf2uxPH.js`, the group is full-width, right-aligned and
uses a 4 px gap; Copy/Edit/timestamp actions are opacity-zero until group hover
or focus. The bubble uses 10 px vertical padding. Its companion CSS module
`conversation-blocks-BHmH6nQ0.css` resolves user width to 70%, radius to 22 px
and horizontal thread inset to 16 px.

The same source does not truncate by character count. Its measurement hook
observes rendered width, font and content height. Ordinary requests receive a
20-line budget; when collapsed, one line is reserved for a standalone ellipsis
and 19 lines remain for content. The toggle stays inside the bubble, aligned to
its content start, with a 6 px top margin and `aria-expanded` state. Hidden
interactive descendants are made inert and removed from accessibility until
visible.

Garive's request is plain text and has no interactive descendants, timestamps
or editable durable command, so it maps only the supported subset: measured
20-line collapse, in-bubble Show more/less, and a real Copy request action on
the Turn hover/focus row. Desktop and Web share the component. No send time or
Edit affordance is invented because current Host projections admit neither.

### XXVIII. Thread scroll ownership and layout anchoring

Installed source is explicit in `thread-scroll-layout-B1Q-zWDJ.js`. Its
controller uses a 24 px tail-attachment threshold and distinguishes `user`
from `system` scrolling. User intent is admitted from wheel, direct pointer or
touch scrolling, and Arrow/Page/Home/End/Space keys; programmatic scroll events
do not detach the reader. A user-direction grace window lasts 1,000 ms. Before
layout changes, the controller records `distanceFromBottomPx` and
`scrollHeightPx`, then restores that distance on the next animation frame. It
also observes the scroll owner and content, compensates footer height changes,
and disables browser `overflow-anchor`. The installed implementation uses
reverse flex and negative coordinates internally; that coordinate convention
is an implementation fact, not a visual requirement.

Garive maps the same ownership contract onto its normal positive `scrollTop`
layout. Desktop and Web share a 24 px attachment threshold, key-direction
mapping and distance-preservation helpers. Only recent user intent changes the
follow state. Request measurement, font reflow, streaming content and Composer
height changes either remain attached or retain the reader's prior distance
from the tail. Native browser anchoring is disabled so it cannot compete with
the controller. Garive does not copy reverse flex or negative coordinates
because its existing document order and accessibility semantics do not require
them.

### XXIX. Composer mode belongs to placement, not viewport width

Installed source in `app-initial-B6Gk5KCN.js` defines two product modes before
it resolves the rendered layout: `multiline` and `auto-single-line`. The Home
composer explicitly supplies `composerLayoutMode: "auto-single-line"`; the
thread composer defaults its mode to `multiline`. The `G_o`/`Y_o` measurement
path admits a single-line surface only in auto mode, with no attachments,
editor newline or voice layout, and only when measured text plus the exact
`q_o = 32` px reserve fits the measured input lane. Corresponding CSS in
`app-initial-NNCUNt29.css` gives multiline input a separate first row and a
three-column footer, while single-line uses one adaptive row. Footer labels
then progressively hide through named container queries at 440–480 px; the
Composer itself does not collapse into one row merely because a thread pane is
narrow.

Garive now exposes the same placement mode to its shared pure layout resolver.
New Work uses `auto-single-line` and retains the existing real-width/32 px fit
test. Once a durable thread contains a Turn, Desktop and Web use `multiline`,
including inside the 352 px Artifact split. Running state, suspension,
Workspace context and selected next-Turn context also require multiline
disclosure. This preserves a stable input row and authority/action footer
without inventing Codex controls or labels.

### XXX. Footer labels collapse by their own container

Installed CSS in `app-initial-NNCUNt29.css` marks responsive Composer footers
with the named `composer-footer` inline-size container. Secondary and small
labels disappear at 440 px; Electron adds the same secondary-label collapse
through 480 px. Icons and controls remain. This is independent of the viewport
breakpoint and therefore still works when an Artifact split narrows only the
thread pane. The source does not discard the underlying control or status
semantics when the visible label is removed.

Garive maps this to the admitted execution-scope label only. The shared
Composer establishes the same named inline-size container. At or below 440 px
Web clips `Local · text only`, resume, file-count or output-enabled text to the
standard accessible one-pixel representation while retaining the shield and
semantic text; Desktop uses the source-backed 480 px threshold. No model,
permission level or billing label is added. Browser evidence proves the label
visible at 672 px, visually clipped but present in the accessibility snapshot
at a 342 px Artifact split, and clipped at a 432 px Composer in a 480 px Web
viewport with zero document overflow.

### XXXI. Routine recovery and Home suggestions stay work-like

Installed Codex 26.825.51511 contains a dedicated `LoadingPage` implementation
in `app-initial-B6Gk5KCN.js`. Its layout is a transparent, centered surface with
an optional draggable toolbar region, and it explicitly accepts
`showLogo=false`; the separate access-splash component owns permission-oriented
headline, description and learn-more treatment. Routine loading therefore does
not require reusing an access, onboarding or marketing card.

The installed `home-suggestion-surface-B3IuURqZ.js` list variant uses 40 px
Electron rows, one truncated task label, an optional semantic icon, rounded
hover/focus treatment and a 0.99 active scale. It does not require a category
column, separator table or trailing chevron. Garive now uses this list contract
for its three admitted static outcome starters. Their existing prompt text is
unchanged; category text remains in each button's accessible name and tooltip
instead of consuming a permanent visual column.

Garive's routine Host recovery now renders one muted live status and a 6 px
pulse, with no SVG, product mark, large headline or card. The full title remains
available to assistive technology. Wide browser evidence measures all three
Home rows at 672x40 px with zero document overflow. Reduced motion freezes the
pulse. This is a Garive product mapping of source-supported structure, not a
claim that Codex exposes Garive's Runtime wording or starter prompts.

### XXXII. Thread entry uses the scroll-layout inset

Installed source in `thread-scroll-layout-B1Q-zWDJ.js` assigns the Electron
thread viewport `--thread-content-top-inset: calc(var(--spacing) * 8)`. Its
4px spacing unit resolves this to 32px. Garive previously hard-coded 40px, so
the first durable Turn sat eight pixels lower even though the toolbar and
content rail already followed the installed source.

Desktop and Web now consume a shared `--thread-content-top-inset: 32px` token.
It belongs to scroll layout rather than an individual page or message, so a
future thread renderer cannot silently re-estimate the entry position from a
screenshot.

### XXXIII. Toolbar actions use product-owned tooltips

Installed Codex source does not leave split-thread actions as unrelated native
browser titles. `thread-app-shell-chrome-DQMbICmE.js` registers them through
compact `HeaderToolbar.Actions`; the shared control sources repeatedly pass
`tooltipContent` through a Tooltip wrapper, including focusable explanations
for disabled controls and zero-delay contextual disclosures. These are direct
component-composition facts. They do not establish one universal delay for
every Codex surface.

Garive formerly used 22 browser-native `title` attributes, so delay, material,
placement and keyboard disclosure differed by host. Desktop and Web now share
one semantic Tooltip primitive for the high-frequency thread title, result,
Composer, progress and pane actions. It keeps the trigger's accessible name,
binds a `role=tooltip` description, supports an optional shortcut and owns
top/bottom/right plus edge alignment. Garive chooses a quiet 350ms pointer delay
and immediate focus disclosure; that timing is a documented Garive interaction
decision rather than a fabricated Codex constant.

Live Web evidence at 1280×720 verifies immediate keyboard disclosure in 20ms,
the separate `⌘⇧A` hint, removal of the native title, in-viewport placement for
thread, Composer and file-tab edges, and zero document/main/workspace overflow.

### XXXIV. Global command menu is a palette, not a modal card

The installed `app-initial-NNCUNt29.css` defines `.command-menu-dialog` at
`min(520px, 92vw)`, derives its vertical position from the available menu
height and gives the global list a 440px cap. Its root uses a 4px inset and
elevated surface; items have a 24px minimum before content padding, 75% resting
opacity, neutral hover/focus material and no visible list scrollbar. Input
padding is 6px by 10px. These are direct stylesheet facts from Codex
26.825.51511, not measurements inferred from a screenshot.

Garive's former 620px palette used 50px rows, 31px colored icon tiles, a 60px
search bar and a permanent footer. It read as a generic Web modal and exposed
only one ArrowDown transition from the input. Desktop and Web now share a
520px/4px command surface, 44px search chrome, neutral 20px icon lane, 32px
commands, 40px factual Work rows and the same source-backed adaptive list cap.
The duplicate footer and decorative chevrons are removed. Arrow Up/Down wrap
through real actions, Home/End move to boundaries, Tab remains trapped and
Escape closes without losing the underlying Work and restores its prior focus.
Live 1280×720 evidence measures a 520px palette at y=108, 44px search chrome,
32px action rows, 20px icons, 75% resting opacity and zero overflow. The same
run proves Down, wrapping Up, Home and End order plus Escape returning focus to
the running thread's next-instruction draft.

### XXXV. Shell disclosure uses one Tooltip system

Installed `app-initial-B6Gk5KCN.js` composes the Electron sidebar trigger with
an explicit Tooltip whose copy is “Toggle sidebar”, while keeping separate
accessible “Hide sidebar” and “Show sidebar” labels. The same source resolves a
275px default and clamps the rail from 240px to 520px. These facts also confirm
that Garive's stable sidebar geometry was already correct; a transparent
Desktop material rendered in an ordinary browser is not valid Web evidence.

Garive now routes the shell collapse/search/settings controls, action-menu
triggers, Workspace detach, code copy and Home starter disclosure through the
shared Tooltip primitive instead of browser-native `title`. Opening a menu
suppresses its trigger Tooltip so the two overlays never compete. Long starter
descriptions wrap within 260px above their row; their category remains available
without adding a permanent visual column. Live Web evidence preserves all three
starter rows at 672×40px, shows immediate keyboard disclosure fully inside a
1280×720 viewport, proves the action menu replaces its Tooltip on open and
reports zero horizontal overflow.

### XXXVI. A user Turn is not a full-width dark slab

Installed `app-initial-B6Gk5KCN.js` composes the user-message surface from
`bg-user-message`, `max-w-(--user-chat-width)`, horizontal
`--thread-content-margin`, `py-2.5` and a continuous rounded bubble. This is the
rendered component contract behind the already recorded 70%, 16px, 10px and
22px tokens. The full-width Turn wrapper exists only to right-align the bubble
and its progressive actions.

Garive's inner bubble satisfied that contract, but an older dark-theme rule
still painted the direct `.user-turn` wrapper `#292a27`. It visually expanded
every request into a square, pane-wide card and hid the correct 70% bubble
hierarchy. The legacy fill is removed and an executable CSS gate forbids its
return. Live 1280×720 split-Workbench evidence measures a transparent 332px
Turn wrapper and a 232.4px bubble (70%) with 10×16px padding, 22px radius and
zero horizontal overflow.

### XXXVII. Runtime state color is a semantic contract

Installed `app-initial-NNCUNt29.css` defines success, warning and danger as
theme-resolved foreground/surface variables and consumes them through utility
classes such as `bg-success-solid`, `bg-warning-surface` and
`bg-danger-surface`. This is source evidence for semantic indirection; it does
not authorize copying Codex's product-specific status wording into Garive.

Garive already owned equivalent state tokens, but recovery errors, update
failures, capacity warnings and artifact export receipts still carried separate
light/dark literals. These paths now consume the shared success, attention and
danger foreground/surface pairs. The obsolete setup-logo CSS is also removed;
the component contract and visual test already established that routine setup
has no logo-led heading. A CSS regression gate prevents theme-specific status
literals from returning.

Live Web settings evidence verifies that the same capacity-attention chip
resolves to `rgb(230,184,111)` on `rgb(71,53,29)` in dark mode and
`rgb(153,96,12)` on `rgb(255,243,220)` in light mode, with zero horizontal
overflow at 1280×720. Desktop and Web receive the change from the same visual
system rather than maintaining client-specific theme patches.

### XXXVIII. Only the conversation may scroll the thread viewport

Installed `thread-scroll-layout-B1Q-zWDJ.js` gives the actual thread element
`overflow-y-auto`, disables browser anchoring there and owns the Composer as an
absolute footer inside a `relative h-full` shell. Its inner content clips only
the horizontal axis with `overflow-x-clip`. This structure assigns scroll
state to one explicit controller instead of letting an outer work surface
become a second programmatic scroll container.

Garive's `.work-surface` used `overflow: hidden`. Although visually clipped,
that value still allowed focus/scroll-into-view behavior to set an outer
`scrollTop`. Opening an Activity while Environment was visible moved the whole
conversation and Composer upward by 238px: the Composer jumped from y=646 to
y=408 even though its own rail remained 36px tall. The shared outer surface now
uses `overflow: clip`; conversation remains the sole vertical scroll owner.

Live Web evidence repeats the exact interaction with Environment open and keeps
the Composer at y=646/bottom=712 before and after Activity disclosure, while
the outer `scrollTop` remains zero. At 480px the conversation independently
retains its 525px scroll position, the 432px Composer remains at the bottom and
document overflow is zero. The 1280px split workbench likewise retains zero
outer/workspace overflow and an independently scrolling file surface.

### XXXIX. Setup reveals one decision group at a time

Installed `onboarding-page-CHw3TEaW.js` implements onboarding as explicit
progressive phases (`thinking`, `role`, `invitation`, `revealed`) and does not
render later choices before the preceding phase is complete. Its transitions
use a restrained fade/vertical entrance. This is source evidence for staged
disclosure, not permission to copy Codex product copy or invent Garive options.

Garive maps that structure onto configuration the Runtime actually accepts.
Connect first shows preset, profile, target and model; Continue then reveals
deployment and agent identity; Review and Restart retain their existing secure
boundaries. Back preserves both groups, the first newly revealed field receives
keyboard focus and the credential remains write-only on the review boundary.
Desktop and Web consume the same component and translations.

Live Web evidence verifies the runtime group is absent before Continue,
Deployment becomes active afterward and target/model values survive Back. A
480×720 run also exposed a separate inherited 72px sidebar maximum: restored
mobile labels leaked over Setup while the translated rail remained offscreen.
The overlay now resets its flex/max-width and clips its closed bounds; the same
run shows an unobstructed Setup surface and a fully operable opened navigation.

### XL. Tooltip material has no browser-native islands

Installed `thread-app-shell-chrome-DQMbICmE.js` composes shell disclosure through
the shared Tooltip contract, and installed control sources pass disabled reasons
through focusable tooltip content. Garive had already adopted that structure for
its high-frequency actions, but a source audit found three remaining native
`title` islands: durable navigation, Runtime identity and the current Workspace
path. Their host-defined delay, material and keyboard behavior broke the shared
Desktop/Web interaction language.

All three now use the existing semantic Tooltip primitive. Unavailable Memory or
Search explanations expose a focusable `aria-disabled` anchor while keeping the
disabled action inert; Runtime identity retains its full rail geometry; a deep
Workspace path binds its visible text to `role=tooltip` without changing durable
display names. Executable tests forbid the native attributes and verify the
description relationship.

Live Web evidence confirms Memory and Workspace path have no `title`, both bind
an explicit tooltip description and the Runtime status point remains aligned to
the far side of its rail row after the wrapper change. This closes the last known
native-tooltip exception in the shared client source.

### XLI. Every visible task row keeps its state anchor

The supplied Codex references show a trailing circular state anchor on every
visible Pinned row, including the selected task. Installed source registers
separate `sidebar-thread-active` and `sidebar-thread-selected` row facts, while
its stylesheet resolves selected material through the thread-selected data
attribute. The row state and the row selection are therefore independent
signals; selecting a task must not consume its execution status.

Garive already rendered semantic attention, active, failed and completed rings
for Host-returned Recents. During the interval before that list was available,
however, its selected current-Session fallback rendered only hidden status copy.
The same running task consequently lost its trailing ring in the exact primary
thread fixture even though its Composer and progress rail remained active.

The fallback now derives the same five-state category from admitted Work state
and renders the shared `task-state-dot`. It does not infer a new Runtime fact:
submitting/running, suspension, failure and terminal completion are values the
client already owns. Live Web evidence shows the active ring at the far edge of
the selected current row at 1280×720. A 981×720 four-state queue keeps attention,
running, failed/review and completed markers aligned without sacrificing title
ellipsis or changing the quiet selected surface.

### XLII. Setup choices belong to the product surface

Installed `onboarding-page-CHw3TEaW.js` renders its direct choices as product
buttons, exposes selection through `aria-pressed` and supplies an explicit
`focus-visible:outline-2 ... outline-ring` contract. The source contains no
evidence for delegating these high-salience onboarding decisions to a browser
native select. This is a control-level source fact, separate from Garive's
configuration vocabulary.

Garive's Setup previously left two native select islands inside an otherwise
owned Desktop surface. They now share a reusable `ChoicePicker`: options and
initial values still come only from the Host `SetupCatalogue`, selected state is
pressed state, and Arrow keys plus Home/End move a single roving tab stop. The
current catalogue exposes one preset and one profile, so both decisions remain
directly visible instead of being hidden in a speculative popup. Desktop and
Web use the same component, DOM and visual tokens.

Live Web evidence at 1280×720 shows the two choice groups aligned to the 36px
input grid with no combobox in the semantic tree. Continue moves focus to the
first Runtime-identity field; Back preserves the entered `token9` target and
`gpt-5.6` model while retaining the pressed catalogue values. The 480×720 pass
keeps the progressive flow operable without reintroducing native form chrome.

### XLIII. Search preserves the work context

Installed `app-initial-B6Gk5KCN.js` names the global surface “Command menu”,
describes it as searching commands and past chats, and supplies separate
“Search chats” and “Search chats or run a command” placeholders. Together with
the source-backed palette geometry in Section XXXIV, this is direct evidence
that high-frequency history search belongs inside the command overlay rather
than replacing the active work surface with a Web-style destination page.

Garive's sidebar Search, `⌘F`, native Search intent and Work action formerly
routed to a full page with a large heading, explanatory copy and bordered form
card. They now open the existing shared Command menu in search mode while the
current thread remains mounted and inert beneath it. The unified mode retains
the real durable-Session query and attention/active/completed filters; ordinary
Command-menu mode still combines actions and work switching. Both modes share
the same focus trap, Arrow/Home/End navigation, Escape dismissal and exact
return-focus contract. The retired page component and its duplicate CSS are
removed, so later search iterations have one surface rather than two.

Live Web evidence at 1280×800 shows the 520px search palette over the running
thread instead of a route transition. A four-state queue returns all four real
fixture Sessions and narrows “Launch” to the one matching attention task. At
480×720 the palette stays within a 16px viewport inset, preserves all filters
and has no horizontal overflow. `⌘K` on the same narrow work surface opens the
command/action mode without changing the underlying work.

### XLIV. New work is a quiet execution surface

Installed Codex `26.825.51511` provides three direct implementation facts for
the empty task state. `app-initial-B6Gk5KCN.js` uses `Do anything` as the Codex
composer placeholder and `随心输入` in its Chinese locale. The main region in
`new-thread-panel-page-DPS1Lu4g.js` is an empty `max-w-3xl` flex content area,
not a permanent marketing heading and description. Finally,
`home-suggestion-surface-B3IuURqZ.js` owns the starter rows beside the composer;
the Electron token `--spacing-home-suggestion-row` resolves to 40px and the
surface applies an 8px vertical inset. These are source facts, not dimensions
estimated from the supplied screenshots.

Garive now maps that grammar onto its own product capability. New work keeps a
screen-reader heading but removes the visible hero heading and instructional
copy. The real composer remains the primary control and shows the source-backed
placeholder while retaining Garive's accessible outcome label, workspace
context, access state and commit boundary. Starter actions are 40px rows in the
same `composer-stack`, separated by 8px, and disappear as soon as a draft is
entered. This structural ownership removes independent hero/suggestion offsets
and gives Desktop and Web one future layout seam.

The supplied Codex references also show one search icon beside the product
switcher, not a second full Search row in primary navigation. Garive therefore
keeps that icon, `⌘F`, native intent and the shared search overlay while removing
the duplicate row. At 1280×800 the live Web surface exposes one Search button,
one semantic heading, the quiet composer and three adjacent starters. At
480×720 selecting a starter focuses the composer, removes the starter surface
and leaves the send action reachable without horizontal overflow.

### XLV. Goal rail is a durable projection, not chat inference

Installed Codex `app-initial-B6Gk5KCN.js` defines the Composer-adjacent Goal
summary as a status label plus objective and a trailing factual measure. Its
states include `Pursuing goal`, paused, stalled, usage-limited, budget-limited
and achieved. The trailing value selects token usage when a budget exists and
otherwise uses elapsed time. The shared Composer-rail implementation in the
same bundle owns presence/exiting state, above/below placement, overlap, tuck,
border and reduced-motion behavior. These are direct source facts.

Garive already used the same visual rail grammar but derived its displayed
“goal” from the latest user message. That was a presentation inference, not a
Runtime fact. The Desktop Host now exposes the existing bounded `GoalPageV1`
through one main-window-only Tauri command. The shared client strictly decodes
Goal state, objective, revision and verified criteria counts, scopes the page
to the selected Session, and stores it beside Artifact and Workspace
projections. Each optional projection commits independently so a failed Goal
read cannot suppress a successful Artifact or Workspace read.

The rail selects only an active or suspended durable Goal. It displays the
Host objective and verified `{satisfied} / {total}` criteria progress; a
suspended Goal uses the paused treatment. Sessions without a Goal fall back to
neutral `Working` and Runtime activity copy. Garive deliberately does not copy
Codex's elapsed-time or token display because `GoalPageV1` does not expose
those facts. The 1280×800 running fixture proves the compact source-shaped rail
with one active Goal and `1 / 3 criteria`; the narrow fixture verifies the same
facts without clipping the Composer controls.

### XLVI. Window history is capability, not decorative chrome

Installed Codex `26.825.51511` registers application commands
`navigateBack` and `navigateForward` in `app-initial-B6Gk5KCN.js`. Their command
menu labels are Back and Forward; their default bindings are
`CmdOrCtrl+[` / `CmdOrCtrl+]` plus MouseBack / MouseForward. The same bundle
renders the paired toolbar buttons from live `canGoBack` / `canGoForward`
state. This proves that the arrows visible in the supplied desktop references
are application navigation, not disabled window decoration.

Garive now uses one pure, bounded navigation model for Desktop and Web. New
Work, durable Session, Agents, and each Settings destination have explicit
identities. A new destination truncates the forward branch, adjacent duplicate
destinations are ignored, and only the latest 50 entries are retained. Applying
history selects a real Product Session or restores the actual destination;
history never manufactures transcript or execution state. Sidebar buttons,
native Settings intent, command palette routes, `⌘[` / `⌘]`, and mouse history
buttons all enter the same path. Unit tests lock branch, deduplication and bound
semantics; the App test proves enabled states and Work ↔ Agents round trips.

## Gate 1 — Codex fidelity

This gate passes only when both Desktop and Web show:

- neutral layered surfaces with no decorative gradient;
- a 240–520 px resizable rail with 275 px preferred width, compact rows and quiet selected surfaces;
- a 46 px main toolbar, 36 px compact chrome and 40 px pane toolbar;
- a source-equivalent 672 px reference reading measure and clear user/agent turn rhythm;
- an elevated compact composer with a 14 px input and 28 px actions;
- semantic navigation groups in sentence case;
- Environment as a compact dismissible overlay and artifacts/files as a true
  resizable tabbed workbench, never one fixed generic inspector;
- readable 12 px minimum metadata, except bounded 11 px timestamps;
- light, dark, 720 px and 200% text-zoom evidence without horizontal overflow;
- identical DOM, keyboard semantics and tokens for Desktop and Web.

The desktop composition is also a Gate 1 requirement. The web client reuses
the same rail, title/tab bar, workbench, overlays and composer geometry; only
the native traffic-light safe zone and OS window behavior differ. Neither
client may introduce a logo-led marketing header, a fake account control or a
second execution status merely to fill chrome.

The review asks “would this feel at home beside Codex?” Feature count is not a
substitute for visual quality.

## Gate 2 — exceed the baseline

Garive exceeds rather than imitates when existing Runtime truth is visibly
useful without making the shell busier:

1. **Attention-first task rail.** Needs-input, running, failed and completed
   Sessions retain explicit text and stable ordering; color is secondary.
2. **Outcome-first canvas.** The request, committed result and next action
   outrank implementation chatter.
3. **Evidence without debug noise.** Activity, artifacts, revisions and
   verification live in one optional panel and never compete with reading.
4. **Authority at the action boundary.** Execution scope, Workspace authority
   and approval appear next to the composer action they constrain.
5. **Durable continuity.** Reconnect, restart and unknown-outcome states keep
   drafts and explain the safe next action in place.
6. **Honest capacity.** Usage appears only from trusted facts and never
   masquerades as task progress.
7. **Workbench continuity.** A committed artifact opens beside its originating
   Turn with revision and authority intact; closing it restores the undisturbed
   thread and composer position.

Gate 2 cannot compensate for a failed Gate 1. A powerful but visually rough
surface is still a failed product experience.

## Review method

Every visual batch records the reference and revision, captures wide light,
wide dark and 720 px fixtures, compares proportions/type/density/hierarchy,
runs keyboard and accessibility modes, checks Desktop and Web from the same
fixture, then requires build, unit and visual-contract gates before admission.
