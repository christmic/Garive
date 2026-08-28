plugins {
    kotlin("multiplatform") version "2.3.21"
    id("com.squareup.wire") version "6.4.7"
}

group = "com.garive"
version = "0.1.0-SNAPSHOT"

kotlin {
    jvm()
    iosArm64()
    iosSimulatorArm64()
    sourceSets {
        commonMain.dependencies { api("com.squareup.wire:wire-runtime:6.4.7") }
        commonTest.dependencies { implementation(kotlin("test")) }
        jvmTest.dependencies { implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.10.0") }
    }
}

wire {
    sourcePath {
        srcDir("../../spec/proto")
        include("garive/host/v1/host.proto")
    }
    kotlin {}
}

tasks.withType<Test>().configureEach {
    systemProperty("garive.repo.root", rootProject.layout.projectDirectory.dir("../..").asFile.absolutePath)
}
