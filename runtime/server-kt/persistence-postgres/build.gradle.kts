dependencies {
    implementation(project(":ledger-contract"))
    implementation("org.postgresql:postgresql:42.7.13")
    testImplementation("io.zonky.test:embedded-postgres:2.2.2")
    testImplementation("io.zonky.test.postgres:embedded-postgres-binaries-darwin-arm64v8:18.4.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
}
