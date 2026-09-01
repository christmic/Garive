# Desktop and Web Codex fidelity study

> Reviewed 2026-08-31. This is evidence and design reasoning. The normative
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
| neutral ramp | `#fff`, `#f9f9f9`, `#ededed`, `#cdcdcd`, `#afafaf`, `#414141`, `#303030`, `#212121`, `#181818`, `#0d0d0d` |

The bundle also separates modules for the local conversation thread and turn
entries, scroll layout and virtualizer, application chrome, message navigation
rail, composer utility bar, task suggestions, terminal panel, worktree
environment, text-file tabs and artifact tabs. This confirms that Codex is a
desktop workbench, not one chat column plus a generic inspector. These values
and module boundaries are evidence, not a license to import code or assets.

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
| composer | approximately 545 px wide, one-line input start and one circular stop action | 490 px wide, two-line empty input, duplicate text stop plus spinner and a visible disclaimer | 39 rem / 546 px measure at the 14 px root, one-row start, 32 px actions and one truthful stop control |
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
request as a single ellipsized line, and keeps only the current public Activity
state plus the details action at the edge. Exact tool labels remain available
to assistive technology and in the Environment drill-in; no timer is invented
because the current Host projection does not expose a trusted elapsed-time
fact. With Environment open at the reference viewport, the rail remains x=353,
544 px wide and free of internal overflow. At 720 and 480 px the Composer is
546 and 432 px wide respectively, with zero rail or document overflow.

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

The same installed bundle exposes the full Composer surface contract. A
multiline default Composer uses `radius-3xl`: 20 px fallback and 25 px when the
Electron engine supports the bundle's `superellipse(1.5)` corner scale. The
root explicitly forces `border: 0`. Light elevation is a 4% one-pixel ring, a
4% 2×8 px lift and a 2.4% 4×80 px ambient shadow; dark elevation is only a 20%
white one-pixel inset.

Garive previously used a 16 px round corner, a 15% dark border and a 28% black
18×50 px dark shadow. It now uses the resource-backed 20/25 px radius,
superellipse where supported, zero physical border and independent Composer
elevation tokens. Computed dark output is exactly a 25 px superellipse with a
20% white inset; computed light output matches all three source shadow layers.
New Work remains 64 px high, Running 97 px, and the 480 px governed-approval
Composer remains fully visible at 272.7 px. All measured states have zero
internal and document overflow.

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

## Gate 1 — Codex fidelity

This gate passes only when both Desktop and Web show:

- neutral layered surfaces with no decorative gradient;
- a 240–275 px comfortable rail, 30–36 px rows and quiet selected surfaces;
- a 34 px title bar with one-line orientation and low-chrome actions;
- a 39 rem reference reading measure and clear user/agent turn rhythm;
- an elevated compact composer with a 14 px input and 36 px actions;
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
