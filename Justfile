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

skill-boundaries:
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|java\.io|java\.net|ModelPort|reqwest|tokio' engine/skill/src experiments/engine-kt/skill/src/main; then echo 'S0 must remain a pure instruction and activation contract' >&2; exit 1; fi

memory-boundaries:
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|java\.io|java\.net|ModelPort|reqwest|tokio|rusqlite|postgres' engine/memory/src experiments/engine-kt/memory/src/main; then echo 'M0 Engine must remain a pure memory value and reduction contract' >&2; exit 1; fi

knowledge-boundaries:
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|java\.io|java\.net|ModelPort|reqwest|tokio|rusqlite|postgres' engine/knowledge/src experiments/engine-kt/knowledge/src/main; then echo 'K0 Engine must remain a pure retrieval value and reduction contract' >&2; exit 1; fi
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|reqwest|OPENAI|ANTHROPIC|api[_-]?key' runtime/replica/src/core_bridge/knowledge_*.rs; then echo 'K0 Runtime ports must receive connector configuration explicitly' >&2; exit 1; fi

scheduler-boundaries:
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|java\.io|java\.net|ModelPort|reqwest|tokio|rusqlite|postgres' engine/scheduler/src experiments/engine-kt/scheduler/src/main; then echo 'Q0 Engine must remain a pure recurrence contract' >&2; exit 1; fi
    @if rg -n 'std::env|System\.getenv|OPENAI|ANTHROPIC|api[_-]?key' runtime/replica/src/scheduler_runtime; then echo 'Q0 Runtime must receive clock, authority, worker and dispatch configuration explicitly' >&2; exit 1; fi

multiagent-boundaries:
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|java\.io|java\.net|ModelPort|reqwest|tokio|rusqlite|postgres' engine/multiagent/src experiments/engine-kt/multiagent/src/main; then echo 'MA0 Engine must remain a pure delegation value and reduction contract' >&2; exit 1; fi
    @if rg -n 'std::env|System\.getenv|OPENAI|ANTHROPIC|api[_-]?key' runtime/replica/src/delegation_runtime.rs; then echo 'MA0 Runtime must receive authority, child, clock and budget inputs explicitly' >&2; exit 1; fi

observability-boundaries:
    @if rg -n 'std::env|std::fs|std::process|System\.getenv|java\.io|java\.net|reqwest|tokio|rusqlite|opentelemetry|tracing' engine/observability/src experiments/engine-kt/observability/src/main; then echo 'O0 Engine must remain a pure signal contract' >&2; exit 1; fi
    @if rg -n 'std::env|System\.getenv|OPENAI|ANTHROPIC|api[_-]?key' runtime/replica/src/observability_runtime.rs; then echo 'O0 Runtime configuration must enter explicitly' >&2; exit 1; fi

evaluation-boundaries:
    @if rg -n 'std::env|std::fs|std::process|std::net|System\.getenv|java\.io|java\.net|reqwest|tokio|rusqlite' engine/eval/src; then echo 'E0 Engine must remain a pure evidence contract' >&2; exit 1; fi

creativity-boundaries:
    @if rg -n -v '^(//|#!\[|[[:space:]]*$$)' engine/creativity/src; then echo 'Creativity behavior remains gated; its Engine crate must stay empty' >&2; exit 1; fi

test-layout:
    @if rg -n '#\[cfg\(test\)\]|#\[(tokio::)?test\]' --glob '**/src/**/*.rs' .; then echo 'Rust tests must live under tests/' >&2; exit 1; fi

conformance: architecture config-boundaries skill-boundaries memory-boundaries knowledge-boundaries scheduler-boundaries multiagent-boundaries observability-boundaries evaluation-boundaries creativity-boundaries
    cargo test -p garive-config -p garive-core -p garive-eval -p garive-knowledge -p garive-ledger -p garive-llm -p garive-memory -p garive-multiagent -p garive-observability -p garive-scheduler -p garive-skill -p garive-tools
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :config:test :core:test :knowledge:test :ledger:test :llm:test :memory:test :multiagent:test :observability:test :scheduler:test :skill:test :tools:test

adapter-boundaries:
    @if rg -n 'std::env|System\.getenv|OPENAI_API_KEY|ANTHROPIC_API_KEY' adapters/openai-responses adapters/anthropic-messages experiments/engine-kt/adapter-openai-responses experiments/engine-kt/adapter-anthropic-messages --glob '!**/build/**'; then echo 'Protocol adapters must not read process configuration' >&2; exit 1; fi
    @if rg -n 'garive-(core|llm|runtime|ledger)|project\(\":(core|llm|runtime|ledger)' adapters/openai-responses adapters/anthropic-messages experiments/engine-kt/adapter-openai-responses experiments/engine-kt/adapter-anthropic-messages --glob '!**/build/**'; then echo 'Protocol adapters must not depend on Garive semantic layers' >&2; exit 1; fi
    @if rg -n '"(application/json|text/event-stream|content-type)"' adapters/openai-responses/src adapters/anthropic-messages/src experiments/engine-kt/adapter-openai-responses/src/main experiments/engine-kt/adapter-anthropic-messages/src/main --glob '!**/wire.rs' --glob '!**/Wire.kt'; then echo 'Repeated protocol HTTP literals belong in wire.rs or Wire.kt' >&2; exit 1; fi

