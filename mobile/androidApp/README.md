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
  org.gradle.wrapper.GradleWrapperMain app:assembleDebug
```

With an API 36 device or emulator attached, run the native UI gate:

```text
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain app:connectedDebugAndroidTest
```

The app includes `../shared` as a Gradle project, accepts an explicit loopback
Host URL and renders the terminal returned by `LiveHostClient`; it does not
duplicate Host reduction in the UI tier.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: live-H1 Compose shell, SDK 36 APK and API 36 instrumentation gate
  verified.
