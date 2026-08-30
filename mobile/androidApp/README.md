# mobile/androidApp/

> **Android app.** Jetpack Compose UI, Material 3, AndroidX.
> Thin tier — calls into `mobile/shared/` for business logic.

## Implemented slice

- **UI**: Jetpack Compose + Material 3
- **Async**: Kotlin Coroutines
- **Min SDK**: 26 (Android 8.0); Target SDK: latest stable
- **Build**: Gradle (Kotlin DSL)

## Module Layout

```
androidApp/
├── app/src/main/java/com/garive/android/MainActivity.kt
├── app/src/main/res/                         theme and network policy
├── app/build.gradle.kts
├── build.gradle.kts
└── README.md
```

## Depends On

- `mobile/shared/` — the only business-logic dep.

## Build

From this directory, with Android SDK 36 installed:

```text
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain app:lintDebug app:assembleDebug \
  app:lintRelease app:assembleRelease
```

The checked-in Gradle memory limit is also required by the optimized Release
pipeline. With an API 36 device or emulator attached, run the native UI gate:

```text
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain app:connectedDebugAndroidTest
```

The gate includes the connected product shell itself: Work boot, drawer
navigation, Sessions, the native new-task sheet, goal starters, and enabled
server submission are exercised together. The journey commits fake-Host
create/start responses, reloads the durable timeline, cancels the running Turn,
and appends a second Turn in the opened Conversation in addition to focused
component and secure-storage tests.
The full-shell journey also selects Light and Dark through the same Settings
segmented control used by the shipping activity, then confirms unpair and
proves the shell returns to secure pairing.
Run `just mobile-android-live-ui` from the repository root for the opt-in
network journey. It starts the real repository Debug Host, establishes
`adb reverse`, then proves create/start, cancel and second-Turn append through
the installed Activity rather than a UI test double. It also opens independent
seeded suspensions, commits both `Approve once` and `Decline`, and verifies both
Host completions.
An active-app `garive://pair` journey verifies singleTop delivery, exact query
shape, shared HTTPS-origin canonicalization, visible service confirmation, and
an enabled explicit connection action.

The app includes `../shared` as a Gradle project, accepts an explicit loopback
Host URL and renders the terminal returned by `LiveHostClient`; it does not
duplicate Host reduction in the UI tier.

## Private wake hints

Provide the public Firebase Android identifiers at build time when FCM wake
delivery is required:

```text
GARIVE_FIREBASE_APP_ID=...
GARIVE_FIREBASE_API_KEY=...
GARIVE_FIREBASE_PROJECT_ID=...
GARIVE_FIREBASE_SENDER_ID=...
```

These values identify the Firebase app; they are not server credentials. The
FCM service-account key stays only on Gateway. Without all four values, the
remote-work app remains usable while push registration is disabled. A
configured build registers its current Firebase Installation ID against the
paired Gateway grant, accepts only the exact content-free wake envelope, and
resolves its opaque route token before opening a verified destination.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: installable remote-work app with private FCM return path; physical
  provider delivery evidence pending.
