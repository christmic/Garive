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

# Codegen: regenerate protobuf bindings from spec/proto/.
# Pending — wire to `engine/proto/build.rs` once that crate lands.
codegen:
    @echo "codegen: not yet wired — engine/proto is not in the workspace"

# Build: codegen + Rust workspace
build: codegen
    @echo "build: not yet wired — workspace has no members yet"

# Test: cargo test across workspace
test:
    @echo "test: not yet wired — workspace has no members yet"

# Conformance: Rust + Kotlin read same fixtures, output identical
# Pending — wire to `cargo run -p bench --bin conformance` + the
# Kotlin `:conformance:run` task once both implementations land.
conformance:
    @echo "conformance: not yet wired"

# Desktop: Tauri build (TS frontend + Rust backend)
desktop:
    cd desktop && pnpm tauri build

# Mobile: Gradle Kotlin Multiplatform build
mobile:
    cd mobile && gradle build

# Bench
bench:
    @echo "bench: not yet wired — bench crate is not in the workspace"

# Clean: remove build artefacts across the workspace
clean:
    cargo clean