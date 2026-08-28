// settings.gradle.kts — Engine-KT root.
//
// Kotlin mirror of the Rust engine + runtime, organised as a
// Gradle multi-module project. Each sub-directory is its own
// Gradle subproject; this file is the source of truth for
// which modules exist.

rootProject.name = "engine-kt"

// Core Agent (mirrors `engine/*` from the Rust tree)
include(":engine:core")
include(":engine:ledger")
include(":engine:llm")
include(":engine:tools")
include(":engine:memory")
include(":engine:knowledge")
include(":engine:skill")
include(":engine:multiagent")
include(":engine:scheduler")
include(":engine:creativity")
include(":engine:eval")
include(":engine:observability")
include(":engine:config")
include(":engine:proto")      // generated Kotlin bindings from spec/proto/

// Runtime tier (mirrors `runtime/replica` from the Rust tree)
include(":runtime:replica")