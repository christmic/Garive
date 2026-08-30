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

Publication additionally requires the update manifest/signature, SBOM, license
inventory, SHA-256 checksum publication, rollback instructions, and a clean-Mac
install/update/downgrade test. Until those artifacts and a real Apple identity
exist, `spec/STATUS.md` remains active and no signed-release claim is valid.
