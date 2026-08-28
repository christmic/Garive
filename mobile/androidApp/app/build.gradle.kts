plugins { id("com.android.application"); kotlin("android") }
android { namespace = "com.garive.android"; compileSdk = 36
    defaultConfig { applicationId = "com.garive.android"; minSdk = 26; targetSdk = 36; versionCode = 1; versionName = "0.1.0" }
    buildFeatures { compose = true } }
dependencies { implementation(platform("androidx.compose:compose-bom:2026.06.01")); implementation("androidx.activity:activity-compose:1.12.4");
    implementation("androidx.compose.material3:material3") }
