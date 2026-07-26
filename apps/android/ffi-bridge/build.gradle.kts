plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.featherkey.ffi"
    compileSdk = 35
    defaultConfig {
        minSdk = 26
        // The UniFFI bindgen writes generated Kotlin into src/main/kotlin/uniffi/…
        // and the cargo-ndk build drops per-ABI .so files into src/main/jniLibs/.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlin { jvmToolchain(17) }
}

dependencies {
    // JNA is the runtime the UniFFI-generated bindings use to load libfeatherkey_core.so.
    // The @aar classifier bundles the native JNA dispatcher for Android.
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    implementation(libs.androidx.core.ktx)
}
