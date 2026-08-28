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

`cd mobile && gradle :shared:build`

## Verify

`just conformance` (run from repo root) is the cross-language
sync gate for anything that touches the wire types consumed
from this module.