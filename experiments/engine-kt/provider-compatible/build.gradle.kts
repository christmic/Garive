dependencies {
    implementation(project(":llm"))
    implementation(project(":adapter-openai-responses"))
    implementation(project(":adapter-anthropic-messages"))
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")
}
