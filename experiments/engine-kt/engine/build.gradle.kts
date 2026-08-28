// engine/build.gradle.kts — shared build config for every
// `:engine:*` module. Applies the Kotlin JVM plugin and the
// common dependency set; per-module `build.gradle.kts` adds
// what it specifically needs.
//
// Kotlin / JVM toolchain pinned to 17 (matches the Rust
// baseline we use elsewhere).

subprojects {
    apply(plugin = "org.jetbrains.kotlin.jvm")

    extensions.configure<org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension> {
        jvmToolchain(17)
    }

    dependencies {
        // engine modules depend on the generated proto bindings
        // and may pull additional deps per-module.
        "implementation"(project(":engine:proto"))
        "implementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
        "implementation"("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")

        "testImplementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
        "testImplementation"(kotlin("test"))
        "testImplementation"("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
    }
}