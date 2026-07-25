# JNA (UniFFI Kotlin bindings call the Rust core through JNA reflection).
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
# UniFFI-generated bindings + our FFI records/enums are reached reflectively via JNA.
-keep class com.featherkey.ffi.generated.** { *; }
-keep class uniffi.** { *; }
# Compose tooling keeps (belt-and-suspenders; AGP usually injects these).
-keep class androidx.compose.runtime.** { *; }
