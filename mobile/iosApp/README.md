# mobile/iosApp/

> **iOS app.** SwiftUI UI, native Apple frameworks. Thin tier —
> calls into the generated `mobile/shared/` XCFramework.

## Implemented slice

- **UI**: SwiftUI (iOS 17+)
- **Async**: Swift Concurrency over Kotlin/Native completion handlers
- **Min target**: iOS 17
- **Build**: SwiftPM linked to the generated KMP XCFramework

## Module Layout

```
iosApp/
├── Sources/GariveIOS/main.swift
├── Tests/GariveIOSTests/LiveHostTests.swift
├── Package.swift
└── README.md
```

## Bridge

- Host v1 enters through Kotlin/Native generated completion handlers around the
  shared KMP suspend client. Add SKIE only when a later slice exports `Flow`;
  do not duplicate HTTP, SSE, or event reduction in Swift.

## Depends On

- `mobile/shared/` (via the KMP framework).

## Build

The executable contract gate builds the shared framework before SwiftPM:

```text
cd ../shared
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain assembleGariveSharedDebugXCFramework
cd ../iosApp
swift test
```

SwiftPM links the XCFramework when it exists. A conditional local fallback
keeps source-only editing possible, but it is not the acceptance path.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: live-H1 SwiftUI shell with verified KMP framework and Swift tests.
