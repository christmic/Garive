// build.gradle.kts — experimental Kotlin Engine root build.
//
// Plugin versions and project-wide defaults. Each module's own
// `build.gradle.kts` applies what it specifically needs; this
// file only pins plugin versions, group / version, and
// repositories for every subproject.

plugins {
    kotlin("jvm") version "2.4.10" apply false
    kotlin("plugin.serialization") version "2.4.10" apply false
    id("com.google.protobuf") version "0.9.6" apply false
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
        jvmToolchain(21)
        if (project.name == "llm") explicitApi() else explicitApiWarning()
        compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }

    project.dependencies {
        "implementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
        "implementation"("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")

        "testImplementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
        "testImplementation"(kotlin("test"))
        "testImplementation"("org.jetbrains.kotlin:kotlin-test-junit5")
        "testImplementation"("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
        "testRuntimeOnly"("org.junit.platform:junit-platform-launcher")
    }

    project.tasks.withType<Test>().configureEach {
        useJUnitPlatform()
        systemProperty("garive.repo.root", rootProject.projectDir.resolve("../..").canonicalPath)
    }

    project.tasks.withType<JavaCompile>().configureEach {
        options.release.set(17)
    }
}
