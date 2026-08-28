pluginManagement {
    plugins {
        kotlin("multiplatform") version "2.4.10"
        id("com.squareup.wire") version "6.4.7"
    }
    repositories { gradlePluginPortal(); mavenCentral() }
}
dependencyResolutionManagement { repositories { mavenCentral() } }
rootProject.name = "garive-mobile-shared"
