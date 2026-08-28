# desktop/backend/

> **Tauri shell, Rust.** OS-side capabilities of the Desktop
> Agent App. Embeds `engine/` crates directly; talks to
> `runtime/gateway/` over the wire schema in `spec/proto/`.

This directory is a **Cargo workspace member** of the root
workspace — it is listed in the root `Cargo.toml` `[workspace]
members` table. It is the only Rust crate under `desktop/`.

## What Goes Here

| Allowed | Examples |
|---------|----------|
| Tauri commands (`#[tauri::command]`) | `agent_start`, `memory_store` |
| IPC types (serde) | request / response structs shared with the frontend |
| OS integrations | tray icon, notifications, deep-link handlers, file dialogs |
| Window management | Tauri window / webview config |

| Forbidden | Why |
|-----------|-----|
| React / JS / HTML / CSS | UI lives in `frontend/` |
| Business logic | logic lives in `engine/`; backend just bridges it |
| Direct `std::fs` / shell access from a frontend `invoke` | always behind a `#[tauri::command]` |

## Layout

```
backend/
├── Cargo.toml              workspace member; depends on engine/*, runtime/replica
├── tauri.conf.json         Tauri 2.x config
├── build.rs                Tauri build hook
├── icons/                  app icons
├── src/
│   ├── main.rs             app entry, Tauri builder
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── agent.rs       agent.start, agent.stop, agent.ping
│   │   ├── memory.rs      memory.store, memory.recall
│   │   └── ...
│   └── ipc/
│       └── mod.rs          serde IPC types — generated via ts-rs into ../frontend/src/ipc/
└── README.md
```

## Conventions

- One `#[tauri::command]` per file in `commands/<domain>.rs`.
- Command names use `<domain>.<verb>` snake_case.
- Every command returns `Result<T, CommandError>` — never panic
  to the frontend.
- State is held in a `tauri::State<...>` struct, not in module
  statics.

## Depends On

- `engine/*` crates (path deps).
- `runtime/replica` (only if the backend acts as a replica).
- `spec/proto/` (generated bindings).

## Build

`just desktop` (orchestrates frontend + backend via Tauri CLI).