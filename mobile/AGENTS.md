# mobile/AGENTS.md

> **Mobile Agent Apps.** Kotlin Multiplatform (KMP) for shared
> business logic, **Jetpack Compose** on Android, **SwiftUI**
> on iOS. KMP shares logic; UI stays native per platform.
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
| `shared/` | KMP `commonMain` + `androidMain` + `iosMain` (or native targets). Holds: agent client, tool registry, memory / knowledge stores, KMP-side protocol bindings. **No UI.** |
| `androidApp/` | Android app module. Jetpack Compose UI, Android-specific glue (push, permissions, notifications). Depends on `shared/`. |
| `iosApp/` | iOS app. SwiftUI views in `Sources/UI/`; Swift ↔ Kotlin bridge in `Sources/Bridge/` (uses [SKIE](https://skie.touchlab.co/) for type-safe coroutines / flows across the Kotlin boundary). Depends on `shared/`. |

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
- **Shared logic is where the value lives** — KMP already shares
  the agent runtime, tools, memory, knowledge client. Forcing
  UI into KMP too doubles the platform-specific risk for
  marginal reuse.

## Cross-platform Boundaries

The `shared/` module is the **only** place that knows the
business logic. UI tiers (`androidApp/`, `iosApp/`) are thin:

- **Compose** calls `shared/` through plain Kotlin interfaces.
- **SwiftUI** calls `shared/` through SKIE-exposed `Flow` /
  `suspend` functions. Never hand-roll an interop layer.

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

`just conformance` must pass before any commit that touches
`shared/`.

## Build

```
cd mobile
gradle :shared:build               # KMP shared module
gradle :androidApp:assembleDebug   # Android debug build
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
- ❌ Don't skip `just conformance` before committing to
  `shared/`.