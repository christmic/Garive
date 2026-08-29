dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
    implementation("io.github.erdtman:java-json-canonicalization:1.1")
}

tasks.withType<Test>().configureEach {
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/memory-capability-v1.json"))
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/memory-hypothesis-lifecycle-v1.json"))
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/memory-maintenance-v1.json"))
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/memory-promotion-erasure-v1.json"))
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/memory-recall-quality-v1.json"))
}
