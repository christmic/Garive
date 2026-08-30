# A-DESKTOP-VE — visual, journey, and user-manual evidence

> Garive Desktop is not admitted because it launches. It is admitted only when
> the shipped app is coherent, attractive, efficient, and every documented
> workflow is demonstrated from a reproducible product state.

## Status and ownership

Accepted companion contract for A-DESKTOP-WORK. This document owns final
visual-quality evidence, end-to-end journey evidence, screenshot provenance,
and the macOS user manual. It does not weaken Runtime, Workspace, privacy,
accessibility, packaging, or release requirements in their owning Specs.

## Product bar

The interaction quality must match or exceed the referenced Work products in
five observable ways:

1. **Orientation:** a new user can identify the current Session, local status,
   context, next action, and outputs without reading documentation.
2. **Momentum:** New Work, Search, resume, context selection, approval, preview,
   and export each require no avoidable navigation or mode change.
3. **Trust:** pending, committed, verified, blocked, failed, and unavailable
   states are visually distinct and never inferred from animation alone.
4. **Craft:** hierarchy, rhythm, typography, contrast, empty states, motion,
   native window chrome, menus, and light/dark appearance feel intentional as
   one macOS product rather than a wrapped web page.
5. **Recovery:** failures retain the user's place and provide a precise safe
   next action; restart does not turn durable work into an empty UI.

Comparison is journey-based, not a subjective screenshot contest. A release
review records task completion time, primary actions, focus transitions,
confusing pauses, errors, and visual defects for the same representative jobs.

## Evidence classes

Every image in the user manual has one provenance class in the screenshot
manifest:

| Class | Permitted claim |
|---|---|
| `packaged-real` | Captured from the exact candidate `.app` using a real local Runtime and file-backed SQLite. May prove the documented function works. |
| `packaged-recovery` | Captured from the candidate after an intentionally induced restart, denial, stale grant, offline provider, or damaged backing file. May prove recovery behavior. |
| `deterministic-visual` | Captured from a bounded development fixture. May prove layout, copy, contrast, and responsive presentation only. It must never prove Runtime or filesystem behavior. |
| `system-surface` | Native macOS menu, picker, Keychain prompt, save panel, About window, Gatekeeper, or update UI associated with the candidate. |

Production documentation prefers `packaged-real`. If a private credential or
unpublishable source is required, use a local loopback provider and synthetic
Workspace through the normal shipped path. Never replace a missing product
capability with client-owned demo data.

## Screenshot safety

- Use a dedicated synthetic macOS account or clean test profile.
- Window title, sidebar, notifications, menus, picker, preview, clipboard, and
  background surfaces contain no personal name, path, credential, endpoint,
  raw private source, bookmark reference, or database identifier.
- Synthetic Workspace names and content are stable, reviewable, and committed
  beside the capture manifest; identifiers visible only in developer tools do
  not enter the manual.
- Capture the whole product window unless a detail crop is necessary. Crops
  retain enough surrounding UI to establish location and state.
- Do not add status labels, buttons, shadows, or content in an image editor.
  Only lossless crop, privacy redaction, and scale conversion are permitted,
  and every edit is declared in the manifest.
- Pointer location is omitted unless it explains a hover-only affordance.
  Keyboard focus remains visible in keyboard and accessibility captures.

## Canonical capture environments

| Environment | Window and scale | Appearance | Purpose |
|---|---|---|---|
| `standard-light` | 1280 × 800 points, 100% | Light, comfortable | Primary manual images and hierarchy review. |
| `standard-dark` | 1280 × 800 points, 100% | Dark, comfortable | Dark token and contrast parity. |
| `compact` | 900 × 640 points, 100% | System, compact | Collapsed rail and inspector overlay. |
| `zoom-200` | 1280 × 800 physical, effective 640 × 400 CSS points | Light, comfortable | 200% zoom/reflow and large-text evidence. |
| `contrast` | 1280 × 800 points | Increase contrast + reduce transparency + reduce motion | macOS accessibility preference evidence. |
| `cjk` | 1280 × 800 points | Light, comfortable, Simplified Chinese | CJK composition, wrapping, typography, and localized copy. |

Every release candidate includes Retina captures. The manifest records macOS,
machine architecture, display scale, Garive version, Git revision, package
checksum, locale, appearance, density, window size, evidence class, setup
recipe, expected assertions, capture time, and image SHA-256.

## Full-function capture matrix

The following IDs are stable. The final manual may reuse one image across
adjacent instructions, but no row is complete without an image and its listed
interactive assertions.

