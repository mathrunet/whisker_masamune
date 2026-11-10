// Android implementation of masamune-whisker-auth0. whisker-build includes it in the app's settings.gradle.kts as a subproject.
//
// Auth0.Android's manifest declares RedirectActivity with `${auth0Domain}` / `${auth0Scheme}`
// placeholders. The app module gets the real values from the crate's CNG plugin (MasamuneAuth0);
// the values below only satisfy this library's own test builds and do not reach the app.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("com.google.devtools.ksp") version "2.0.21-1.0.27"
}

android {
    namespace = "net.mathru.whisker.auth0"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        manifestPlaceholders["auth0Domain"] = "unused.auth0.com"
        manifestPlaceholders["auth0Scheme"] = "unused"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
        // Newer SDKs ship Kotlin 2.x metadata.
        freeCompilerArgs += "-Xskip-metadata-version-check"
    }

    sourceSets {
        getByName("main") {
            kotlin.srcDirs("android/src/main/kotlin")
        }
    }
}

ksp {
    arg("whisker.moduleName", "MasamuneWhiskerAuth0")
    arg("whisker.crateName", "masamune-whisker-auth0")
}

dependencies {
    implementation("rs.whisker:whisker-module-android:0.1.21")
    ksp("rs.whisker:ksp:0.1.21")
    implementation("com.auth0.android:auth0:3.21.3")
}
