# Desktop backend

Tauri 2 Rust composition shell. `DesktopHost` embeds Live Host, SQLite, the
bounded local worker and a constructed model port. `run_agent_turn` returns a
typed durable terminal through IPC. UI code stays in `../frontend`; Provider
configuration and Runtime capabilities remain backend-only.

```text
cargo test -p garive-desktop
cargo check -p garive-desktop
```

Native tests run the complete embedded model-only loop against temporary
SQLite. The shipping state reports `not_configured` until the Garive backend
configuration subsystem installs the explicit composition; it never reads
credentials from frontend input or process environment.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable Tauri shell
