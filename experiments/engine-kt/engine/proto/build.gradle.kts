// engine/proto/build.gradle.kts — generated Kotlin bindings
// from `spec/proto/`. The protobuf Gradle plugin points at the
// schema source, generates Kotlin types into
// `build/generated/source/proto/main/kotlin/`, and exposes them
// as `com.garive.eng.kt.proto.*` for every other engine
// module to consume via `implementation(project(":engine:proto"))`.

import com.google.protobuf.gradle.id

plugins {
    kotlin("jvm")
    id("com.google.protobuf")
}

dependencies {
    "implementation"(platform("org.jetbrains.kotlin:kotlin-bom"))
    "implementation"("com.google.protobuf:protobuf-kotlin-lite:4.28.2")

    "testImplementation"(kotlin("test"))
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:4.28.2"
    }
    plugins {
        id("java") {
            artifact = "io.grpc:protoc-gen-grpc-java:1.65.1"
        }
    }
    generateProtoTasks {
        all().forEach { task ->
            task.builtins {
                id("java") {
                    option("lite")
                }
                id("kotlin") {
                    option("lite")
                }
            }
        }
    }
}

sourceSets {
    main {
        proto {
            srcDir("../../../../spec/proto")
        }
    }
}

kotlin {
    jvmToolchain(17)
}