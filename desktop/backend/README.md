# Desktop backend

Tauri 2 Rust composition shell. `run_fake_host` invokes the shared Runtime fake
Host and returns assembled output through a fallible IPC command. UI code stays
in `../frontend`; OS and Runtime capabilities stay behind Tauri commands.

```text
cargo test -p garive-desktop
cargo check -p garive-desktop
```

The command has a native fake-host test. Durable live Runtime composition,
window signing and distribution remain later slices.

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: executable Tauri shell
