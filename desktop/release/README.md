# Garive macOS release gates

Garive has two deliberately different packaging lanes. Neither lane reads
credentials from source files, tracked configuration, command arguments, or
frontend state.

## Process-isolation XPC service

Build the service executable, then assemble it with explicit signed metadata:

```sh
swift build --package-path desktop/macos-native \
  --configuration release --product GariveProcessIsolationService
python3 desktop/release/build_process_xpc.py \
  --executable desktop/macos-native/.build/release/GariveProcessIsolationService \
  --output target/GariveProcessIsolationService.xpc \
  --bundle-identifier com.garive.desktop.process-isolation-service \
  --bundle-version 1 \
  --short-version 0.1.0 \
  --backend-requirement 'identifier "com.garive.desktop" and anchor apple generic' \
  --signing-identity 'Developer ID Application: Garive' \
  --codesign-tool /usr/bin/codesign
```

The builder rejects overwrites, ambient configuration and an inexact bundle
identifier. It signs with the supplied identity, verifies the signature and
executable identifier, and admits only the exact plist, executable and signing
seal layout. An ad-hoc run validates assembly mechanics but is not V0-C1
production-signing evidence. The verified service belongs at
`Garive.app/Contents/XPCServices/GariveProcessIsolationService.xpc`; embedding
and parent-app signature verification remain part of the release gate.

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

The workspace pins Cargo's documented `strip = "none"` release default. This
avoids Cargo 1.98's deferred `rust-objcopy` optimization becoming an undeclared
host dependency; release symbols remain available for crash diagnosis. The
local verifier reports the exact architectures and SHA-256 for every candidate.

## Public release admission

The release runner must start from an exact clean Git revision and supply a
Developer ID Application identity plus App Store Connect API-key notarization
values through the protected CI environment expected by Tauri. Build the
`universal-apple-darwin` target and both bundles, then run:

```sh
python3 desktop/release/build-updater-config.py \
  --endpoint 'https://public.example/releases/{{target}}/{{arch}}/{{current_version}}' \
  --public-key /protected/public/garive-updater.pub \
  --output target/release-config/updater.json
```

The generator accepts one or two bounded public HTTPS channels and an exact
Minisign public-key document. It rejects credentials, fragments, localhost/IP
literals, symlinked or malformed keys, external output, and overwrites. Build
with `tauri build --config target/release-config/updater.json`; the protected
runner supplies `TAURI_SIGNING_PRIVATE_KEY` and its password without writing
them to the overlay, source tree, logs, or evidence.

After Tauri emits the signed Universal `.app.tar.gz` and adjacent `.sig`, bind
both macOS updater targets to that exact archive from the same clean revision:

```sh
python3 desktop/release/build-update-manifest.py \
  --archive target/universal-apple-darwin/release/bundle/macos/Garive.app.tar.gz \
  --signature target/universal-apple-darwin/release/bundle/macos/Garive.app.tar.gz.sig \
  --archive-url 'https://releases.example.com/garive/Garive.app.tar.gz' \
  --notes 'Garive 0.1.0' \
  --output target/desktop-release/latest.json
```

The static manifest uses the configured stable version and commit timestamp,
embeds the Tauri-required base64 form of the exact Minisign document, and maps
`darwin-aarch64` and `darwin-x86_64` to the same Universal archive. The
generator rejects dirty Git state, malformed or non-adjacent signatures,
non-public/mismatched URLs, symlinks, files outside `target/`, and overwrites.

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

Bind the verified Universal DMG to deterministic supply-chain materials from
the same clean Git revision:

```sh
python3 desktop/release/build-release-materials.py \
  target/universal-apple-darwin/release/bundle/dmg/Garive_0.1.0_universal.dmg \
  --updater-archive target/universal-apple-darwin/release/bundle/macos/Garive.app.tar.gz \
  --updater-signature target/universal-apple-darwin/release/bundle/macos/Garive.app.tar.gz.sig \
  --update-manifest target/desktop-release/latest.json \
  --updater-config target/release-config/updater.json
```

The generator reruns the bundle audit, requires exactly the `arm64` and
`x86_64` slices, and writes a CycloneDX 1.6 SBOM, production third-party
license inventory, `SHA256SUMS`, provenance, and rollback boundary under
`target/desktop-release/<digest-prefix>`. It rejects dirty revisions,
symlinks, external packages/output, undeclared dependency licenses, digest
mismatches, and overwrites. Pass `--mode release` only for the signed and
notarized candidate because that mode invokes every public release gate and
requires all four mutually bound updater inputs. Their exact digests and
public archive URL are included in provenance and `SHA256SUMS`.

Publication additionally requires the update manifest/signature, SBOM, license
inventory, SHA-256 checksum publication, rollback instructions, and a clean-Mac
install/update/downgrade test. Until those artifacts and a real Apple identity
exist, `spec/STATUS.md` remains active and no signed-release claim is valid.
Garive Desktop implements a signed, no-downgrade updater lifecycle, but local
builds intentionally contain no channel or public key. Generated materials do
not substitute for a protected signing run or update/downgrade evidence.

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

Render the screenshot-bound Chinese manual draft from the repository root with
the bundled PDF runtime (or another Python environment containing ReportLab):

```sh
python3 desktop/release/build-desktop-manual.py
python3 desktop/release/build-desktop-manual.py --tagged
```

The default output is `output/pdf/garive-macos-user-guide-draft.pdf`. The
builder fails closed when the manual's screenshot placeholders differ from the
accepted evidence spec and replaces the PDF atomically. This draft validates
layout, extractable text, navigation, and evidence placement only. The tagged
lane additionally requires `soffice` plus Pypdf, configures an isolated macOS
CJK font cache, exports PDF/UA mode, and normalizes the document language to
`zh-CN`. Neither draft is public while placeholders remain; `Tagged: yes` and a
structure tree are necessary evidence, not a PDF/UA conformance or VoiceOver
reading-order result, so those still require independent final gates.
