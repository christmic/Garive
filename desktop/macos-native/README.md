# Garive macOS native extensions

This independent Swift package contains only macOS integration that the Tauri
backend cannot implement faithfully. It never embeds Engine, opens the Ledger,
decides authority, or reads Runtime configuration from the environment.

`GariveComputerUse` currently implements side-effect-free Accessibility and
Screen Recording permission preflight. It does not prompt, enumerate targets,
capture pixels, inject input, or claim an XPC service. Those capabilities land
only with their accepted target-identity, wire, broker and packaged-app tests.

Build and test with the stable toolchain pinned by `Package.swift`:

```sh
swift test --package-path desktop/macos-native
```
