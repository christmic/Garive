// proto/build.gradle.kts — generated Kotlin bindings from
// `spec/proto/`. The protobuf Gradle plugin points at the
// schema source, generates Kotlin types into
// `build/generated/source/proto/main/kotlin/`, and exposes them
// under each schema-declared package for admitted consumers.

import com.google.protobuf.gradle.id

plugins {
    kotlin("jvm")
    id("com.google.protobuf")
}

dependencies {
    "implementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
    "api"("com.google.protobuf:protobuf-kotlin:4.36.0")

    "testImplementation"(kotlin("test"))
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:4.36.0"
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