protocol-adapters: adapter-boundaries
    cargo test -p garive-adapter-openai-responses -p garive-adapter-anthropic-messages
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :adapter-openai-responses:test :adapter-anthropic-messages:test

provider-boundaries:
    @if rg -n 'std::env|System\.getenv|OPENAI_API_KEY|ANTHROPIC_API_KEY' providers experiments/engine-kt/provider-* --glob '!**/build/**'; then echo 'Providers must receive deployment configuration explicitly' >&2; exit 1; fi
    @if rg -n 'garive-runtime|runtime/replica|project\(":runtime' providers experiments/engine-kt/provider-* --glob '!**/build/**'; then echo 'Portable Providers must not own Runtime transport' >&2; exit 1; fi

providers: protocol-adapters provider-boundaries
    cargo test -p garive-provider-compatible -p garive-provider-profile -p garive-provider-openai -p garive-provider-anthropic
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :provider-compatible:test :provider-profile:test :provider-openai:test :provider-anthropic:test

runtime-boundaries:
    @if rg -n 'std::env|std::fs|System\.getenv|OPENAI_API_KEY|ANTHROPIC_API_KEY' runtime/replica/src/model_http_transport.rs runtime/replica/src/live_host; then echo 'Runtime transport and Host configuration must enter explicitly' >&2; exit 1; fi

local-runtime-boundaries:
    @if rg -n 'std::env|std::fs|System\.getenv|OPENAI|ANTHROPIC|api[_-]?key|credential|SecretValue' runtime/replica/src/local_composition.rs runtime/replica/src/local_worker.rs runtime/replica/src/local_recovery.rs; then echo 'R1 must receive storage, model ports, clocks and limits explicitly' >&2; exit 1; fi

runtime-host: runtime-boundaries
    cargo test -p garive-runtime --test model_http_transport --test live_host

local-runtime: runtime-boundaries local-runtime-boundaries
    cargo test -p garive-runtime --test local_composition --test local_worker --test local_live_flow --test process_kill_recovery

host-client-boundaries:
    @if rg -n 'std::env|OPENAI|ANTHROPIC|api[_-]?key|credential|SecretValue|garive-(runtime|core|ledger|llm)' clients/host-rs; then echo 'A1 Host clients must receive bounds and loopback address explicitly and cannot depend on semantic layers' >&2; exit 1; fi

host-client: host-client-boundaries
    cargo test -p garive-host-client

knowledge-runtime: knowledge-boundaries
    cargo test -p garive-runtime --test durable_core_execution --test knowledge_authority --test knowledge_recovery

scheduler-runtime: scheduler-boundaries
    cargo test -p garive-runtime --test schedule_lease --test scheduler_lifecycle --test scheduler_management --test scheduler_worker --test process_kill_recovery

multiagent-runtime: multiagent-boundaries
    cargo test -p garive-runtime --test delegation_continuation --test delegation_runtime --test process_kill_recovery

observability-runtime: observability-boundaries
    cargo test -p garive-runtime --test observability_runtime

kotlin-experiment:
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain build

web:
    cd web && pnpm install --frozen-lockfile && pnpm test && pnpm build

desktop-config-boundaries:
    @if rg -n 'std::env|System\.getenv|OPENAI_API_KEY|ANTHROPIC_API_KEY' desktop/backend/src; then echo 'Desktop configuration must not read process environment' >&2; exit 1; fi
    @if rg -n 'credential|profile_id|endpoint|model_id|database' desktop/frontend/src; then echo 'Desktop frontend must not own backend configuration' >&2; exit 1; fi

desktop: desktop-config-boundaries
    cd desktop/frontend && pnpm install --frozen-lockfile && pnpm test && pnpm build
    cargo test -p garive-desktop
    cargo check -p garive-desktop

mobile-shared:
    cd mobile/shared && java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain jvmTest assembleGariveSharedDebugXCFramework

mobile-ios: mobile-shared
    cd mobile/iosApp && swift test

mobile-android:
    cd mobile/androidApp && java -classpath ../../experiments/engine-kt/gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain :app:assembleDebug

mobile: mobile-ios mobile-android

apps: host-client web desktop mobile
    cargo test -p garive-cli -p garive-tui

rust:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

build: codegen architecture
    cargo build --workspace
    cd experiments/engine-kt && java -classpath gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain --no-daemon --console=plain build

verify: test-layout conformance protocol-adapters runtime-host local-runtime host-client knowledge-runtime scheduler-runtime multiagent-runtime observability-runtime kotlin-experiment apps rust

bench:
    cargo test -p bench

clean:
    cargo clean
