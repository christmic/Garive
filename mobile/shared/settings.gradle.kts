pluginManagement {
    plugins {
        kotlin("multiplatform") version "2.4.10"
        id("com.squareup.wire") version "6.4.7"
        id("com.android.kotlin.multiplatform.library") version "9.1.1"
    }
    repositories { google(); gradlePluginPortal(); mavenCentral() }
}
dependencyResolutionManagement { repositories { google(); mavenCentral() } }
rootProject.name = "garive-mobile-shared"
