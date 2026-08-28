# desktop/macos-native/

> **macOS-only SwiftUI / AppKit extensions** for the integration
> points Tauri/Rust cannot reach.

This directory is **not** part of the Rust workspace. It is an
independent set of Xcode targets that ship inside the Tauri
`.app` bundle. Each sub-project addresses a single macOS
surface.

## When to Add a Sub-project Here

Add a sub-project **only when** Tauri has no API for the
surface you need (or its API is broken / laggard). Examples
that have historically required native extensions:

| Surface | Why Tauri is not enough |
|---------|------------------------|
| `MenuBarAgent/` — NSStatusItem menu bar agent | Tauri tray is process-bound; menu-bar long-lived agents need an out-of-process agent with a SwiftUI popover |
| `Spotlight/` — importer / actions | CoreSpotlight requires an in-process importer extension (`.appex`) |
| `QuickLook/` — preview thumbnails | Quick Look is a separate extension target (`QLGenerator`) |
| `FinderSync/` — Finder sidebar badges | Finder Sync is a separate extension target |
| `Shortcuts/` — App Intents | App Intents extensions are Xcode targets, not in-process APIs |
| `SystemSettings/` — settings pane | macOS System Settings pane is a separate `PreferencePanes` bundle |
| `LiveActivities/` — Lock-screen Live Activity | ActivityKit requires a WidgetKit extension target |

## When NOT to Add Anything Here

If Tauri has it, use Tauri:

- Main window → `backend/` + `frontend/`
- Notifications → `tauri-plugin-notification`
- Tray icon → Tauri tray API (one process)
- File dialogs → `tauri-plugin-dialog`
- Auto-updater → `tauri-plugin-updater`
- Menu structure → Tauri menu API

## IPC with the Tauri App

- The macos-native extension runs as its **own process** inside
  the Tauri `.app` bundle.
- Communication is **XPC** (NSXPCConnection), not Tauri IPC.
- The XPC interface is described in a `.xcinterface` file in
  `IDL/`.
- IDL types are **generated** from a Rust surface via UniFFI
  (`uniffi_macros::export`) so the Swift side imports the same
  shape as the Rust side.

## Don't Embed `engine/` Here

`macos-native/` never directly depends on `engine/*` crates.
It brokers to the running Tauri backend via XPC. This keeps
the Rust dependency tree manageable and avoids FFI duplication.

## Build

`cd desktop/macos-native && xcodebuild -scheme <Extension>`

The Tauri release pipeline packages the resulting `.appex` /
`.bundle` into the right slot of the `.app` bundle.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
