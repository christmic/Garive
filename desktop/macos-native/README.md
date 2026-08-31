# Garive macOS native extensions

This independent Swift package contains only macOS integration that the Tauri
backend cannot implement faithfully. It never embeds Engine, opens the Ledger,
decides authority, or reads Runtime configuration from the environment.

`GariveComputerUse` implements side-effect-free Accessibility and Screen
Recording permission preflight plus the XPC listener's caller-admission
primitive. The latter validates an explicit bounded Security requirement,
installs it into `NSXPCListener` before activation, and requires the exact
effective user and login audit session after system signature admission. It
does not read configuration from the environment.

Application targets use a separate verifier. It binds a running PID to process
start time, a validated signing identifier and CodeDirectory hash, validates
the dynamic code before and after evidence collection, and can re-resolve that
exact instance before every AX observation or input. It never admits a bundle
name or PID alone.

The AX observer enumerates windows only after permission and application
identity checks, retains each exact AX window object behind a broker-private
binding, and revalidates it before and after bounded semantic observation. Its
iterative projection rejects cycles, enforces node/text limits, exposes only a
closed portable action set and never reads secure text values.

Each observation also retains the exact AX object behind every snapshot-local
node index. Native `press`, non-secure `set_value`, focused Unicode `type_text`
and closed portable keys rebuild and compare the whole projection, revalidate
the selected object, atomically consume the old binding, dispatch once, and
require a new bounded observation. Keyboard input additionally proves that the
signed application remains frontmost and the retained AX window remains its
exact focused window. CoreGraphics events use a private source and
`CGEvent.postToPid` for the verified process, never the clipboard or a global
session event tap. Revoked permission, changed focus, stale semantics, replaced
nodes and protected values fail before dispatch; missing post-dispatch evidence
is uncertain rather than replayable.

The package does not yet capture pixels, inject scroll/pointer input, expose the
production XPC IDL, or claim a packaged XPC service. Real permission-granted
keyboard injection remains a packaged-app evidence gate. Those capabilities
land only with their accepted wire, broker and packaged-app tests.

Build and test with the stable toolchain pinned by `Package.swift`:

```sh
swift test --package-path desktop/macos-native
```
