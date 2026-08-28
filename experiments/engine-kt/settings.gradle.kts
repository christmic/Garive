// settings.gradle.kts — Engine-KT root.
//
// Kotlin mirror of the Rust engine + runtime, organised as a
// standard Gradle multi-module project. Every sub-directory is
// a top-level Gradle module; there is no intermediate `engine/`
// or `runtime/` grouping (those names describe Rust crates, not
// Gradle modules).

rootProject.name = "engine-kt"

// Active modules — `proto` and `replica` are the only modules
// with real build configuration today. The other sub-directories
// at this level (`core/`, `llm/`, `tools/`, `memory/`,
// `knowledge/`, `skill/`, `multiagent/`, `scheduler/`,
// `creativity/`, `eval/`, `observability/`, `config/`,
// `ledger/`) are empty placeholders. Add a module to this file
// when its slice starts landing code.
include(":proto")         // generated Kotlin bindings from spec/proto/
include(":replica")       // mirror of Rust `runtime/replica`