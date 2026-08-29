dependencies {
    implementation(project(":ledger"))
    implementation("org.postgresql:postgresql:42.7.13")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    testImplementation("io.zonky.test:embedded-postgres:2.2.2")
    testImplementation("io.zonky.test.postgres:embedded-postgres-binaries-darwin-arm64v8:18.4.0")
}
