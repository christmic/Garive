plugins { id("com.android.application"); id("org.jetbrains.kotlin.plugin.compose") }
android { namespace = "com.garive.android"; compileSdk = 36
    defaultConfig { applicationId = "com.garive.android"; minSdk = 26; targetSdk = 36; versionCode = 1; versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner" }
    buildFeatures { compose = true } }
// These are the latest reviewed versions in this project that support the
// accepted compileSdk 36 gate. The next versions require compileSdk 37.
dependencies { implementation(platform("androidx.compose:compose-bom:2026.06.01")); implementation("androidx.activity:activity-compose:1.12.4");
    implementation("androidx.compose.material3:material3"); implementation("androidx.compose.material:material-icons-extended"); implementation(project(":shared"))
    androidTestImplementation(platform("androidx.compose:compose-bom:2026.06.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest") }
