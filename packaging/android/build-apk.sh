#!/usr/bin/env bash
# Builds the Android APK with the SDK's own tools (no Gradle):
# cargo (Rust engine) -> NDK clang (JNI shim) -> javac -> d8 -> aapt2 ->
# zipalign -> apksigner.
#
#   packaging/android/build-apk.sh
#
# Environment:
#   ANDROID_HOME     SDK location (default: ~/Library/Android/sdk or ~/Android/Sdk)
#   ANDROID_ABIS     default "arm64-v8a x86_64"
#   KEYSTORE, KEYSTORE_PASS, KEY_ALIAS, KEY_PASS
#                    release signing key; default is a generated debug key
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
VERSION_CODE="${VERSION_CODE:-1}"
MIN_SDK=30
TARGET_SDK=35

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
for d in "$SDK" "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
  if [[ -n "$d" && -d "$d/platforms" ]]; then SDK="$d"; break; fi
done
[[ -d "$SDK/platforms" ]] || { echo "Android SDK not found; set ANDROID_HOME" >&2; exit 1; }
latest() { ls "$1" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1; }
BT="$SDK/build-tools/$(latest "$SDK/build-tools")"
NDK="${ANDROID_NDK_HOME:-$SDK/ndk/$(latest "$SDK/ndk")}"
JAR="$SDK/platforms/android-$TARGET_SDK/android.jar"
[[ -f "$JAR" ]] || JAR="$SDK/platforms/$(ls "$SDK/platforms" | grep -E '^android-[0-9]+$' | sort -t- -k2,2n | tail -1)/android.jar"
HOST_TAG="$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)"
LLVM="$NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"

WORK="$ROOT/target/package-android"
OUT="$ROOT/dist"
rm -rf "$WORK" && mkdir -p "$WORK/classes" "$WORK/stage" "$OUT"
cd "$ROOT"

# 1. Rust engine + JNI shim for each ABI
for abi in ${ANDROID_ABIS:-arm64-v8a x86_64}; do
  case "$abi" in
    arm64-v8a) triple=aarch64-linux-android ;;
    x86_64) triple=x86_64-linux-android ;;
    armeabi-v7a) triple=armv7-linux-androideabi ;;
    *) echo "unknown ABI $abi" >&2; exit 1 ;;
  esac
  clang_triple="${triple/armv7/armv7a}"
  CC="$LLVM/${clang_triple}${MIN_SDK}-clang"
  env_triple="$(echo "$triple" | tr 'a-z-' 'A-Z_')"
  env "CARGO_TARGET_${env_triple}_LINKER=$CC" cargo build --release -p reckon-ffi --target "$triple"
  mkdir -p "$WORK/stage/lib/$abi"
  "$CC" -shared -fPIC -O2 -s \
    -I crates/reckon-ffi/include android/jni/reckon_jni.c \
    "target/$triple/release/libreckon_ffi.a" \
    -llog -ldl -lm -Wl,--gc-sections -Wl,-soname,libreckon.so -Wl,-z,max-page-size=16384 \
    -o "$WORK/stage/lib/$abi/libreckon.so"
done

# 2. Java -> dex
javac -Xlint:-options -source 17 -target 17 -classpath "$JAR" -d "$WORK/classes" \
  $(find android/src -name '*.java')
"$BT/d8" --release --min-api "$MIN_SDK" --lib "$JAR" --output "$WORK/stage" \
  $(find "$WORK/classes" -name '*.class')

# 3. Resources + manifest
"$BT/aapt2" compile --dir android/res -o "$WORK/res.zip"
# The license notices shown on the About screen.
mkdir -p "$WORK/assets"
cp MOBILE_LICENSES.txt "$WORK/assets/licenses.txt"
"$BT/aapt2" link -o "$WORK/unsigned.apk" -I "$JAR" --manifest android/AndroidManifest.xml \
  -A "$WORK/assets" \
  --min-sdk-version "$MIN_SDK" --target-sdk-version "$TARGET_SDK" \
  --version-code "$VERSION_CODE" --version-name "$VERSION" "$WORK/res.zip"
(cd "$WORK/stage" && zip -qr -X "$WORK/unsigned.apk" classes.dex lib)

# 4. Align + sign
"$BT/zipalign" -P 16 -f 4 "$WORK/unsigned.apk" "$WORK/aligned.apk" 2>/dev/null \
  || "$BT/zipalign" -f 4 "$WORK/unsigned.apk" "$WORK/aligned.apk"
if [[ -z "${KEYSTORE:-}" ]]; then
  KEYSTORE="$ROOT/target/android-debug.keystore"
  KEYSTORE_PASS=android KEY_ALIAS=reckondebug KEY_PASS=android
  [[ -f "$KEYSTORE" ]] || keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android \
    -alias reckondebug -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Reckon Debug" >/dev/null 2>&1
fi
APK="$OUT/Reckon-${VERSION}-android.apk"
"$BT/apksigner" sign --v4-signing-enabled false --ks "$KEYSTORE" --ks-pass "pass:$KEYSTORE_PASS" --ks-key-alias "$KEY_ALIAS" \
  --key-pass "pass:${KEY_PASS:-$KEYSTORE_PASS}" --out "$APK" "$WORK/aligned.apk"
"$BT/apksigner" verify "$APK"
echo "$APK"
