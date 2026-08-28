# desktop/frontend/

> **Tauri webview, TypeScript + React + Vite.** Presentation
> layer of the Desktop Agent App. **No OS access from here** —
> every OS-side capability goes through a Tauri command in
> `../backend/`.

This directory is a **pnpm project** (independent of the Rust
workspace). The Tauri CLI (`pnpm tauri build`) orchestrates
frontend + backend builds.

## What Goes Here

| Allowed | Examples |
|---------|----------|
| React components, screens, themes | `App.tsx`, `AgentChat.tsx`, theme tokens |
| Client-state stores | Zustand / Jotai / Redux (pick one) |
| Server-state caching | TanStack Query, SWR |
| Typed IPC wrappers | `invoke('agent.start', args)` with full TS types |
| Routing | React Router, TanStack Router |

| Forbidden | Why |
|-----------|-----|
| Direct `fetch` to internal services | backend is the only network egress |
| `fs` / `path` / `shell` access | OS APIs go through Tauri commands |
| Business logic | logic lives in `engine/`; UI calls into it via Tauri |
| Hand-written IPC types | generated from backend via `ts-rs` |

## Layout

```
frontend/
├── package.json              pnpm-managed; tauri-apps/cli as devDep
├── pnpm-lock.yaml
├── tsconfig.json             strict mode on
├── vite.config.ts
├── index.html                Tauri webview entry
├── src/
│   ├── main.tsx              React root
│   ├── ipc/                  generated TS wrappers around @tauri-apps/api
│   │   ├── agent.ts          invoke('agent.start', ...) + types
│   │   └── ...
│   ├── ui/                   components, screens
│   ├── state/                client-state stores
│   └── routes/               router
└── README.md
```

## IPC Discipline

- All `@tauri-apps/api` `invoke` calls live in `src/ipc/`.
  Components never call `invoke` directly.
- The IPC layer's TypeScript types are generated from the Rust
  IPC types via `ts-rs` (or a similar tool). Don't hand-write
  parallel structs — they will drift.

## Stack

- TypeScript (strict), React 19, Vite.
- pnpm for package management.
- ESLint + `tsc --noEmit` in CI.

## Build

`just desktop` (orchestrates frontend + backend via Tauri CLI).

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: stub — slice not yet landed; content is scaffolding.
