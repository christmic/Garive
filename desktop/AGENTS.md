# desktop/AGENTS.md

> **Desktop Agent App.** Tauri (Rust backend + TypeScript / React
> frontend) is the **main** path. macOS-only SwiftUI / AppKit
> extensions under `macos-native/` exist **only for the macOS
> integration points Rust cannot reach** — menu bar, Spotlight,
> Quick Look, System Extensions, etc.
>
> Decision: client embeds the agent runtime on-device. The
> `backend/` Rust crate hosts `engine/` crates locally — no
> round-trip required for the agent loop.
>
> This file applies to everything under `desktop/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Layout

```
desktop/
├── backend/        Rust — Tauri shell (Cargo workspace member)
├── frontend/       TypeScript + React + Vite (pnpm)
└── macos-native/   SwiftUI / AppKit — ONLY for the macOS
                    integration points Tauri/Rust cannot reach
```

| Subdir | Role | Boundary |
|--------|------|----------|
| `backend/` | Tauri commands, IPC handlers, file-system / shell, OS integrations (notifications, tray). Embeds `engine/` and `runtime/replica`. | Main app process; ships on every supported platform. |
| `frontend/` | React UI rendered into the Tauri webview. Pure UI — never touches OS APIs; goes through Tauri IPC commands. | Main app webview; ships on every supported platform. |
| `macos-native/` | macOS-only SwiftUI / AppKit **extensions** for what Rust can't reach: Menu Bar agent, Spotlight plugin, Quick Look preview, System Extensions, Shortcuts actions. | **macOS only.** Each sub-project is a separate Xcode target, packaged inside the Tauri `.app` bundle. |

### What Lives in `macos-native/` (and what does NOT)

`macos-native/` exists **only** for the macOS surface area that
Tauri/Rust genuinely cannot reach. Examples that belong here:

- Menu Bar agent (NSStatusItem) — small SwiftUI popover
- Spotlight importer / actions
- Quick Look preview extension
- Finder Sync extension
- System Settings pane
- Apple Shortcuts actions
- Live Activities
- AppleScript / `NSAppleScript` integration
- Any NSService / XPC service that lives outside the main app

Examples that **do NOT** belong here — keep them in `backend/`
or `frontend/`:

- Main chat window
- Tool-call panels
- Settings dialogs
- File attachments and basic previews
- Notifications (Tauri's notification plugin covers them)
- Tray icon (Tauri's tray plugin covers it)
- Menu structure (Tauri's menu API covers it)

Rule of thumb: **if Tauri already has an API for it, use Tauri**.
Only when Tauri has no API — or its API is a known-broken thin
wrapper around the Objective-C bridge — does it move to
`macos-native/`.

### Why This Split (Day-One Decision)

- **Rust backend hosts the agent runtime.** That constraint
  alone forces `backend/` to be Rust. The frontend rides on top
  of the webview that Tauri provides.
- **macOS users see a coherent desktop app** — most interactions
  go through the same UI. SwiftUI extensions are siblings, not
  replacements.
- **Rust cannot reach** a small, well-defined set of macOS
  surfaces. We accept that and supplement with SwiftUI rather
  than try to bypass Tauri entirely (which would force us off
  Rust for the whole app).
- **Other platforms** (Windows / Linux) ship only `backend/` +
  `frontend/`. They do **not** get the macOS extensions and
  they do **not** need them.

### Cross-tier Contract (Tauri ↔ macos-native)

- macOS extensions are packaged **inside** the Tauri `.app`
  bundle (Xcode build phase copies them into
  `Contents/PlugIns/` or `Contents/Library/Services/`).
- Communication is **XPC** (NSXPCConnection) — not Tauri IPC.
  The macos-native extension runs in its own process; Tauri
  talks to it via XPC.
- The XPC interface is **a thin IDL file** (`.xcinterface`)
  generated from Rust types via `uniffi` or `swift-bridge`.
- macos-native never embeds `engine/*` directly. It brokers
  to the running Tauri backend via XPC.

## Stack

| Tier | Tech | Workspace |
|------|------|-----------|
| `backend/` | Rust 2021, Tauri 2.x, `engine/*` crates, `runtime/replica` | main (root Cargo) |
| `frontend/` | TypeScript (strict), React 19, Vite, Zustand or Jotai, TanStack Query | independent pnpm |
| `macos-native/` | Swift 6, SwiftUI (iOS 17+ / macOS 14+), AppKit where SwiftUI lacks APIs, XPC for IPC | independent Xcode projects, packaged into the Tauri `.app` |

## Layout Per Tier

### `desktop/backend/` (Rust, Tauri 2.x)

```
backend/
├── Cargo.toml              workspace member; depends on engine/*, runtime/replica
├── tauri.conf.json         Tauri app config (window, bundle, security, capabilities)
├── src/
│   ├── main.rs             app entry, Tauri builder, .app bundle post-processing
│   ├── commands/           one module per domain (agent, memory, ...)
│   └── ipc/                IPC types + serde glue
└── README.md
```

### `desktop/frontend/` (TS / React / Vite)

```
frontend/
├── package.json            pnpm-managed; tauri-apps/cli as devDep
├── pnpm-lock.yaml
├── tsconfig.json           strict mode on
├── vite.config.ts
├── index.html
├── src/
│   ├── main.tsx
│   ├── ipc/                typed wrappers (ts-rs-generated) around @tauri-apps/api
│   ├── ui/
│   ├── state/
│   └── routes/
└── README.md
```

### `desktop/macos-native/` (Swift / SwiftUI / AppKit)

```
macos-native/
├── Package.swift           or per-extension Xcode projects under sub-dirs
├── IDL/                    .xcinterface files + UniFFI-generated Swift bindings
├── MenuBarAgent/           (lands when needed) NSStatusItem + SwiftUI popover
├── Spotlight/              (lands when needed) importer + actions
├── QuickLook/              (lands when needed) preview extension
├── Shortcuts/              (lands when needed) App Intents / Shortcuts actions
└── README.md
```

Each sub-project inside `macos-native/` is its own Xcode target.
They are independent of each other and **independent of the root
Cargo workspace**. They are built and packaged by an Xcode
project (or `swift package` workspace) that lives inside
`macos-native/`.

## Wire Contracts

- **backend ↔ engine**: Rust path-dependency into `engine/`
  crates; **same process**.
- **backend ↔ runtime/gateway**: generated proto bindings from
  `spec/proto/`.
- **frontend ↔ backend**: typed IPC wrappers in
  `frontend/src/ipc/`, generated from Rust IPC types (via
  `ts-rs`).
- **macos-native ↔ backend**: XPC. IDL is a `.xcinterface`
  generated from a Rust surface via UniFFI; Swift side imports
  the generated bindings.

## Verification

Each slice lands Red-Green-Refactor per `.agents/ddd.md`:

- **3a. Test first.** A Rust integration test in `backend/tests/`
  (or `macos-native/<X>/Tests/`) referencing the fixture in
  `spec/fixtures/`.
- **3b. Implement.** Minimal command + handler.
- **3c. Refactor.** Move invariants into the aggregate root in
  `engine/`; expose them via the Tauri command / XPC layer.

The first Tauri shell embeds the durable local Runtime behind typed IPC. Its
frontend is deliberately minimal until product UI and backend-only Garive
configuration provisioning slices land.

## Build

```
just desktop                       # cargo-checks the active Tauri shell
```

The macOS release pipeline additionally runs the Xcode project
inside `macos-native/` and packages the resulting `.appex` /
`.bundle` into `Contents/PlugIns/` and `Contents/Library/
Services/` of the Tauri `.app`.

## What NOT to Do

- ❌ Don't put business logic in the frontend or in
  `macos-native/`. Logic lives in `engine/`; both Tauri and
  macos-native just bridge it.
- ❌ Don't reach for `macos-native/` when Tauri already has an
  API for it. Default to Tauri; only escalate to SwiftUI when
  there is no other option.
- ❌ Don't expose filesystem / shell / OS APIs to the frontend
  except through a `#[tauri::command]`.
- ❌ Don't hand-write DTOs that mirror proto fields. Use
  generated bindings.
- ❌ Don't bundle a Chromium runtime. Tauri uses the system
  webview.
- ❌ Don't add a Rust crate under `desktop/macos-native/` —
  Swift / SwiftUI / Xcode only.

## Testing

This tier follows the test pyramid in `.agents/testing.md`.
For `desktop/`:

| Layer | Where | What |
|-------|-------|------|
| Static (Rust) | `desktop/backend/` | `cargo fmt --check`, `cargo clippy -- -D warnings` |
| Static (TS) | `desktop/frontend/` | ESLint + `tsc --noEmit` |
| Unit/contract (Rust) | `desktop/backend/tests/` | Commands through the public library boundary; no test modules in `src/`. |
| Unit (TS) | `desktop/frontend/src/ipc/`, `state/`, `ui/` | component + state-store tests |
| Integration (Rust) | `desktop/backend/tests/` | `#[tauri::test]` calling commands end-to-end inside the Tauri runtime |
| E2E | `tests/e2e/desktop/` (Playwright against the built Tauri webview, or `tauri-driver`) | the app boots, IPC round-trips, native bridge works |

`macos-native/` (SwiftUI) has its own XCUITest target per
sub-project; tests live inside each sub-project's Xcode
target.
