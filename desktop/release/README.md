# Garive macOS release gates

Garive has two deliberately different packaging lanes. Neither lane reads
credentials from source files, tracked configuration, command arguments, or
frontend state.

## Local runnable evidence

From the repository root:

```sh
cd desktop/backend
APPLE_SIGNING_IDENTITY=- ../frontend/node_modules/.bin/tauri build --bundles dmg
cd ../..
desktop/release/verify-macos-bundle.sh \
  target/release/bundle/dmg/Garive_0.1.0_aarch64.dmg local
```

This lane must produce a resource-sealed ad-hoc signature with hardened runtime
and a checksum-valid DMG. It proves a locally runnable package, not Gatekeeper,
Developer ID, notarization, universal architecture, or update eligibility.

Universal local evidence additionally requires both Rust macOS targets and a
single consistent rustup toolchain. If Homebrew Rust precedes rustup, bind
Tauri's subprocesses explicitly:

```sh
rustup target add x86_64-apple-darwin
cd desktop/backend
CARGO="$(rustup which cargo)" RUSTC="$(rustup which rustc)" \
  APPLE_SIGNING_IDENTITY=- ../frontend/node_modules/.bin/tauri build \
  --target universal-apple-darwin --bundles dmg
cd ../..
desktop/release/verify-macos-bundle.sh \
  target/universal-apple-darwin/release/bundle/dmg/Garive_0.1.0_universal.dmg local
```

The verified local Universal DMG on 2026-08-30 contained `x86_64 arm64` and had
SHA-256 `bfa68aed3ea5fdfe74d092402c80581baea1ce63eaa40f3602f3d2bcafae7f71`.
This host's rustup `rust-objcopy` could not load its `libLLVM.dylib`, so strip
warnings remain a release-CI failure even though bundle construction succeeded.

## Public release admission

The release runner must start from an exact clean Git revision and supply a
Developer ID Application identity plus App Store Connect API-key notarization
values through the protected CI environment expected by Tauri. Build the
`universal-apple-darwin` target and both bundles, then run:

```sh
desktop/release/verify-macos-bundle.sh path/to/Garive.dmg release
```

The release audit requires all of the following:

- a non-ad-hoc sealed signature and hardened runtime;
- both `arm64` and `x86_64` slices;
- Gatekeeper execution assessment;
- a stapled notarization ticket;
- the exact `com.garive.desktop` identifier and checksum-valid DMG.

The build log must also be warning-free; failed stripping, missing architecture,
skipped signing, or skipped notarization is a failed public-release run.

Publication additionally requires the update manifest/signature, SBOM, license
inventory, SHA-256 checksum publication, rollback instructions, and a clean-Mac
install/update/downgrade test. Until those artifacts and a real Apple identity
exist, `spec/STATUS.md` remains active and no signed-release claim is valid.

## Screenshot and manual evidence

The accepted M01–M85 matrix is tracked without duplicating its ID list in code.
Start from a clean candidate revision and initialize its exact DMG once:

```sh
node desktop/release/initialize-desktop-evidence.mjs \
  target/universal-apple-darwin/release/bundle/dmg/Garive_0.1.0_universal.dmg \
  15.6
```

The initializer rejects packages outside this checkout's `target/`, symlinks,
dirty Git state, non-DMG input, invalid macOS versions, admitted captures, and
an already initialized manifest. Use `--dry-run` to print the derived candidate
identity without changing the manifest. Then run the gate:

```sh
node desktop/release/verify-desktop-evidence.mjs
```

The verifier derives the exact required IDs from A-DESKTOP-VE, rejects missing,
duplicate, extra, or pending rows, binds every passing image to the candidate
Git/package identity, checks required provenance and safety declarations, and
recomputes every PNG SHA-256 under `docs/manual/assets/desktop`. The checked-in
manifest intentionally starts red: it becomes green only after the real
candidate capture matrix is complete.
