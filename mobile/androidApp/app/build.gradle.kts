plugins { id("com.android.application"); id("org.jetbrains.kotlin.plugin.compose") }
fun firebaseValue(name: String): String {
    val value = System.getenv(name).orEmpty()
    require(value.matches(Regex("[A-Za-z0-9:._-]*"))) { "$name contains unsupported characters" }
    return "\"$value\""
}
android { namespace = "com.garive.android"; compileSdk = 36
    defaultConfig { applicationId = "com.garive.android"; minSdk = 26; targetSdk = 36; versionCode = 1; versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "FIREBASE_APP_ID", firebaseValue("GARIVE_FIREBASE_APP_ID"))
        buildConfigField("String", "FIREBASE_API_KEY", firebaseValue("GARIVE_FIREBASE_API_KEY"))
        buildConfigField("String", "FIREBASE_PROJECT_ID", firebaseValue("GARIVE_FIREBASE_PROJECT_ID"))
        buildConfigField("String", "FIREBASE_SENDER_ID", firebaseValue("GARIVE_FIREBASE_SENDER_ID")) }
    buildFeatures { compose = true; buildConfig = true } }
// These are the latest reviewed versions in this project that support the
// accepted compileSdk 36 gate. The next versions require compileSdk 37.
dependencies { implementation(platform("androidx.compose:compose-bom:2026.06.01")); implementation("androidx.activity:activity-compose:1.12.4");
    implementation("androidx.compose.material3:material3"); implementation("androidx.compose.material:material-icons-extended"); implementation(project(":shared"))
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.3")
    implementation(platform("com.google.firebase:firebase-bom:34.18.0"))
    implementation("com.google.firebase:firebase-messaging")
    androidTestImplementation(platform("androidx.compose:compose-bom:2026.06.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest") }
