#!/bin/sh
# Xcode "Build Rust engine" phase: compiles reckon-ffi as a static library
# for the platform being built and copies it where the linker expects it.
set -eu
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
case "${PLATFORM_NAME:-iphonesimulator}" in
  iphoneos) TARGET=aarch64-apple-ios ;;
  *)
    if [ "${NATIVE_ARCH_ACTUAL:-arm64}" = "x86_64" ]; then TARGET=x86_64-apple-ios; else TARGET=aarch64-apple-ios-sim; fi ;;
esac
if [ "${CONFIGURATION:-Debug}" = "Release" ]; then FLAGS="--release"; DIR=release; else FLAGS=""; DIR=debug; fi
cd "$SRCROOT/.."
# Xcode exports SDK settings that confuse cargo's host build scripts.
env -u SDKROOT -u LIBRARY_PATH cargo build -p reckon-ffi --target "$TARGET" $FLAGS
mkdir -p "$TARGET_TEMP_DIR/rust"
cp "target/$TARGET/$DIR/libreckon_ffi.a" "$TARGET_TEMP_DIR/rust/libreckon_ffi.a"
