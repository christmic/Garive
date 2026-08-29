dependencies {
    implementation(project(":core"))
    implementation(project(":ledger"))
    implementation(project(":persistence-postgres"))
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    testImplementation("io.zonky.test:embedded-postgres:2.2.2")
    testImplementation("io.zonky.test.postgres:embedded-postgres-binaries-darwin-arm64v8:18.4.0")
}
