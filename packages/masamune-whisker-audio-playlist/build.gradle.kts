// Android implementation of masamune-whisker-audio-playlist. whisker-build includes it in the app's settings.gradle.kts as a subproject.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("com.google.devtools.ksp") version "2.0.21-1.0.27"
}

android {
    namespace = "net.mathru.whisker.audio_playlist"
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
    arg("whisker.moduleName", "MasamuneWhiskerAudioPlaylist")
    arg("whisker.crateName", "masamune-whisker-audio-playlist")
}

dependencies {
    implementation("rs.whisker:whisker-module-android:0.1.21")
    ksp("rs.whisker:ksp:0.1.21")
    // AndroidX Media3 from Google Maven. 1.9.x needs compileSdk 35 in the app (1.10 and later
    // need 36); Gradle picks the highest version when whisker-audio / whisker-video bring 1.4.x.
    implementation("androidx.media3:media3-exoplayer:1.9.4")
    implementation("androidx.media3:media3-common:1.9.4")
}
