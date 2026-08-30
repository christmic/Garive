# A-DESKTOP-WORK — macOS work product

> This Spec defines Garive Desktop as a local-first work cockpit: a user states
> an outcome, observes bounded durable progress, controls authority, reviews
> deliverables, and can resume the same work after restart.

## Status and audience

Accepted product contract for Desktop product, Runtime Host, React/Tauri,
macOS integration, security, accessibility, release, and test engineers.

This Spec refines A-UX1 for macOS. H1/H2/H3, A-DESKTOP-C2, M2-D, C5/C6,
Scheduler, Skill, Knowledge, Memory, and Observability retain their authority.
This document owns product composition and presentation, not a second domain
model or an alternate persistence layer.

## Product promise

Garive Desktop turns an outcome into finished, inspectable work:

```text
outcome -> scoped context -> visible execution -> controlled decisions
        -> verified deliverables -> durable continuation
```

The primary unit is a durable Session in one local Workspace. Chat is the
coordination surface, not the product boundary. A successful Turn may produce
an answer, one or more artifacts, verified changes, or a suspension requiring
the user. A completion without inspectable output or verification must not be
presented as finished work.

Garive is local-first and vendor-neutral. The Desktop process embeds Runtime;
credentials remain in the OS credential service; files are accessed only
through explicit opaque workspace capabilities; all user-visible progress is
projected from committed Runtime facts.

## Product evidence

The interaction model is informed by current first-party product material:

