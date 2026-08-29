# mobile/androidApp/

> **Android app.** Jetpack Compose UI, Material 3, AndroidX.
> Thin tier — calls into `mobile/shared/` for business logic.

## Stack

- **UI**: Jetpack Compose + Material 3
- **DI**: Hilt (or Kotlin-Inject; pick one and stay consistent)
- **Async**: Kotlin Coroutines + Flow (already idiomatic in KMP)
- **Min SDK**: 26 (Android 8.0); Target SDK: latest stable
- **Build**: Gradle (Kotlin DSL)

## Module Layout

```
androidApp/
├── src/main/kotlin/com/garive/mobile/   application code
│   ├── ui/        Compose screens, themes, components
│   ├── nav/       Navigation graph
│   ├── platform/  Android-specific glue (push, permissions)
│   └── MainActivity.kt
├── src/main/res/  resources (drawables, strings, themes)
├── build.gradle.kts
└── README.md
```

## Conventions

- All screens are `@Composable` functions; no XML layouts.
- ViewModels expose `StateFlow<UiState>`; screens `collectAsState`.
- Navigation via `androidx.navigation:navigation-compose`.
- Push notifications via FCM (Android) wired in `platform/`.
- Permissions: `accompanist-permissions` or the Activity Result
  API.

## Depends On

- `mobile/shared/` — the only business-logic dep.

## Build

From this directory, with Android SDK 36 installed:

```text
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain app:assembleDebug
```

The app includes `../shared` as a Gradle project, accepts an explicit loopback
Host URL and renders the terminal returned by `LiveHostClient`; it does not
duplicate Host reduction in the UI tier.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-27
- Status: active Compose shell; Gradle configuration verified, APK gate requires
  a local Android SDK.
