# Garive verification orchestration. Individual modules own implementation.

set shell := ["bash", "-uc"]

default:
    @just --list

setup:
    bash scripts/setup-claude-symlinks.sh

# Cargo and Gradle generate bindings from spec/proto; generated files stay in
# build output and are never parallel handwritten DTOs.
codegen:
    cargo check -p garive-proto
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :proto:generateProto

architecture:
    cargo metadata --locked --format-version 1 | jq -e '[.packages[] | select(.manifest_path | contains("/engine/")) | .dependencies[] | select(.path != null and (.path | contains("/engine/") | not))] | length == 0'

test-layout:
    @if rg -n '#\[cfg\(test\)\]|#\[(tokio::)?test\]' --glob '**/src/**/*.rs' .; then echo 'Rust tests must live under tests/' >&2; exit 1; fi

conformance: architecture
    cargo test -p garive-core -p garive-llm
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :core:test :llm:test

providers:
    cargo test -p garive-llm-openai -p garive-llm-anthropic
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :provider-openai:test :provider-anthropic:test

kotlin-experiment:
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain build

web:
    cd web && pnpm install --frozen-lockfile && pnpm test && pnpm build

desktop:
    cd desktop/frontend && pnpm install --frozen-lockfile && pnpm test && pnpm build
    cargo test -p garive-desktop
    cargo check -p garive-desktop

mobile:
    cd mobile/shared && java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain jvmTest assembleGariveSharedDebugXCFramework
    cd mobile/iosApp && swift test
    cd mobile/androidApp && java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain tasks --all

apps: web desktop mobile
    cargo test -p garive-cli -p garive-tui

rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

build: codegen architecture
    cargo build --workspace
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain build

verify: test-layout conformance providers kotlin-experiment apps rust

bench:
    cargo test -p bench

clean:
    cargo clean
