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
SQLite. At Tauri setup, the backend reads the bounded `desktop-v1.json` from
Tauri's OS app-config directory and resolves its opaque `credential_ref` from
the OS credential-store service `com.garive.desktop`. Missing configuration
reports `not_configured`; invalid present configuration aborts startup with a
stable code. Frontend input and process environment never supply configuration.

The exact schema and failure rules are specified in
`../../spec/design/desktop-system-configuration.md`.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable Tauri shell
