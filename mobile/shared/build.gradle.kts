import org.jetbrains.kotlin.gradle.plugin.mpp.apple.XCFramework

plugins {
    kotlin("multiplatform")
    id("com.squareup.wire")
    id("com.android.kotlin.multiplatform.library")
}

group = "com.garive"
version = "0.1.0-SNAPSHOT"

kotlin {
    explicitApi()
    jvm()
    android {
        namespace = "com.garive.mobile.shared"
        compileSdk = 36
        minSdk = 26
    }
    val xcframework = XCFramework("GariveShared")
    listOf(iosArm64(), iosSimulatorArm64(), macosArm64()).forEach { target ->
        target.binaries.framework {
            baseName = "GariveShared"
            isStatic = true
            xcframework.add(this)
        }
    }
    sourceSets {
        commonMain.dependencies {
            api("com.squareup.wire:wire-runtime:6.4.7")
            implementation("io.ktor:ktor-client-core:3.5.2")
            implementation("io.ktor:ktor-client-cio:3.5.2")
            implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
            implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.10.0")
        }
        commonTest.dependencies { implementation(kotlin("test")) }
        jvmTest.dependencies { implementation("io.ktor:ktor-client-mock:3.5.2") }
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
