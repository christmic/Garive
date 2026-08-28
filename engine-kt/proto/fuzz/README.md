# engine-kt/proto/fuzz/

> **Kotlin-side fuzz targets.** Mirror of
> `engine/proto/fuzz/`. Same wire messages, same invariants,
> same cadence — different tool (jazzer + libFuzzer for the
> JVM) and same goal: decoder never panics.

## Why

The wire types are generated, but **the Kotlin decoder is
re-built per `.proto` change**. A fuzz target on the Rust
side doesn't cover the Kotlin side. Each language has its
own fuzz targets, one per message.

## Mechanism

```
./gradlew :proto:jazzerRunner --tests="*Fuzz*"
```

Jazzer (libFuzzer binding for JVM) integrates with Gradle's
test runner. Targets live in `engine-kt/proto/src/jvmTest/`
(or equivalent) and are wired up when the slice lands.

## Required Targets

| Target | Asserts |
|--------|---------|
| `FuzzAgentIdentityDecode` | random bytes → Kotlin decoder → never panics |
| `FuzzPingRequestDecode` | same |
| `FuzzPingResponseDecode` | same |
| `Fuzz*Decode` | one per message in `spec/proto/*.proto` |

The contract: **every wire message has a fuzz target on
both sides.**

## Cadence

Mirrors the Rust side. Per-PR round-trip contract; nightly
fuzz; release deeper fuzz.

## Status

Placeholder. Targets land as messages land in
`spec/proto/*.proto` and the Kotlin Gradle plugin is
configured.