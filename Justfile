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
    cd runtime/server-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :proto:generateProto

architecture:
    cargo metadata --locked --format-version 1 | jq -e '[.packages[] | select(.manifest_path | contains("/engine/")) | .dependencies[] | select(.path != null and (.path | contains("/engine/") | not))] | length == 0'

conformance: architecture
    cargo test -p garive-core -p garive-llm
    cd runtime/server-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :agent-core:test :llm-contract:test

providers:
    cargo test -p garive-llm-openai -p garive-llm-anthropic
    cd runtime/server-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :provider-openai:test :provider-anthropic:test

server:
    cd runtime/server-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain build

web:
    cd web && pnpm install --frozen-lockfile && pnpm test && pnpm build

desktop:
    cd desktop/frontend && pnpm install --frozen-lockfile && pnpm test && pnpm build
    cargo test -p garive-desktop
    cargo check -p garive-desktop

mobile:
    cd mobile/shared && java -classpath ../../runtime/server-kt/gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain jvmTest assembleGariveSharedDebugXCFramework
    cd mobile/iosApp && swift test
    cd mobile/androidApp && java -classpath ../../runtime/server-kt/gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain tasks --all

apps: web desktop mobile
    cargo test -p garive-cli -p garive-tui

rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

build: codegen architecture
    cargo build --workspace
    cd runtime/server-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain build

verify: conformance providers server apps rust

bench:
    cargo test -p bench

clean:
    cargo clean
