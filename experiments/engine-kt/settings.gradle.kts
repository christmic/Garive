// settings.gradle.kts — Engine-KT root.
//
// Kotlin mirror of the Rust engine + runtime, organised as a
// standard Gradle multi-module project. Every sub-directory is
// a top-level Gradle module; there is no intermediate `engine/`
// or `runtime/` grouping (those names describe Rust crates, not
// Gradle modules).

rootProject.name = "engine-kt"

// Active modules — `proto` is the only module with real build
// configuration today. The other sub-directories
// at this level (`core/`, `llm/`, `tools/`, `memory/`,
// `knowledge/`, `skill/`, `multiagent/`, `scheduler/`,
// `creativity/`, `eval/`, `observability/`, `config/`,
// `ledger/`) are empty placeholders. Add a module to this file
// when its slice starts landing code.
include(":core")          // C0 portable execution control
include(":llm")           // C1 provider-neutral model facts
include(":proto")         // generated Kotlin bindings from spec/proto/
