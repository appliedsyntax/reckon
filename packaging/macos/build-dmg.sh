#!/usr/bin/env bash
# Builds Reckon.app and a .dmg.
#
#   packaging/macos/build-dmg.sh
#
# Environment:
#   UNIVERSAL=1           build arm64 + x86_64 (needs both rustup targets)
#   SIGN_IDENTITY="..."   codesign identity, e.g. "Developer ID Application: Name (TEAMID)".
#                         Default "-" (ad-hoc: runs on this Mac, Gatekeeper blocks it elsewhere)
#   NOTARY_PROFILE=name   notarize + staple with a `xcrun notarytool store-credentials` profile
#   BUNDLE_ID=...         default local.reckon.app
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUNDLE_ID="${BUNDLE_ID:-local.reckon.app}"
SIGN_IDENTITY="${SIGN_IDENTITY:--}"
OUT="$ROOT/dist"
WORK="$ROOT/target/package-macos"
APP="$WORK/Reckon.app"

if [[ "${UNIVERSAL:-0}" == "1" ]]; then
  ARCH="universal"
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$t" -p reckon-app -p reckon-cli
  done
  BIN_APP=("target/aarch64-apple-darwin/release/reckon-app" "target/x86_64-apple-darwin/release/reckon-app")
  BIN_CLI=("target/aarch64-apple-darwin/release/reckon" "target/x86_64-apple-darwin/release/reckon")
else
  ARCH="$(uname -m)"
  cargo build --release --locked -p reckon-app -p reckon-cli
  BIN_APP=("target/release/reckon-app")
  BIN_CLI=("target/release/reckon")
fi

rm -rf "$WORK" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUT"
if [[ "$ARCH" == "universal" ]]; then
  lipo -create -output "$APP/Contents/MacOS/reckon-app" "${BIN_APP[@]}"
  lipo -create -output "$APP/Contents/MacOS/reckon" "${BIN_CLI[@]}"
else
  cp "${BIN_APP[0]}" "$APP/Contents/MacOS/reckon-app"
  cp "${BIN_CLI[0]}" "$APP/Contents/MacOS/reckon"
fi

# Icon
ICONSET="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET"
SRC="$ROOT/packaging/icon/icon-mac-1024.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$SRC" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$SRC" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp THIRD_PARTY_LICENSES.txt "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Reckon</string>
  <key>CFBundleDisplayName</key><string>Reckon</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleExecutable</key><string>reckon-app</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSHumanReadableCopyright</key><string>MIT License</string>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# Sign the inner CLI first, then the bundle.
SIGN_ARGS=(--force --sign "$SIGN_IDENTITY")
if [[ "$SIGN_IDENTITY" != "-" ]]; then
  SIGN_ARGS+=(--options runtime --timestamp)
fi
codesign "${SIGN_ARGS[@]}" "$APP/Contents/MacOS/reckon"
codesign "${SIGN_ARGS[@]}" "$APP"
codesign --verify --strict "$APP"

# Disk image: the app, a link to /Applications and a short readme.
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cp THIRD_PARTY_LICENSES.txt "$STAGE/Third-Party Licenses.txt"
cat > "$STAGE/Read Me.txt" <<'TXT'
Drag Reckon to Applications.

Command-line tool: the `reckon` CLI is inside the app. To use it in a terminal:
  sudo ln -sf /Applications/Reckon.app/Contents/MacOS/reckon /usr/local/bin/reckon

If macOS blocks the app (unsigned build): open it once and dismiss the
warning, then go to System Settings > Privacy & Security and click
"Open Anyway". On macOS 14 and earlier, right-click it and choose Open.
TXT
DMG="$OUT/Reckon-${VERSION}-macos-${ARCH}.dmg"
rm -f "$DMG"
hdiutil create -volname "Reckon ${VERSION}" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
if [[ "$SIGN_IDENTITY" != "-" ]]; then
  codesign --force --sign "$SIGN_IDENTITY" --timestamp "$DMG"
fi
if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi
echo "$DMG"
