# Desktop frontend

Strict TypeScript, React and Vite presentation shell for the Tauri desktop app.
OS/runtime access is isolated under `src/ipc/`; React components do not call
Tauri directly. The typed command returns durable Session, Turn, Execution,
cursor, terminal and committed text values from the embedded backend Runtime.

```text
pnpm install --frozen-lockfile
pnpm test
pnpm build
```

The production build writes ignored assets to `dist/`, which the Tauri backend
loads through `frontendDist`.
