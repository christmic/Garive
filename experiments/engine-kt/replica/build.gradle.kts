// replica/build.gradle.kts — Kotlin mirror of the Rust
// `runtime/replica` service container.

plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
    application
}

dependencies {
    "implementation"(project(":core"))
    "implementation"(project(":tools"))
    "implementation"(project(":memory"))
    "implementation"(project(":proto"))

    "implementation"("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    "implementation"("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.1")

    "testImplementation"(kotlin("test"))
}

application {
    mainClass.set("com.garive.eng.kt.replica.MainKt")
}

kotlin {
    jvmToolchain(17)
}