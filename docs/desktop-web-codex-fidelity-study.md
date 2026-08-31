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

### E. 2026-09-01 workbench gap closure

The supplied 1280 × 800 references and an equal-size Garive Web render were
placed side by side. The comparison produced concrete component changes rather
than a general “more polished” direction:

| Surface | Reference evidence | Rejected Garive baseline | Required shared behavior |
|---|---|---|---|
| narrow thread title | one line in the split pane | wrapped to three lines | preserve one-line orientation and elide before status/actions |
| terminal actions | quiet glyphs at the content edge | wide `Export .md` and `Copy` labels | 30 px icon controls with persistent accessible names |
| composer | approximately 545 px wide, one-line input start and one circular stop action | 490 px wide, two-line empty input, duplicate text stop plus spinner and a visible disclaimer | 40 rem measure, one-row start, 32 px actions and one truthful stop control |
| fenced output | bounded code card with a language header and copy action | undifferentiated bordered `pre` block | labeled code workbench, exact-source copy and persistent accessible feedback |
| split workbench geometry | conversation, Composer and file divider share one bounded column; document starts 24 px after its edge | 352 px grid column whose child painted at 586 px, sending the 538 px Composer 210 px beneath the file surface; document added a second centered 40 px inset | explicit zero-min grid track, clipped conversation surface, 352 px default/320–520 px persisted separator and one 24 px document inset |
| Environment | named floating surface | Activity/Artifacts tabs inside a small card | one Environment heading and a dismiss action; evidence rows only |
| file chrome | selected file tab plus a second location/action row | generic Inspector tabs plus an expanded artifact card | one selected file tab, breadcrumb row, source/render action and independent scroll |
| source inspection | available beside rendered document | no source presentation | reversible Rendered/Source switch over the same immutable preview bytes |

Garive deliberately keeps verified revision evidence and the explicit close
action. Those are Gate 2 additions: they do not add ordinary-screen noise and
they preserve the Runtime truth boundary.

## Gate 1 — Codex fidelity

This gate passes only when both Desktop and Web show:

- neutral layered surfaces with no decorative gradient;
- a 240–275 px comfortable rail, 30–36 px rows and quiet selected surfaces;
- a 46 px title bar with one-line orientation and low-chrome actions;
- a 40 rem default reading measure and clear user/agent turn rhythm;
- an elevated compact composer with a 14 px input and 36 px actions;
- semantic navigation groups in sentence case;
- Environment as a compact dismissible overlay and artifacts/files as a true
  resizable tabbed workbench, never one fixed generic inspector;
- readable 12 px minimum metadata, except bounded 11 px timestamps;
- light, dark, 720 px and 200% text-zoom evidence without horizontal overflow;
- identical DOM, keyboard semantics and tokens for Desktop and Web.

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
