plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.woxlium.zombie"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "com.woxlium.zombie"
        minSdk = 24 // Android 7.0
        targetSdk = 36 // runs on Android 17 (API 37) as well
        versionCode = 1
        versionName = "0.1.0-mvp"
        ndk {
            // 32-bit + 64-bit ARM, plus x86_64 for emulators / Chromebooks
            abiFilters += listOf("armeabi-v7a", "arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // replace with your own keystore before publishing
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))

    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

// Rust core -> libwoxlium.so for every ABI (see rust/build_android.sh)
val buildRust by tasks.registering(Exec::class) {
    val sdk = android.sdkDirectory.absolutePath
    val ndkDir = "$sdk/ndk/${android.ndkVersion}"
    workingDir = rootProject.file("rust")
    commandLine("bash", "build_android.sh", layout.buildDirectory.dir("rustJniLibs").get().asFile.absolutePath)
    environment("ANDROID_NDK_HOME", ndkDir)
    inputs.dir(rootProject.file("rust/woxlium_core/src"))
    inputs.file(rootProject.file("rust/woxlium_core/Cargo.toml"))
    inputs.file(rootProject.file("rust/build_android.sh"))
    outputs.dir(layout.buildDirectory.dir("rustJniLibs"))
}
tasks.named("preBuild") { dependsOn(buildRust) }
