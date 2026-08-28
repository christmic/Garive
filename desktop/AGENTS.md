# desktop/AGENTS.md

> **Desktop Agent App.** Tauri shell, Rust backend (in the
> workspace), TypeScript / React frontend (independent pnpm
> workspace).
>
> This file applies to everything under `desktop/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Layout

```
desktop/
├── backend/      Rust — Tauri shell (workspace member of the root Cargo workspace)
└── frontend/     TypeScript + React + Vite (independent pnpm project)
```

| Subdir | Role |
|--------|------|
| `backend/` | Tauri commands, IPC handlers, file-system / shell access, OS integrations (notifications, tray, deep-link). Talks to `engine/` and `runtime/gateway/` over the wire schema in `spec/proto/`. |
| `frontend/` | React UI rendered into the Tauri webview. Pure UI — never calls OS APIs directly; goes through Tauri IPC commands defined in `backend/`. |

## Why Tauri (not Electron)

- **Smaller binary / faster cold start** — system webview vs.
  bundled Chromium.
- **Rust backend** — reuses `engine/` crates directly. The
  desktop app embeds the agent runtime; no IPC bridge to a
  separate process unless we deliberately want one.
- **Memory footprint** — orders of magnitude lower than
  Electron-based shells.

## Cross-tier Contract

| Frontend | → | Backend |
|----------|---|---------|
| `@tauri-apps/api` `invoke` | → | `#[tauri::command]` handler |

- Every OS-side capability exposed to the UI is a `#[tauri::command]`.
  Frontend never touches the filesystem / shell / OS APIs directly.
- Command names: `<domain>.<verb>` (`agent.start`, `memory.store`).
- Command arguments and return values are serde-serializable
  structs. **Use the generated proto bindings for cross-process
  payloads**, not ad-hoc DTOs.

## Stack

| Tier | Tech |
|------|------|
| Backend | Rust 2021, Tauri 2.x, `engine/` crates, `spec/proto/` bindings |
| Frontend | TypeScript (strict), React 19, Vite, Zustand or Jotai for state, TanStack Query for server-state |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` (Rust), `eslint` + `tsc --noEmit` (TS) |

## Layout Per Tier

### `desktop/backend/` (Rust, Tauri 2.x)

```
backend/
├── Cargo.toml              workspace member; depends on engine/*, runtime/replica
├── tauri.conf.json         Tauri app config (window, bundle, security)
├── src/
│   ├── main.rs             app entry, Tauri builder
│   ├── commands/           one module per domain (agent, memory, ...)
│   └── ipc/                IPC types + serde glue
└── README.md
```

### `desktop/frontend/` (TS / React / Vite)

```
frontend/
├── package.json            pnpm-managed; tauri-apps/cli as devDep
├── tsconfig.json           strict mode on
├── src/
│   ├── main.tsx            React root
│   ├── ipc/                typed wrappers around @tauri-apps/api invoke
│   ├── ui/                 components, screens, themes
│   ├── state/              client-state stores
│   └── routes/             router
└── README.md
```

## Wire Contracts

- Backend ↔ engine: Rust path-dependency into `engine/` crates;
  no IPC. Same process.
- Backend ↔ runtime/gateway: generated proto bindings from
  `spec/proto/`.
- Frontend ↔ Backend: typed IPC wrappers in `frontend/src/ipc/`,
  declared once; the wire format mirrors `backend/src/ipc/`
  exactly. Generated (e.g. via `ts-rs`) — no hand-written
  parallel types.

## Verification

Each slice lands Red-Green-Refactor per `.agents/ddd.md`:

- **3a. Test first.** A unit test in `backend/src/commands/` (or
  `frontend/src/ipc/`) referencing the fixture in
  `spec/fixtures/`.
- **3b. Implement.** Minimal command + UI handler.
- **3c. Refactor.** Move invariants into the aggregate root in
  `engine/`; expose them via the Tauri command layer.

`just desktop` builds both tiers; `just conformance` is the
sync gate for any change that touches the wire types.

## Build

```
just desktop
```

(equivalent to `cd desktop/frontend && pnpm tauri build`,
which orchestrates both frontend and backend via the Tauri CLI).

## What NOT to Do

- ❌ Don't put business logic in the frontend. Frontend is
  presentation + user input only.
- ❌ Don't expose filesystem / shell / OS APIs to the frontend
  except through a `#[tauri::command]`.
- ❌ Don't hand-write DTOs that mirror proto fields. Use
  generated bindings.
- ❌ Don't bundle a Chromium runtime. Tauri uses the system
  webview.
- ❌ Don't add the `backend/` Cargo crate to the workspace
  outside the root `Cargo.toml` `[workspace.members]`. The
  frontend is **not** a Cargo member; it is a pnpm project.