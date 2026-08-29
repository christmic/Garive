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

config-boundaries:
    @if rg -n 'std::env|std::fs|System\.getenv|java\.io|runtime/replica' engine/config/src experiments/engine-kt/config/src/main; then echo 'D0 values and resolution must not load Runtime configuration' >&2; exit 1; fi

test-layout:
    @if rg -n '#\[cfg\(test\)\]|#\[(tokio::)?test\]' --glob '**/src/**/*.rs' .; then echo 'Rust tests must live under tests/' >&2; exit 1; fi

conformance: architecture config-boundaries
    cargo test -p garive-config -p garive-core -p garive-llm -p garive-tools
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :config:test :core:test :llm:test :tools:test

adapter-boundaries:
    @if rg -n 'std::env|System\.getenv|OPENAI_API_KEY|ANTHROPIC_API_KEY' adapters/openai-responses adapters/anthropic-messages experiments/engine-kt/adapter-openai-responses experiments/engine-kt/adapter-anthropic-messages --glob '!**/build/**'; then echo 'Protocol adapters must not read process configuration' >&2; exit 1; fi
    @if rg -n 'garive-(core|llm|runtime|ledger)|project\(\":(core|llm|runtime|ledger)' adapters/openai-responses adapters/anthropic-messages experiments/engine-kt/adapter-openai-responses experiments/engine-kt/adapter-anthropic-messages --glob '!**/build/**'; then echo 'Protocol adapters must not depend on Garive semantic layers' >&2; exit 1; fi
    @if rg -n '"(application/json|text/event-stream|content-type)"' adapters/openai-responses/src adapters/anthropic-messages/src experiments/engine-kt/adapter-openai-responses/src/main experiments/engine-kt/adapter-anthropic-messages/src/main --glob '!**/wire.rs' --glob '!**/Wire.kt'; then echo 'Repeated protocol HTTP literals belong in wire.rs or Wire.kt' >&2; exit 1; fi

protocol-adapters: adapter-boundaries
    cargo test -p garive-adapter-openai-responses -p garive-adapter-anthropic-messages
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :adapter-openai-responses:test :adapter-anthropic-messages:test

# Compatibility alias for existing local workflows; this target does not build Providers.
providers: protocol-adapters

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

verify: test-layout conformance protocol-adapters kotlin-experiment apps rust

bench:
    cargo test -p bench

clean:
    cargo clean
