// build.gradle.kts — Engine-KT root build.
//
// Plugin versions and project-wide defaults. Each module's own
// `build.gradle.kts` applies what it specifically needs; this
// file only pins plugin versions, group / version, and
// repositories for every subproject.

plugins {
    kotlin("jvm") version "2.0.21" apply false
    kotlin("plugin.serialization") version "2.0.21" apply false
    id("com.google.protobuf") version "0.9.4" apply false
}

allprojects {
    group = "com.garive.eng.kt"
    version = "0.1.0-SNAPSHOT"

    repositories {
        mavenCentral()
    }
}

// Apply Kotlin JVM + shared dependencies to every project
// module via a generated-by-include convention. Each module
// still owns its own `build.gradle.kts`; the per-module
// `subprojects { ... }` style is replaced here with a simple
// shared default applied lazily.
gradle.beforeProject {
    project.apply(plugin = "org.jetbrains.kotlin.jvm")

    project.extensions.configure<org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension> {
        jvmToolchain(17)
    }

    project.dependencies {
        "implementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
        "implementation"("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")

        "testImplementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
        "testImplementation"(kotlin("test"))
        "testImplementation"("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
    }
}