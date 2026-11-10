// masamune-whisker-firebase-remote-config の Android 実装。whisker-build がアプリの settings.gradle.kts にサブプロジェクトとして組み込む。

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("com.google.devtools.ksp") version "2.0.21-1.0.27"
}

android {
    namespace = "net.mathru.whisker.firebase_remote_config"
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
        // Firebase / Play Services の新しい SDK は Kotlin 2.3 のメタデータを含むため。
        freeCompilerArgs += "-Xskip-metadata-version-check"
    }

    sourceSets {
        getByName("main") {
            kotlin.srcDirs("android/src/main/kotlin")
        }
    }
}

ksp {
    arg("whisker.moduleName", "MasamuneWhiskerFirebaseRemoteConfig")
    arg("whisker.crateName", "masamune-whisker-firebase-remote-config")
}

dependencies {
    implementation("rs.whisker:whisker-module-android:0.1.21")
    ksp("rs.whisker:ksp:0.1.21")
    implementation(platform("com.google.firebase:firebase-bom:34.19.0"))
    implementation("com.google.firebase:firebase-config")
}
