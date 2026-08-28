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
│   ├── androidMain/kotlin/  Android-specific glue (push, FCM, KeyStore)
│   ├── iosMain/kotlin/      iOS-specific glue (NSUserDefaults, Keychain)
│   └── commonTest/kotlin/   shared unit tests (referenced from spec/fixtures)
├── build.gradle.kts         KMP module config; protobuf plugin wired here
└── README.md
```

## What Goes Here

| Allowed | Examples |
|---------|----------|
| Agent loop client, tool registry | `AgentLoop`, `ToolRegistry`, `ToolResult` |
| Domain types | `Agent`, `Session`, `MemoryEntry`, `Knowledge` |
| Repositories | `SessionRepository`, `MemoryRepository` |
| Generated proto bindings (Kotlin) | from `spec/proto/` via Gradle protobuf plugin |
| Platform interfaces | `Clock`, `SecureStorage`, `PushNotifications` |

| Forbidden | Why |
|-----------|-----|
| Compose / SwiftUI / any UI | UI lives in `androidApp/` / `iosApp/` |
| Android SDK imports outside `androidMain/` | breaks KMP |
| iOS / Foundation imports outside `iosMain/` | breaks KMP |
| Hand-written types mirroring `.proto` fields | use generated bindings |
| `println` / `System.out` for logging | use the project's logging facade |

## Build

Not wired. Add a Gradle build only when the mobile client slice is selected.

## Verify

No mobile conformance gate exists yet. Add consumer-specific wire or semantic
checks with the first shared module that consumes a shipped contract.

## Meta

- Owner: `@christmic`
- Last reviewed: 2026-08-29
- Status: stub — slice not yet landed; content is scaffolding.
