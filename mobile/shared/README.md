# mobile/shared/

> **KMP shared module.** Pure business logic — agent client,
> tool registry, memory / knowledge stores, protocol bindings.
> No UI.

This is the only place in `mobile/` that holds business code.
UI tiers (`androidApp/`, `iosApp/`) depend on this module but
this module depends on neither.

## Module Layout

```
shared/
├── src/
│   ├── commonMain/kotlin/   platform-agnostic business code
│   ├── commonMain/kotlin/   generated-wire Host client and reducer
│   └── jvmTest/kotlin/      shared fixture and protobuf round-trip tests
├── build.gradle.kts         KMP + Square Wire generation
└── README.md
```

## What Goes Here

| Allowed | Examples |
|---------|----------|
| Agent loop client, tool registry | `AgentLoop`, `ToolRegistry`, `ToolResult` |
| Domain types | `Agent`, `Session`, `MemoryEntry`, `Knowledge` |
| Repositories | `SessionRepository`, `MemoryRepository` |
| Generated proto bindings (Kotlin) | from `spec/proto/` via Square Wire Gradle plugin |
| Platform interfaces | `Clock`, `SecureStorage`, `PushNotifications` |

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

`jvmTest` reads `spec/fixtures/host/fake-session.json`, constructs only
Wire-generated Host values, round-trips protobuf bytes, and verifies terminal,
identity and durable-position reduction.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: active Host API v1 client slice.
