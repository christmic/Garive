# Dependency and toolchain governance

> Garive uses the newest stable toolchain and SDK release that satisfies the
> admitted platform contract. Manifests and lockfiles are the version SSOT;
> this rule defines how versions are selected, reviewed, and verified.

## Stable-only admission

- Resolve versions from the publisher's official release page or canonical
  registry. Search snippets, transitive resolution, and local caches are not
  version evidence.
- `alpha`, `beta`, `rc`, `eap`, `nightly`, snapshots, mutable Git branches,
  dynamic ranges such as `+`, and unpinned Git revisions are forbidden in
  product builds. A focused research experiment may use one only when its
  Spec records the reason, exact immutable revision, owner, and removal gate.
- Select the newest stable patch of the newest stable line compatible with the
  accepted OS/API floor and every directly coupled build tool. Compatibility
  outranks recency; a deliberate hold must be documented beside the pin with
  the blocking upstream issue and a review date.
- A version is not admitted until the repository's native build and tests pass.
  Publication date or successful dependency resolution alone is insufficient.

## Version ownership

| Ecosystem | Version SSOT | Reproduction lock |
|---|---|---|
| Rust | root workspace manifest and `rust-toolchain.toml` | `Cargo.lock` |
| Kotlin/Android | version catalog or root plugin block plus Gradle Wrapper | dependency locking/checksums |
| TypeScript | each shipping package manifest | `pnpm-lock.yaml` |
| Swift | `Package.swift` tool/platform declarations | `Package.resolved` when packages exist |
| Protocol SDK evidence | focused protocol Spec | exact official tag/commit and inspected paths |

Do not repeat a shared version in leaf manifests. BOM-managed libraries omit
individual versions. Generated output never becomes the version owner.

## Upgrade procedure

1. Record the official source URL, stable version, and review date in the
   change description or focused Spec when protocol behavior is affected.
2. Update the owning manifest and lock/checksum artifact in one change.
3. Read release notes for breaking, security, wire, persistence, and minimum
   platform changes; migrate code explicitly.
4. Run formatting, lint, unit/contract tests, production builds, and the native
   platform gate affected by the upgrade.
5. Reject or revert an upgrade that weakens an accepted invariant. Record a
   temporary hold beside the version instead of silently retaining an old pin.

## Runtime configuration boundary

Dependency versions are build-time facts. Endpoints, credentials, model IDs,
timeouts, limits, and feature policy are Runtime-owned construction inputs;
Engine, protocol adapter, Provider mapping, and SDK modules must not discover
them from process environment variables. Shipping secrets use an injected OS
credential resolver. Test values are explicit fixtures or constructor inputs.

## Review cadence

- Review toolchains, direct dependencies, SDK/API levels, and lockfiles before
  every release and at least once every 30 days during active development.
- Security releases trigger an immediate bounded review.
- CI fails on prerelease/dynamic dependencies, a missing required lockfile, or
  an undocumented compatibility hold.
