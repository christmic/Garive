plugins { application }

dependencies {
    implementation(project(":proto"))
    implementation(project(":agent-core"))
    implementation(project(":ledger-contract"))
    implementation(project(":persistence-postgres"))
    implementation(project(":provider-openai"))
    implementation(project(":provider-anthropic"))
}

application { mainClass.set("com.garive.runtime.server.host.MainKt") }
