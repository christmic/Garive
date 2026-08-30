dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
    implementation("io.github.erdtman:java-json-canonicalization:1.1")
}

tasks.withType<Test>().configureEach {
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/prepared-tool-call.json"))
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/deterministic-effect-batches-v1.json"))
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/sandbox-safety-v1.json"))
}
