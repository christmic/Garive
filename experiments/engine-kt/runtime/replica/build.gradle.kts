// runtime/replica/build.gradle.kts — Kotlin mirror of the
// Rust replica service container.

plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
    application
}

dependencies {
    "implementation"(project(":engine:core"))
    "implementation"(project(":engine:tools"))
    "implementation"(project(":engine:memory"))
    "implementation"(project(":engine:proto"))

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