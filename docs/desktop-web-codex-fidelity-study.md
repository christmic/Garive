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
inspection of the installed package established these current facts:

| Family | Observed values |
|---|---|
| spacing | 4 px base |
| type | 11, 12, 14, 16 and 28 px core steps |
| radii | 4, 6, 8, 10, 12, 16 and 20 px |
| navigation rows | 30 and 36 px variants |
| neutral ramp | `#fff`, `#f9f9f9`, `#ededed`, `#cdcdcd`, `#afafaf`, `#414141`, `#303030`, `#212121`, `#181818`, `#0d0d0d` |

These values are evidence, not a license to import packaged code or assets.

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

## Gate 1 — Codex fidelity

This gate passes only when both Desktop and Web show:

- neutral layered surfaces with no decorative gradient;
- 252 px comfortable rail, 36 px navigation rows and quiet selected surfaces;
- 56 px title bar with one-line orientation and low-chrome actions;
- 720–760 px reading measure and clear user/agent turn rhythm;
- an elevated compact composer with a 14 px input and 36 px actions;
- semantic navigation groups in sentence case;
- a 360 px evidence panel using the shell's type and spacing scale;
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

Gate 2 cannot compensate for a failed Gate 1. A powerful but visually rough
surface is still a failed product experience.

## Review method

Every visual batch records the reference and revision, captures wide light,
wide dark and 720 px fixtures, compares proportions/type/density/hierarchy,
runs keyboard and accessibility modes, checks Desktop and Web from the same
fixture, then requires build, unit and visual-contract gates before admission.
