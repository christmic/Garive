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
xcodebuild test -project GariveIOS.xcodeproj -scheme GariveIOS \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=<simulator-udid>' \
  CODE_SIGNING_ALLOWED=NO -parallel-testing-enabled NO \
  -only-testing:GariveIOSUITests
xcodebuild -project GariveIOS.xcodeproj -target GariveIOS \
  -configuration Debug -sdk iphoneos ARCHS=arm64 \
  CODE_SIGNING_ALLOWED=NO clean build
```

The nine UI tests require `go run ./cmd/garive-mobile-demo-host` from
`runtime/gateway/` for the connected journeys. They exercise secure
pairing fields, the Remote drawer, the inactive-workspace privacy replacement,
Sessions search/status filtering, new-task starters and enabled
server submit control in a fully expanded sheet plus an actual loopback create/start, cancellation and
second-Turn append in the opened Conversation, collapsed Activity, approve/decline and cancellation
confirmation, committed real-Host `Approve once` and `Decline` results, safe
diagnostics, notification entry, the native system share sheet with its Copy
activity, and confirmed unpair. The new-task goal editor has a stable
accessibility label, and UI coverage requires it to be visible and hittable on
first presentation.
The Settings journey additionally selects Light, Dark, and System and verifies
the native segmented-control state after every change. It terminates and
relaunches the shipping app after both explicit choices to prove AppStorage
restoration rather than only an in-memory selection.
Swift contract tests also require pairing links to pass the same shared remote
HTTPS-origin validator before any service suggestion is presented.

The Xcode target produces `Garive.app`, registers expiring `garive://pair`
handoffs, and links the static XCFramework. Distribution still requires the
operator's Apple team, signing, and physical-device verification.

## Private wake hints

The app target enables remote notifications and uses development APNs
entitlements in Debug and production entitlements in Release. A signed build
must use a provisioning profile that authorizes the matching environment.
Gateway owns `GARIVE_APNS_TEAM_ID`, `GARIVE_APNS_KEY_ID`,
`GARIVE_APNS_TOPIC`, and `GARIVE_APNS_KEY_FILE`; the APNs provider private key
must never enter the app bundle.

After pairing, the app registers its APNs device token against that grant. It
accepts only the exact content-free wake envelope, resolves the opaque route
token through authenticated Gateway transport, refreshes Runtime truth, and
only then opens the verified Session or Settings destination.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-30
- Status: installable remote-work app; physical remote release evidence pending.
