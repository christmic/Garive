# Garive build orchestration
# https://github.com/casey/just
#
# Thin top-level orchestrator. Each recipe wires the corresponding
# language's build tool; individual crates / modules carry the
# implementation. Install `just` via `brew install just`.
#
# Toolchains (each recipe assumes the relevant ones on PATH):
#   rustup (rust 1.75+), cargo, protoc, pnpm, gradle, go.

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
# Pending — wire to `engine/proto/build.rs` once that crate lands.
codegen:
    @echo "codegen: not yet wired — engine/proto is not in the workspace"

# Build: codegen + Rust workspace
build: codegen
    cargo build --workspace

# Test: cargo test across workspace
test:
    cargo test --workspace

# Conformance: Rust + Kotlin read same fixtures, output identical
# Pending — wire to `cargo run -p bench --bin conformance` + the
# Kotlin `:conformance:run` task once both implementations land.
conformance:
    @echo "conformance: not yet wired"

# Desktop: Tauri build (TS frontend + Rust backend)
desktop:
    @echo "desktop: not yet wired — no Tauri manifest has landed"

# Mobile: Gradle Kotlin Multiplatform build
mobile:
    @echo "mobile: not yet wired — no KMP settings file has landed"

# Bench: SWE-bench verification runtime (orchestrator only).
# Implementation lands as the slice is scoped; until then this
# prints a stub.
bench:
    cargo test -p bench

# Clean: remove build artefacts across the workspace
clean:
    cargo clean