| Product | Verified product pattern | Garive decision |
|---|---|---|
| [ChatGPT Work](https://help.openai.com/en/articles/20001275) | Long, multi-step work; finished documents, sheets, presentations, reports, and Sites; progress, questions, steering, approval, local/cloud distinction, unified recents and projects. | Outcome-first composer, durable recents/workspaces, progress and decision surfaces, deliverable rail, explicit Local badge. |
| [ChatGPT macOS Work with Apps](https://help.openai.com/en/articles/10119604-work-with-apps-on-macos) | Global shortcut, visible attached-app context, explicit permission, reviewable/revertible edits. | Quick entry, context chips with provenance, capability-scoped grants, review before mutation. |
| [QoderWork](https://docs.qoder.com/qoderwork/introduction) | Files, documents, browser/computer use, schedules, channels, skills, voice, and cross-session memory. | One extensible Work surface; capabilities are discoverable but never implied when unavailable. |
| [QoderWork releases](https://docs.qoder.com/release-notes/qoderwork) | Dedicated writing/slides/design modes with live previews; input history; permission onboarding and daily polish. | Artifact-aware inspector, mode suggestions rather than hidden Agent switching, complete keyboard behavior. |
| [OpenWork](https://github.com/different-ai/openwork) | Local workspace, multi-session navigation, SSE progress, permission requests, skills/MCP, file and artifact previews. | Three-region workspace, typed activity, explicit approvals, artifact tabs, local-first setup. |
| [Claude Code Desktop](https://code.claude.com/docs/en/desktop) | Parallel sessions, isolated workspaces, terminal/editor/preview panes, side questions, diffs, and background task visibility. | Sessions remain independent; inspector is contextual; parallelism cannot mix authority or results. |

These references are evidence, not compatibility targets. Garive never copies
vendor-specific protocol, account, cloud, model, or hidden reasoning behavior.

## Users and jobs

1. A knowledge worker turns local source material into a report, table, deck,
   plan, or structured research result.
2. A developer investigates and changes a local repository with reviewable
   effects and verification evidence.
3. An operator delegates a long task, leaves, returns, and immediately knows
   whether it finished, failed, disconnected, or needs a decision.
4. A privacy-sensitive user understands what context and authority each Turn
   has before sending it and can remove access without exposing paths to React.

The first-run experience must reach a real successful Turn in under five
minutes when a supported provider credential is available. Returning users
must reopen recent work in at most two primary actions.

## Product principles

- Outcome over prompt: ask what should exist when the work is done.
- Evidence over animation: only committed events claim progress or completion.
- Context is visible: every attached workspace, file, app, skill, and source is
  represented before send with scope and removal affordance.
- Authority is specific: read, write, execute, network, app control, and secret
  access are separate; approval explains target, duration, and consequence.
- Deliverables are first-class: artifacts open beside the conversation and
  retain provenance, revision, verification, export, and reveal actions.
- Recovery is ordinary: restart, reconnect, unknown command outcome, and stale
  plans are normal product states with exact next actions.
- Calm by default: the main timeline shows decisions and results; verbose safe
  activity is available in the inspector.

## Capability truth and release levels

The UI renders a backend-provided `DesktopCapabilityManifestV1`. It never
infers capability from a button, model name, config file, or installed crate.

| Level | Required real capability |
|---|---|
| Foundation | C2 setup, H2 Agents/Sessions/timeline, multi-turn H1, H3 activity, cancel/continue/reconnect. |
| Work | Opaque local Workspace capability, governed read/write/execute, artifact projection/preview/export, verification receipts. |
| Extended | Browser/computer use, connectors, voice, Scheduler, cross-device gateway, team policy. |

Unavailable levels remain visible only in a labelled capability catalogue or
roadmap. Disabled controls explain the missing capability. Marketing, empty
states, seeded demo data, and client state cannot impersonate a backend claim.

## Information architecture

```text
Garive
├── Work
│   ├── New work
│   ├── Recents (filter, search, pin)
│   └── Workspaces
│       └── Session
│           ├── Conversation
│           ├── Activity
│           └── Artifacts
├── Automations
├── Library
│   ├── Agents
│   ├── Skills
│   ├── Memory
│   └── Connections
└── Settings
```

Primary navigation must not advertise an unavailable Extended surface. The
Foundation release shows Work, Agents, and Settings. Memory appears only after
M2-D; Automations only after a Scheduler product contract; Connections only
after a connector control contract.

## Window and regions

The main macOS window uses native traffic-light space and a draggable titlebar.
Default size is 1180 x 780, minimum 820 x 600, restored within the visible
screen. Fullscreen and split-screen are supported.

| Region | Expanded desktop | Compact window |
|---|---|---|
| Navigation rail | 248 px resizable, persistent | 64 px icons or overlay |
| Conversation | fluid, readable content width 760 px | full width |
| Inspector | 360–520 px resizable, contextual | sheet/overlay |
| Composer | sticky inside conversation, max 760 px | safe-edge sticky |

The rail and inspector collapse independently. Conversation never horizontally
scrolls. At widths below 900 px only one secondary region may overlay. Layout
preferences are bounded local preferences and do not represent durable truth.

## Visual system

The visual character is a quiet, warm workbench rather than a developer
console. Use system fonts (`-apple-system`, then sans-serif), 14 px body, 12 px
metadata, 20–28 px display hierarchy, and tabular numerals for positions/time.

Semantic tokens support light, dark, increased-contrast, and reduced-
transparency modes. The base palette uses neutral graphite surfaces, warm
paper conversation, cobalt focus/action, green verified, amber attention, and
red destructive/error. Color never carries state alone.

Appearance exposes three explicit theme values (`system`, `light`, `dark`) and
two density values (`comfortable`, `compact`). `system` follows live macOS
appearance changes; either explicit theme overrides the system preference.
Density changes information spacing without hiding controls, changing durable
content, or weakening the minimum target and zoom requirements below.

Spacing follows a 4 px base; normal controls are at least 32 px high and all
pointer targets at least 44 x 44 CSS px where the layout permits. Corners use
8 px controls, 12 px cards, and 18 px composer. Shadows are limited to overlays
and the raised composer. Motion is 120–180 ms and becomes instant under
`prefers-reduced-motion`.

## Core screens

### Boot and setup

Boot shows a branded, non-blocking progress surface while backend recovery and
capability discovery complete. It transitions exactly to Ready,
NotConfigured, InvalidConfiguration, RecoveryRequired, or Unavailable.

First run is a guided C2 flow: choose installed profile/model, optionally
disclose an endpoint override, enter the credential in a secure native field,
review a redacted plan, commit, and explicitly restart. Invalid configuration
offers Reconfigure and safe Diagnostics; it never exits before showing a
recoverable window.

### Home / new work

The empty state leads with “What outcome should Garive deliver?” and a large
composer. Suggested jobs are capability-filtered and phrased as outcomes, not
feature advertisements. The user selects an Agent and optionally attaches a
Workspace/context before send. The context summary states Local and the exact
access class.

### Recents and workspaces

Recents group pinned items, today, previous seven days, and older. Each row
shows user-derived or explicit title, Workspace label, latest safe state,
attention badge, and relative time. Search is local over H2 public titles/text
only; no hidden context is indexed. Keyboard navigation, contextual menus, pin,
rename, archive, and deletion appear only when their Host mutations exist.

Workspace rows contain an opaque ID and backend-approved display label, never
a filesystem path. Choosing a folder happens in a native picker. Revoking a
Workspace stops new access and does not falsify existing receipts.

### Session

The header shows title, Workspace, Local/Remote execution location, Agent, and
current state. Technical identities live in Details. The timeline contains:

- user outcome/request;
- concise assistant coordination and committed result;
- expandable activity groups with state, duration, and safe label;
- inline decision cards for approval or structured external input;
- artifact cards with type, revision, verification, and Open action;
- truthful connection, cancellation, suspension, failure, and stale notices.

Hidden reasoning, raw tool arguments/results, credentials, paths, provider
bodies, and internal facts are never rendered. Unknown activity is neutral.

Opening a Session follows 64-item H2 pages with a strictly increasing durable
watermark until the fixed prefix is complete. Desktop admits at most 512 Turns
and 256 immutable Artifacts into one restored view; a repeated/decreasing cursor,
wrong Session identity, or additional page beyond those bounds fails closed
instead of presenting a plausible but incomplete history.

### Composer

Enter sends, Shift+Enter inserts a line break, and IME composition never sends.
The composer grows to eight lines then scrolls. It contains attach context,
Agent/mode selection, execution location, permission posture, voice when
admitted, and send/cancel. A compact footer explains the effective access.

Empty, oversized, unavailable, or mutation-conflicted input is disabled with
an accessible reason. Drafts are per-Session bounded local preferences. A draft
is cleared only after durable start acknowledgement. Exact retry reuses the
same command identity and normalized bytes.

### Activity and artifacts inspector

Activity groups H3 items by Turn and shows only committed states. It follows
the timeline watermark, marks gaps, and never treats stream EOF as terminal.
“Attention” moves to the top and receives a badge, not a modal unless user
authority is required.

Artifacts are immutable Runtime projections with opaque capability IDs,
display names, MIME/type, revision, size, producing Turn, verification state,
and safe preview availability. Text/code, image, PDF, table, and presentation
previews are sandboxed and bounded. Edit, reveal, export, and overwrite each
require their own admitted command; a webview URL or raw path is never trusted
as authority.

### Settings and library

Settings groups General, Appearance, Models, Permissions, Data, Shortcuts,
Diagnostics, and About. Sensitive values are write-only. Permissions show
grants by capability and Workspace with revoke actions. Diagnostics export only
stable codes, versions, bounds, and redacted health. Destructive data actions
name scope, consequences, recovery, and confirmation.

## macOS integration

- `Command-N`: new work; `Command-K`: command palette; `Command-F`: search;
  `Command-,`: settings; `Command-Shift-A`: toggle Activity;
  `Command-Option-Space`: quick entry when enabled.
- Menu commands mirror visible actions and update enabled state.
- Dock badges and notifications represent completed/attention Sessions only;
  clicking opens the exact Session. Notification content is private by default.
- Quick entry is a small companion window that selects context and hands off to
  the main window; it does not create an alternate Runtime.
- File, Accessibility, Screen Recording, Microphone, and Automation permissions
  are requested only at first use with preflight explanation and a route to
  System Settings. Denial leaves the rest of the app usable.
- All macOS-only extensions broker through the backend-owned XPC boundary in
  `desktop/AGENTS.md`; no extension embeds Engine or accesses the database.

## State, commands, and recovery

A-UX1 `AppViewState`, intents, effect identity, generation, Session ownership,
and exact retry rules are normative. Desktop adds composition values only:

```text
DesktopViewState {
  boot, capability_manifest, navigation, selected_session,
  conversation, activity, artifacts, composer, inspector,
  pending_decision, connection, notices, preferences
}
```

One mutation may be pending per Session. Reads and follows may coexist. A
navigation change never cancels work. On restart, H2 loads the fixed timeline,
then H1 follows after its watermark. Unknown command outcome offers exact retry
or read-by-command when available; it never allocates a replacement identity.

Client storage may contain theme, density, rail/inspector size, draft text,
last selected public Session, and dismissed education. It must not contain
credentials, configuration values, paths, raw facts, hidden context, authority
grants, artifact bytes, or invented durable state.

The current preference record is a maximum 256-byte, schema-versioned document
with exactly `schema_version`, `theme`, `density`, and `locale`. Version 2
admits only `system`, `en`, `zh-Hans`, or the QA-only `en-XA` locale. An exact
version 1 appearance record migrates in memory with `locale=system`; no other
legacy or widened shape migrates. Unknown keys, versions, values, malformed
JSON, or oversized records fail closed to `system`, `comfortable`, and
`locale=system`; the client never partially admits a record.

## Security and privacy

- Tauri allows only the main window and an explicit capability allowlist; CSP
  forbids remote script, unsafe eval, arbitrary navigation, and inline secrets.
- All IPC values are typed, bounded, versioned, and validated again in Rust.
- Native pickers return opaque, expiring, operation-scoped capabilities.
- External links require visible origin confirmation and open in the system
  browser; artifact previews use a separate sandboxed protocol/renderer.
- Logs, analytics, crash reports, clipboard, notifications, accessibility
  labels, window titles, and recent-menu items contain no private content by
  default.
- Updates are signed, notarized, verified before install, and preserve a
  rollback path. Downgrade never opens a newer database/config schema.

## Accessibility and localization

All workflows are complete with keyboard alone. Focus order follows visual
order; overlays trap and restore focus; new messages do not steal focus;
streaming uses one throttled polite live region; approvals use an assertive
summary only when blocked. Every icon has a label, every state has text, and
focus indicators meet 3:1 contrast. Text and essential controls meet WCAG 2.2
AA, zoom to 200%, VoiceOver rotor landmarks, reduced motion, increased contrast,
reduced transparency, and full keyboard access.

The native View menu exposes Zoom In (`Command-=`), Zoom Out (`Command--`),
and Actual Size (`Command-0`). These commands carry only closed, data-free
intents and call the supported native WebView zoom API at bounded 80%, 100%,
120%, 150%, 175%, and 200% steps. CSS transforms, viewport emulation, and
browser-only responsive captures do not satisfy the M72 native 200% gate.

UI copy is localized by stable keys with parameter bounds. Dates, numbers,
pluralization, and shortcuts use locale/platform formatting. Pseudolocalization
and CJK composition are release gates.

`locale=system` resolves live macOS preferences to Simplified Chinese for any
`zh` language tag and otherwise to English. Explicit locale selection takes
effect without restart, updates the document language exposed to assistive
technology, and never changes user-authored prompts, model output, filenames,
Workspace display names, Agent identifiers, receipt content, or other durable
facts. Missing catalogue entries fail visibly during tests; production must
never render a raw localization key.

The bundled frontend sends only the resolved `en`, `zh-Hans`, or QA `en-XA`
locale through a dedicated data-free command. The backend rejects `system`,
paths, and unknown values, then atomically rebuilds the complete native Garive,
File, Edit, View, and Window menus—including standard platform items—without
changing command identities or accelerators. The packaged capability manifest
must admit this command and every registered product command to the `main`
window only; direct Rust tests are not a substitute for that ACL parity gate.

English and Simplified Chinese are user-facing release locales. `en-XA` is an
expanded, accented pseudolocale available only in development/evidence modes or
when already selected by an admitted QA preference. It may test layout but may
not substitute for the required M75 Chinese journey. Every release candidate
exercises Setup, Work, Search, Activity, approval, Workspace picker/recovery,
Artifact preview/export, Agents, Settings, menus, errors and empty states in
both user-facing locales, including CJK IME composition and 200% zoom.

## Performance and operations

- first meaningful window <= 1.5 s p95 on a supported warm Mac;
- input-to-durable-pending feedback <= 100 ms p95 excluding Host commit;
- 60 fps scrolling for a 500-item bounded timeline;
- idle CPU <= 1%, idle private memory <= 250 MiB target;
- no unbounded DOM, event, log, preview, draft, or cache collection;
- recovery and migration run before Agent mutation admission and expose stable
  progress/failure states.

Release artifacts target macOS 14+, Apple Silicon and Intel universal builds.
The pipeline produces a signed/notarized `.app`, DMG, update manifest, SBOM,
license inventory, checksums, and rollback instructions. No completion claim is
made from `vite build` or `cargo check` alone.

## Acceptance

1. Fresh install completes C2 setup/restart and one real provider-backed Turn.
2. File-backed SQLite E2E creates two Sessions, runs concurrent work, restarts,
   reopens both timelines, reconnects without loss, and completes another Turn.
3. A governed local Workspace task reads scoped input, requests exact write or
   execute authority, produces an artifact, records verification, and previews
   it without a path crossing React IPC.
4. Crash matrices cover setup, command unknown outcome, running Turn,
   suspension, effect, artifact commit, and update/migration boundaries.
5. React tests cover boot/setup/home/recents/session/composer/activity/artifact/
   permissions/settings, empty/loading/error/stale/offline states, keyboard,
   IME, focus, reduced motion, contrast, zoom, and localization.
6. Source and runtime scans prove capability truth, no fake durable state, no
   secret/path/raw-fact exposure, strict CSP, bounded rendering, and no hidden
   environment/config/provider construction in frontend.
7. A packaged clean-machine test verifies launch, menu, shortcuts, permissions,
   notifications, sleep/wake, offline recovery, quit/reopen, signature,
   notarization, update refusal, and uninstall/data-retention behavior.
8. A-DESKTOP-VE provides the full-function screenshot manifest, journey-based
   visual review, and a versioned user manual whose procedures are replayed on
   the exact candidate package.

## Non-goals

The A-DESKTOP-WORK foundation does not itself imply cloud sync, team accounts,
voice, Scheduler, remote mobile access, or the separately accepted T2
Browser/Computer Use capability. Work does not grant broad disk, shell,
browser, Accessibility or input authority. Matching the product quality of
work-oriented competitors does not mean copying their cloud architecture,
visual identity, hidden model behavior, or unsupported feature catalogue.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: accepted
