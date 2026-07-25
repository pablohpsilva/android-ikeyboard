# jniLibs — generated, not committed

The `libfeatherkey_core.so` files that belong in `arm64-v8a/`, `armeabi-v7a/`, and
`x86_64/` under this directory are **build artifacts compiled from the Rust core**
in `../../../../crates`. They are intentionally **gitignored and never committed**:
a prebuilt binary checked into source control cannot be verified against the source
it claims to come from, which is the exact supply-chain risk FeatherKey's privacy /
reproducible-build posture (F-Droid, BR-65) exists to avoid.

## Regenerate them

```
./android/ffi-bridge/build-jni.sh
```

(Equivalently, the raw `cargo ndk` invocation documented in
`android/BUILD_AND_RUN.md` §4.) The build is pinned by the committed `Cargo.lock`,
so the dependency graph matches what was audited.

CI should build these from source as part of any job that assembles or signs an
APK, rather than consuming a checked-in binary.
