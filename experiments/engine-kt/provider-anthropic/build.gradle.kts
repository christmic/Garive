dependencies {
    implementation(project(":llm"))
    implementation(project(":adapter-anthropic-messages"))
    implementation(project(":provider-compatible"))
    implementation(project(":provider-profile"))
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
}