### Install, launch, and setup

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M01` | Clean install | DMG and Applications handoff | app bundle is the candidate checksum; no quarantine bypass instructions |
| `M02` | First launch | native window and Connect step | correct traffic lights, titlebar drag, first field focus, no secret displayed |
| `M03` | Valid profile details | Review step | exact provider/model/Agent summary; credential absent |
| `M04` | Credential entered | save action | secure field, write-only explanation, no value echo |
| `M05` | Commit complete | Ready step | explicit restart requirement and successful restart action |
| `M06` | Returning launch | restored Work home | local Runtime status, durable Recents, no setup flash |

### Create and continue work

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M10` | Empty Session | outcome-first home | suggestion fills composer and returns focus |
| `M11` | Draft entered | composer and visible context area | Return sends, Shift-Return adds newline, IME composition does not send |
| `M12` | Turn committed pending/running state | timeline plus Activity | feedback is under 100 ms locally; status text is bounded and truthful |
| `M13` | Completed Turn | GFM result | headings/table/tasks render safely; Copy and Export Markdown work |
| `M14` | Second Turn | same Session | prior result remains; continuation preserves Session identity |
| `M15` | Input suspension | continuation composer | exact suspension resumes; replacement Turn is not created |
| `M16` | App restarted mid/after work | restored timeline | fixed-prefix history is ordered and later pages are present |

### Navigate and find durable work

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M20` | Multiple Sessions | Recents rail | titles/states match durable projection; selection opens in one action |
| `M21` | Search opened | Search surface | menu, Command-F and Command-K converge; input receives focus |
| `M22` | Matching query | result list | local query returns correct durable Session and Turn count |
| `M23` | No match | empty result | query remains editable; no misleading cloud-search claim |
| `M24` | New Work | clean composer | Command-N and menu clear transient context without deleting history |

### Workspace context and authority

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M30` | Add context | native folder picker | cancel changes nothing; broad roots and symlink roots are rejected |
| `M31` | Folder authorized | in-app file picker | only bounded safe metadata appears; Tab loop and Escape work |
| `M32` | Nested folder | breadcrumb and file list | descent/back/pagination preserve opaque authority and selection |
| `M33` | Files selected | pre-send chips | remove acts before send; filenames are not copied into control labels |
| `M34` | Read-only attached Workspace | committed chip | read scope and attachment are distinct from next-Turn selection |
| `M35` | Write authorization requested | native picker and output badge | same-root reauthorization only; write grant is process-local |
| `M36` | Workspace detached | timeline and composer | durable receipt reloads truth and focus returns safely |
| `M37` | Dormant authorization | Settings recovery row | no path is shown; wrong root is rejected; same root restores identity |
| `M38` | Revoke confirmation | two-step destructive action | first click does not revoke; confirmation drops authority immediately |
| `M39` | Cleanup retry after restart | Settings recovery status | tombstone survives and private cleanup retry is bounded |

### Approval, activity, and recovery

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M40` | Exact write prepared | approval card | operation, Workspace, one-file scope, one-call duration, no-overwrite visible |
| `M41` | Keyboard entry to approval | focused Decline | safe action receives focus; assertive announcement contains no private data |
| `M42` | Declined | committed terminal state | denial is durable, write absent, composer usable |
| `M43` | Approved | Activity sequence | prepared→authorized→running→completed order comes from committed facts |
| `M44` | Runtime/provider failure | inline recovery | draft/place retained; failure and next action are specific |
| `M45` | Projection unavailable | recovery state | committed outcome is not rerun; exact read/retry path is offered |
| `M46` | Offline/reconnect | connection state | no fake progress; follow resumes from durable cursor without duplicates |

### Artifacts and export

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M50` | Artifact committed | Inspector Artifacts tab | immutable metadata matches receipt-bound projection |
| `M51` | Preview opened | verified preview | bytes are digest checked, bounded, readable, and not live-announced |
| `M52` | Backing bytes changed | preview failure | metadata remains; changed content fails closed |
| `M53` | Export selected | native save panel | suggested safe filename, explicit cancel, no destination path enters React |
| `M54` | Export successful | Artifact card success | one-shot authority consumed; exact bytes and checksum verified externally |
| `M55` | Existing destination | no-overwrite error | existing bytes unchanged; user chooses another name |
| `M56` | Crash during export | next explicit directory selection | only exact journalled temporary is removed; unrelated files remain |

