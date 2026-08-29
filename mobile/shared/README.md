# mobile/shared/

> **KMP shared module.** Generated Host v1 wire types, bounded live H1 client,
> and durable-event reducer. No UI and no Agent Engine semantics.

This is the only place in `mobile/` that holds business code.
UI tiers (`androidApp/`, `iosApp/`) depend on this module but
this module depends on neither.

## Module Layout

```
shared/
├── src/
│   ├── commonMain/kotlin/   live H1 client and reducer
│   └── jvmTest/kotlin/      shared fixture, real-loopback and wire tests
├── build.gradle.kts         KMP + Square Wire generation
└── README.md
```

## What Goes Here

| Allowed | Examples |
|---------|----------|
| Live Host client | H1 commands, SSE follow and terminal reduction |
| Client view types | Session/Turn command results and reduced terminal view |
| Generated proto bindings (Kotlin) | from `spec/proto/` via Square Wire Gradle plugin |
| Explicit client inputs | loopback URL and response/event bounds |

| Forbidden | Why |
|-----------|-----|
| Compose / SwiftUI / any UI | UI lives in `androidApp/` / `iosApp/` |
| Android SDK imports outside `androidMain/` | breaks KMP |
| iOS / Foundation imports outside `iosMain/` | breaks KMP |
| Hand-written types mirroring `.proto` fields | use generated bindings |
| `println` / `System.out` for logging | use the project's logging facade |

## Build

From this directory, using Garive's pinned Gradle wrapper:

```text
java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain jvmTest
```

## Verify

`jvmTest` reads `spec/fixtures/host/live-host-client-v1.json`, constructs only
Wire-generated Host values, round-trips protobuf bytes, verifies all reducer
failures and exercises HTTP/SSE against a real loopback server. The XCFramework
gate separately compiles the same client for iOS and macOS.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: active Host API v1 client slice.
