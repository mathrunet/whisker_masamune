// Android implementation of masamune-whisker-barcode-scanner. whisker-build includes it in the app's settings.gradle.kts as a subproject.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("com.google.devtools.ksp") version "2.0.21-1.0.27"
}

android {
    namespace = "net.mathru.whisker.barcode_scanner"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
        // Newer Firebase / Play Services SDKs ship Kotlin 2.3 metadata.
        freeCompilerArgs += "-Xskip-metadata-version-check"
    }

    sourceSets {
        getByName("main") {
            kotlin.srcDirs("android/src/main/kotlin")
            manifest.srcFile("android/src/main/AndroidManifest.xml")
        }
    }
}

ksp {
    arg("whisker.moduleName", "MasamuneWhiskerBarcodeScanner")
    arg("whisker.crateName", "masamune-whisker-barcode-scanner")
}

dependencies {
    implementation("rs.whisker:whisker-module-android:0.1.21")
    ksp("rs.whisker:ksp:0.1.21")
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-ktx:1.9.3")
    // Camera preview and frames (Jetpack CameraX).
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    // QR decoding of the frames (ZXing core only; no Google Play services / ML Kit).
    implementation("com.google.zxing:core:3.5.3")
}
