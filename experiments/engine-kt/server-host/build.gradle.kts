plugins { application }

dependencies {
    implementation(project(":proto"))
    implementation(project(":core"))
    implementation(project(":ledger"))
    implementation(project(":persistence-postgres"))
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
}

application { mainClass.set("com.garive.eng.kt.host.MainKt") }