### Inspector, settings, and native macOS surfaces

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M60` | Inspector open | Activity tab | tablist/tabpanel semantics; Command-Shift-A and View menu parity |
| `M61` | Inspector switched | Artifacts tab | focus/selection state is visible and content does not jump timeline |
| `M62` | Settings | Appearance | System/Light/Dark and Comfortable/Compact persist exact admitted values |
| `M63` | Settings | Runtime capability truth | unavailable features remain labelled and gated |
| `M64` | Menu bar | Garive/File/Edit/View/Window | standard macOS roles plus four safe app intents; shortcuts match UI |
| `M65` | Window management | compact/fullscreen/split view | no horizontal conversation scroll; inspector becomes bounded overlay |
| `M66` | Quit/reopen | restored window and Session | visible-screen restoration and no duplicate mutation |

### Accessibility, localization, and appearance

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M70` | Keyboard-only journey | visible focus sequence | every workflow completes without pointer; overlays contain/restore focus |
| `M71` | VoiceOver | rotor/landmark and approval output | names, roles, states, tabs, live regions, and safe filenames are correct |
| `M72` | 200% zoom | Work + approval + Artifact + Settings | reflow is usable; no clipped action, overlap, or horizontal transcript scroll |
| `M73` | Dark appearance | same semantic states as Light | status hierarchy and contrast remain equivalent |
| `M74` | Increased contrast/reduced transparency/motion | representative workflow | focus, boundaries, errors, and state survive system preferences |
| `M75` | Simplified Chinese | setup, Work, picker, approval, Artifact, Settings | stable keys, bounded parameters, CJK wrapping and IME pass |
| `M76` | Pseudolocale | longest major screens | no truncation, collision, inaccessible control, or accidental raw key |

### Release and lifecycle

| ID | Required state | Capture | Interactive assertions |
|---|---|---|---|
| `M80` | Candidate package | About/version surface | version and build identity match signed bundle and manifest |
| `M81` | Clean supported Mac | first launch | Developer ID, Gatekeeper, stapled notarization and permissions pass |
| `M82` | Valid update | update surface | signature verified, restart/recovery clear, durable data retained |
| `M83` | Invalid/downgrade update | refusal surface | install refused safely; running version remains usable |
| `M84` | Sleep/wake and network loss | resumed Session | connection truth and cursor continuation remain exact |
| `M85` | Uninstall/data retention | documented system state | user understands which local data and Keychain items remain and removal path |

## Visual review rubric

Each canonical screen receives 0–2 points for the following, with no zero
permitted and at least 18/20 overall before release:

- hierarchy identifies one primary action;
- spatial rhythm follows the 4-point system;
- content width and line length remain readable;
- controls use consistent shape, weight, and platform language;
- empty/loading/error/blocked/success states feel related but distinct;
- light/dark/contrast modes preserve semantic priority;
- focus is obvious without overwhelming the visual hierarchy;
- user content wraps safely and never distorts controls;
- motion communicates change and respects reduced motion;
- native and webview surfaces transition without visual or terminology drift.

The review records defects by capture ID, severity, screenshot digest, owner,
fix revision, and recapture digest. Replacing an image without closing its
defect is prohibited.

## User manual contract

The delivered manual is task-oriented and contains:

1. install, authenticity check, first launch, setup, and restart;
2. window map and the meaning of Local, pending, committed, and verified;
3. create, steer, continue, search, reopen, and export work;
4. select local context, authorize output, detach, recover, and revoke access;
5. understand Activity, exact approvals, Artifacts, preview verification, and
   no-overwrite export;
6. keyboard shortcuts, menus, appearance, density, accessibility, and Chinese;
7. offline/restart/failure recovery and diagnostic information safe to share;
8. security/privacy model, local data locations described without exposing a
   user's paths, update behavior, uninstall, and data-retention choices;
9. current capability limits stated plainly, with no roadmap feature presented
   as available.

Every procedure starts from an observable screen, gives the shortest primary
path, includes the relevant screenshot ID, states the expected committed
result, and provides a recovery branch. The manual version, app version,
package checksum, screenshot manifest digest, and tested macOS versions appear
on its front matter.

## Admission

A-DESKTOP-VE is complete only when:

- every matrix row has passing interactive evidence or is explicitly gated as
  an unavailable Extended capability outside the shipped UI;
- all manual images resolve, match their manifest digests, and pass the safety
  review;
- a reviewer follows every procedure on a clean supported Mac using the
  candidate package without repository knowledge;
- all review defects are closed by a recaptured image;
- the final PDF and source manual pass link, text extraction, 200% viewing,
  VoiceOver reading order, print, and checksum verification.
