// build.gradle.kts — Engine-KT root build.
//
// Plugin versions and project-wide defaults. Each module's
// `build.gradle.kts` applies its own plugins and dependencies.

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