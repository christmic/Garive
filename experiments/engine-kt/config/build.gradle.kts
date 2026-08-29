dependencies {
    implementation(project(":tools"))
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    implementation("io.github.erdtman:java-json-canonicalization:1.1")
}

tasks.withType<Test>().configureEach {
    inputs.file(rootProject.projectDir.resolve("../../spec/fixtures/agent/agent-definition-snapshot.json"))
}
