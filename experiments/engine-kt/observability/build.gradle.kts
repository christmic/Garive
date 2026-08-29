dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
    implementation("io.github.erdtman:java-json-canonicalization:1.1")
}

tasks.withType<Test>().configureEach {
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/observability-v1.json"))
}
