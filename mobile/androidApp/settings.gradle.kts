pluginManagement {
    plugins {
        kotlin("multiplatform") version "2.4.10"
        id("com.squareup.wire") version "6.4.7"
        id("com.android.kotlin.multiplatform.library") version "9.1.1"
    }
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement { repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS); repositories { google(); mavenCentral() } }
rootProject.name = "garive-android"
include(":app")
include(":shared")
project(":shared").projectDir = file("../shared")
