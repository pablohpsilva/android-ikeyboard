#!/usr/bin/env bash
# Build the shared FeatherKey core for iOS and package it as an xcframework,
# plus generate the UniFFI Swift bindings. Analog of ffi-bridge/build-jni.sh.
# Artifacts (the .xcframework) are gitignored; the generated Swift is committed.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
crate_dir="$root/core/crates/featherkey-core"
gen_dir="$here/Generated"          # committed: ONLY the .swift binding
hdr_dir="$(mktemp -d)"             # transient: FFI header + module.modulemap for the xcframework
xcf="$here/FeatherKeyCore.xcframework"
t="$root/core/target"
lib=libfeatherkey_core.a

cd "$crate_dir"
echo "Building release static libs for device + simulator ..."
cargo build --release --locked --features uniffi --target aarch64-apple-ios
cargo build --release --locked --features uniffi --target aarch64-apple-ios-sim
cargo build --release --locked --features uniffi --target x86_64-apple-ios

# CRITICAL: generate bindings from an UNSTRIPPED build. [profile.release] sets
# `strip = true`, which drops .symtab — and uniffi-bindgen's --library metadata
# extraction reads .symtab, so generating from a release artifact yields ZERO
# files (observed). Build a host DEBUG dylib (unstripped) purely for generation.
echo "Building host debug dylib for binding generation (unstripped) ..."
cargo build --locked --features uniffi          # debug, host target
gen_lib="$t/debug/libfeatherkey_core.dylib"

# Fat simulator static lib (arm64-sim + x86_64-sim)
sim_dir="$(mktemp -d)"
lipo -create \
  "$t/aarch64-apple-ios-sim/release/$lib" \
  "$t/x86_64-apple-ios/release/$lib" \
  -output "$sim_dir/$lib"

# Generate the Swift binding (+ FFI header + modulemap) into a temp dir, then
# split: the .swift is committed under Generated/, the C header + modulemap go
# into the xcframework's headers so it vends the `featherkey_coreFFI` Clang module.
tmp_gen="$(mktemp -d)"
cd "$root/core"
cargo run -p uniffi-bindgen-tool -- generate \
  --library "$gen_lib" --language swift --out-dir "$tmp_gen"
# Guard against silent empty generation (the strip trap):
test -n "$(find "$tmp_gen" -name '*.swift' -print -quit)" || { echo "ERROR: no Swift generated"; exit 1; }

rm -rf "$gen_dir" && mkdir -p "$gen_dir" "$hdr_dir"
find "$tmp_gen" -name '*.swift' -exec cp {} "$gen_dir/" \;
find "$tmp_gen" -name '*FFI.h'  -exec cp {} "$hdr_dir/" \;
# UniFFI emits featherkey_coreFFI.modulemap; the xcframework needs it named
# module.modulemap in the headers dir to vend the Clang module.
find "$tmp_gen" -name '*.modulemap' -exec cp {} "$hdr_dir/module.modulemap" \;

# Assemble the xcframework: two static slices, each with the same C module headers.
rm -rf "$xcf"
xcodebuild -create-xcframework \
  -library "$t/aarch64-apple-ios/release/$lib" -headers "$hdr_dir" \
  -library "$sim_dir/$lib" -headers "$hdr_dir" \
  -output "$xcf"

echo "Done: $xcf ; committed Swift in $gen_dir"
