// masamune-whisker-share の Android 実装。whisker-build がアプリの settings.gradle.kts にサブプロジェクトとして組み込む。

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("com.google.devtools.ksp") version "2.0.21-1.0.27"
}

android {
    namespace = "net.mathru.whisker.share"
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
            manifest.srcFile("android/src/main/AndroidManifest.xml")
            res.srcDirs("android/src/main/res")
        }
    }
}

ksp {
    arg("whisker.moduleName", "MasamuneWhiskerShare")
    arg("whisker.crateName", "masamune-whisker-share")
}

dependencies {
    implementation("rs.whisker:whisker-module-android:0.1.21")
    ksp("rs.whisker:ksp:0.1.21")
    implementation("androidx.core:core-ktx:1.13.1")
}
