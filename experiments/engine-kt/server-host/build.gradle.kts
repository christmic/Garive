plugins { application }

dependencies {
    implementation(project(":proto"))
    implementation(project(":core"))
    implementation(project(":ledger"))
    implementation(project(":persistence-postgres"))
}

application { mainClass.set("com.garive.eng.kt.host.MainKt") }
