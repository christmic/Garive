# mobile/AGENTS.md

> **Mobile Agent Apps.** Kotlin Multiplatform shares client logic,
> Jetpack Compose serves Android, and SwiftUI serves iOS. This is the target
> product structure; build and release gates land slice by slice.
>
> This file applies to everything under `mobile/`. It overrides
> the root `AGENTS.md` where the two disagree.

@AGENTS.md

## Layout

```
mobile/
├── shared/         KMP module — shared business logic only (no UI)
├── androidApp/     Android app — Jetpack Compose UI
└── iosApp/         iOS app — SwiftUI UI (Xcode project)
```

| Subdir | Role |
|--------|------|
| `shared/` | KMP `commonMain` + platform transports. Holds the app controller, immutable views, Host mapping, preference ports, and generated protocol bindings. **No UI, Engine, Runtime store, tool registry, or Memory/Knowledge database.** |
| `androidApp/` | Android app module. Jetpack Compose UI, Android-specific glue (push, permissions, notifications). Depends on `shared/`. |
| `iosApp/` | iOS app. SwiftUI uses the generated KMP XCFramework for synchronous Host v1 calls. Add SKIE when a later slice exports coroutines or flows. Depends on `shared/`. |

## Why Native UI (not Compose Multiplatform)

Garive's mobile surface is an **Agent App**, not a simple form.
SwiftUI on iOS + Jetpack Compose on Android is the recommended
choice because:

- **Performance** — SwiftUI renders via Metal; iOS Agent App
  surfaces (token-streaming chat, image / video panels) demand
  the best text + media pipeline.
- **Ecosystem** — Apple's new SDKs (Apple Intelligence, Live
  Activities, visionOS) default to SwiftUI first.
- **Risk isolation** — UI regressions don't block business-layer
  releases, and vice versa.
- **Shared logic is where the value lives** — KMP shares the
  Host-client controller, retry/reconnect semantics and immutable
  product views. Runtime, tools, Memory and Knowledge remain behind
  the Host boundary. Forcing UI into KMP doubles platform-specific
  risk for marginal reuse.

## Cross-platform Boundaries

The `shared/` module is the **only** mobile tier that knows application
workflow/reducer logic. Agent domain policy stays behind Host/Runtime. UI tiers
(`androidApp/`, `iosApp/`) are thin:

- **Compose** calls `shared/` through plain Kotlin interfaces.
- **SwiftUI** calls synchronous v1 functions through the generated framework;
  future `Flow`/`suspend` APIs use SKIE rather than hand-written conversion.

Anything that has to exist on both platforms goes in
`shared/commonMain/`. Anything that's Android-only goes in
`shared/androidMain/` or `androidApp/`. Anything that's
iOS-only goes in `shared/iosMain/` or `iosApp/`.

## Wire Contracts

`shared/` consumes the generated Kotlin bindings from
`spec/proto/` via the Gradle protobuf plugin. Hand-written
request / response types mirroring `.proto` fields are
forbidden — see `spec/AGENTS.md`.

## Verification

Each slice lands Red-Green-Refactor per `.agents/ddd.md`:

- **3a. Test first.** A unit test in `shared/commonTest/` (or
  the platform-appropriate test source-set) referencing the
  fixture in `spec/fixtures/`.
- **3b. Implement.** Minimal logic in `shared/commonMain/` to
  make the test pass.
- **3c. Refactor.** Move invariants into the aggregate root,
  push I/O behind a repository interface.

Android Compose and iOS SwiftUI call the shared live H1 client. KMP, iOS and
the Android SDK 36 APK are verified natively; the Android device gate uses an
API 36 Compose instrumentation test.

H1 is loopback-only. A mobile fixture or on-device test may inject a same-process
Host transport, but the shipping UI must not imply that a physical device can
reach a Desktop Host. Live remote connectivity starts only after an accepted
authenticated Gateway or on-device Runtime slice.

## Build

```
just mobile                        # verifies iOS and builds the Android APK
just mobile-android-device         # also runs the attached API 36 device gate
```

iOS uses Xcode (`iosApp/iosApp.xcworkspace`). The KMP framework
is built via Gradle and consumed by Xcode through the project
configuration.

## What NOT to Do

- ❌ Don't put UI code in `shared/`. UI stays in
  `androidApp/` / `iosApp/`.
- ❌ Don't reach for Compose Multiplatform iOS. SwiftUI is the
  default for iOS in Garive.
- ❌ Don't hand-write types that mirror a `.proto` field. Use
  the generated bindings.
- ❌ Don't claim cross-language parity without an executable harness.

## Testing

This tier follows the test pyramid in `.agents/testing.md`.
For `mobile/`:

| Layer | Where | What |
|-------|-------|------|
| Static | per module | `ktlint`, `detekt` (KMP shared + Android); ESLint / `tsc --noEmit` not applicable here |
| Unit | `mobile/shared/src/commonTest/`, `mobile/androidApp/src/test/`, `mobile/iosApp/` XCTests | TDD-first |
| Property | `mobile/shared/src/commonTest/` | `kotest-property` |
| Integration | `mobile/shared/src/test-integration/kotlin/` | shared-module flows |
| Contract | `mobile/shared/src/test/...` | round-trip every consumed `.proto` message |
| E2E | `mobile/androidApp/` (Espresso) + `mobile/iosApp/` (XCUITest) | platform-native UI tests; CI on a real device farm |

The shared KMP module is where cross-language conformance runs
(Kotlin side). The platform UI tests live with the platform,
not with shared.
