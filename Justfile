# Garive build orchestration
# https://github.com/casey/just
#
# Thin top-level orchestrator. Each recipe wires the corresponding
# language's build tool; individual crates / modules carry the
# implementation. Install `just` via `brew install just`.
#
# Toolchains (each recipe assumes the relevant ones on PATH):
#   rustup (rust 1.75+), cargo, protoc, pnpm, gradle, go, jq.

set shell := ["bash", "-uc"]

# Default — list recipes
default:
    @just --list

# Setup: wire `.claude/{rules,skills,agents}` as symlinks to the
# canonical `.agents/` tree so Claude Code auto-loads project
# rules / skills. Idempotent.
setup:
    bash scripts/setup-claude-symlinks.sh

# Codegen: regenerate protobuf bindings from spec/proto/.
# Pending — wire Rust generation to `engine/proto`; Kotlin generation is part
# of the independent Gradle build.
codegen:
    @echo "codegen: Rust engine/proto bindings are not wired yet"

# Build: codegen + Rust workspace
build: codegen architecture
    cargo build --workspace

# Test: cargo test across workspace
test:
    cargo test --workspace

# C0-C3 semantic/capability conformance over the same fixtures.
conformance: architecture
    cargo test -p garive-core -p garive-llm
    cd runtime/server-kt && ./gradlew --no-daemon --console=plain :agent-core:test :llm-contract:test

# Architecture: an Engine crate may depend on other Engine crates or external
# libraries, never on a local Runtime/App package.
architecture:
    cargo metadata --locked --format-version 1 | jq -e '[.packages[] | select(.manifest_path | contains("/engine/")) | .dependencies[] | select(.path != null and (.path | contains("/engine/") | not))] | length == 0'

# Desktop: Tauri build (TS frontend + Rust backend)
desktop:
    cargo check -p garive-desktop

# Mobile: Gradle Kotlin Multiplatform build
mobile:
    cd mobile/iosApp && swift test
    @echo "Android SDK gate: cd mobile/androidApp && gradle :app:assembleDebug"

# Bench: SWE-bench verification runtime (orchestrator only).
# Implementation lands as the slice is scoped; until then this
# prints a stub.
bench:
    cargo test -p bench

# Clean: remove build artefacts across the workspace
clean:
    cargo clean
