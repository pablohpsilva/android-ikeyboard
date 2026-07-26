#!/usr/bin/env bash
# Reproducibly build the native FeatherKey core (libfeatherkey_core.so) for every
# shipped Android ABI, straight from the audited Rust in ../../../core/crates.
#
# WHY THIS EXISTS: the .so files are BUILD ARTIFACTS, not source. They are
# gitignored and must never be committed — a prebuilt binary in the tree cannot be
# verified against the source it claims to be, which is exactly the supply-chain
# risk this project's privacy posture (F-Droid, BR-65) rules out. Anyone can
# regenerate them from source with this one command.
#
# PREREQUISITES (see BUILD_AND_RUN.md §1 and §3):
#   - Android NDK (r27+) with ANDROID_NDK_HOME set.
#   - rustup targets: aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   - cargo install cargo-ndk
#   - The UniFFI overlay applied into crates/featherkey-core (BUILD_AND_RUN.md §3).
#
# The build is pinned by the committed Cargo.lock (--locked), so the dependency
# graph is identical to what was audited.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
core_dir="$here/../../../core/crates/featherkey-core"
out_dir="$here/src/main/jniLibs"

echo "Building libfeatherkey_core.so for arm64-v8a, armeabi-v7a, x86_64 -> $out_dir"
cd "$core_dir"
cargo ndk \
  -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o "$out_dir" \
  build --release --locked --features uniffi

echo "Done. Built (uncommitted) libraries:"
find "$out_dir" -name '*.so' -exec ls -la {} +
