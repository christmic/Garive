import org.jetbrains.kotlin.gradle.plugin.mpp.apple.XCFramework

plugins {
    kotlin("multiplatform")
    id("com.squareup.wire")
}

group = "com.garive"
version = "0.1.0-SNAPSHOT"

kotlin {
    explicitApi()
    jvm()
    val xcframework = XCFramework("GariveShared")
    listOf(iosArm64(), iosSimulatorArm64(), macosArm64()).forEach { target ->
        target.binaries.framework {
            baseName = "GariveShared"
            isStatic = true
            xcframework.add(this)
        }
    }
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
