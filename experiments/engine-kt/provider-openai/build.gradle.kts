dependencies {
    implementation(project(":llm"))
    implementation(project(":adapter-openai-responses"))
    implementation(project(":provider-compatible"))
    implementation(project(":provider-profile"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
}
