# Desktop Work visual and interaction benchmark

> Reviewed 2026-08-31. This is exploratory design input; the normative contract
> lives in `spec/design/shared-client-visual-system.md`.

## Product question

Garive must feel like a durable work environment, not a chat window with more
buttons. The comparison focuses on five jobs: understand available capacity,
start and switch work quickly, see what needs attention, inspect and refine a
deliverable, and recover without losing truth.

## Comparison

| Product pattern | Strong idea | Failure to avoid | Garive adaptation |
|---|---|---|---|
| ChatGPT Work / Codex | Shared usage is visible as a window, model choice changes consumption, reset timing is explicit, and limit recovery offers smaller models, credits, or API usage. Active work may finish after the boundary is reached. | A single unexplained percentage suggests false precision and makes users fear that active work will be killed. | Show scope, period, remaining capacity, reset, attribution and continuation policy together. Never estimate billing from prompt length. |
| Codex desktop | Project/task navigation, long-running work, command entry and output inspection share one application frame. | Treating every task as an identical chat hides attention and recovery state. | Keep durable Sessions in a priority rail and expose the same task vocabulary in rail, command center, search and notifications. |
| QoderWork | Specialized desks have their own view, tools and output standard while sharing one Agent and task foundation. Questions, Plan and Nudge keep artifact work editable. | Forking each desk into a different visual language or disconnected task system. | Garive modes may change the center canvas and delivery controls, but shell, tokens, task state, approvals and usage remain invariant. |
| Linear | Dense navigation, command-driven creation, status-first lists and restrained semantic color make large work sets scan quickly. | Compressing labels or hit targets until state becomes cryptic. | Offer comfortable and compact density without hiding state text or reducing keyboard/touch accessibility. |
| Raycast | One predictable command surface, short labels, stable keyboard movement and immediate filtering reduce navigation cost. | Turning the command surface into a second app with different commands and terminology. | `Command-K` mirrors visible actions and durable task names; it never gains authority unavailable on the current screen. |
| macOS | Native typography, focus, reduced motion/transparency, contrast and familiar menu shortcuts make a webview feel at home. | Decorative translucency, tiny metadata and browser-like focus behavior. | Use system typography, semantic surfaces, platform shortcuts and explicit accessibility modes; native-only authority remains outside React. |

## Usage and capacity lessons

OpenAI documents that ChatGPT Work and Codex share usage. Consumption varies
with model, context, reasoning, tool use, retrieval, caching and local/cloud
execution, so prompt length cannot predict exact cost. The product exposes a
usage dashboard, a reset window and recovery choices. This leads to six Garive
rules:

1. Capacity is a first-class work condition, not an account footnote.
2. A percentage is shown only with a named scope and time window.
3. Exact, estimated and unavailable values have visibly different copy.
4. The UI explains whether already-running work can finish.
5. Cost posture uses qualitative labels until a trusted source supplies exact
   units. Model names alone are not prices.
6. Warnings appear before submission; they do not overwrite durable task state.

Garive has four distinct budget concepts. They must never be merged:

| Concept | Owner | User question |
|---|---|---|
| Included plan capacity | Garive account/workspace service | “How much included work remains before reset?” |
| Purchased credits / pay-as-you-go | Billing service | “What will continue after included capacity?” |
| Provider API usage | Configured provider | “Is this work charged outside Garive?” |
| Goal/Turn execution budget | Runtime | “What bounded resources may this task consume?” |

## Visual direction

Garive is a quiet operational workbench: warm paper for thinking, graphite for
structure, cobalt for agency, green for verified completion, amber for required
attention and red only for failure/destruction. Its signature is not a gradient
or mascot; it is the consistent relationship between outcome, state, evidence
and next action.

Every product surface uses the same hierarchy:

1. **Orientation** — location, task, Workspace and execution location.
2. **Condition** — durable state, connection and available capacity.
3. **Work** — conversation, canvas, document or other artifact-first view.
4. **Evidence** — activity, receipts, versions and verification.
5. **Action** — one primary next step plus reversible secondary actions.

Specialized desks may replace layer 3. They may not redefine the other four.

## Sources

- [OpenAI pricing and usage](https://learn.chatgpt.com/docs/pricing)
- [OpenAI desktop app](https://learn.chatgpt.com/docs/app)
- [QoderWork custom desks](https://qoder.com/zh/blog/qoderwork-customdesk)
- [Qoder credits practices](https://qoder.com/zh/blog/qoder-case-creditstips)
- [Linear command menu](https://linear.app/docs/command-menu)
- [Raycast manual](https://manual.raycast.com/)
- [Apple Human Interface Guidelines](https://developer.apple.com/design/human-interface-guidelines/)
