# Garive macOS native extensions

This independent Swift package contains only macOS integration that the Tauri
backend cannot implement faithfully. It never embeds Engine, opens the Ledger,
decides authority, or reads Runtime configuration from the environment.

`GariveComputerUse` currently implements side-effect-free Accessibility and
Screen Recording permission preflight plus the XPC listener's caller-admission
primitive. The latter validates an explicit bounded Security requirement,
installs it into `NSXPCListener` before activation, and requires the exact
effective user and login audit session after system signature admission. It
does not read configuration from the environment.

Application targets use a separate verifier. It binds a running PID to process
start time, a validated signing identifier and CodeDirectory hash, validates
the dynamic code before and after evidence collection, and can re-resolve that
exact instance before every AX observation or input. It never admits a bundle
name or PID alone.

The package does not yet enumerate application targets, capture pixels, inject
input, expose the production XPC IDL, or claim a packaged XPC service. Those
capabilities land only with their accepted target-identity, wire, broker and
packaged-app tests.

Build and test with the stable toolchain pinned by `Package.swift`:

```sh
swift test --package-path desktop/macos-native
```
