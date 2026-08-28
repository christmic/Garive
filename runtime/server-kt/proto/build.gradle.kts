// proto/build.gradle.kts — generated Kotlin bindings from
// `spec/proto/`. The protobuf Gradle plugin points at the
// schema source, generates Kotlin types into
// `build/generated/source/proto/main/kotlin/`, and exposes them
// as `com.garive.eng.kt.proto.*` for every other module to
// consume via `implementation(project(":proto"))`.

import com.google.protobuf.gradle.id

plugins {
    kotlin("jvm")
    id("com.google.protobuf")
}

dependencies {
    "implementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
    "implementation"("com.google.protobuf:protobuf-kotlin:4.28.2")

    "testImplementation"(kotlin("test"))
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:4.28.2"
    }
    generateProtoTasks {
        all().forEach { task ->
            task.builtins {
                id("kotlin")
            }
        }
    }
}

sourceSets {
    main {
        proto {
            srcDir(rootProject.file("../../spec/proto"))
        }
    }
}

kotlin {
    jvmToolchain(21)
}
