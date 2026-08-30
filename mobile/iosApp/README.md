# mobile/iosApp/

> **iOS app.** SwiftUI UI, native Apple frameworks. Thin tier —
> calls into the generated `mobile/shared/` XCFramework.

## Implemented slice

- **UI**: SwiftUI (iOS 17+)
- **Async**: Swift Concurrency over Kotlin/Native completion handlers
- **Min target**: iOS 17
- **Build**: installable Xcode app plus SwiftPM contract tests

## Module Layout

```
iosApp/
├── GariveIOS.xcodeproj
├── Config/Info.plist
├── Sources/GariveIOS/
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

Build the shared framework, contract tests, then the unsigned device app gate:

```text
cd ../shared
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain assembleGariveSharedDebugXCFramework
cd ../iosApp
swift test
xcodebuild -project GariveIOS.xcodeproj -target GariveIOS \
  -configuration Debug -sdk iphoneos CODE_SIGNING_ALLOWED=NO clean build
```

The Xcode target produces `Garive.app`, registers expiring `garive://pair`
handoffs, and links the static XCFramework. Distribution still requires the
operator's Apple team, signing, and physical-device verification.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: installable remote-work app; physical remote release evidence pending.
