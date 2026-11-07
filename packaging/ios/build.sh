#!/usr/bin/env bash
# Builds the iOS app.
#
#   packaging/ios/build.sh               simulator build -> dist/Reckon-<v>-ios-simulator.zip
#   DEVELOPMENT_TEAM=ABCDE12345 packaging/ios/build.sh ipa
#                                         signed device build -> dist/Reckon-<v>-ios.ipa
#
# The .ipa needs an Apple developer team and lets Xcode create provisioning
# profiles (-allowProvisioningUpdates). EXPORT_METHOD defaults to
# "debugging" (registered devices); use "app-store-connect" for TestFlight.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
OUT="$ROOT/dist"
WORK="$ROOT/target/package-ios"
mkdir -p "$OUT" "$WORK"
cd "$ROOT/ios"

case "${1:-simulator}" in
  simulator)
    xcodebuild -project Reckon.xcodeproj -scheme Reckon -configuration Release \
      -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' \
      -derivedDataPath "$WORK/dd" CODE_SIGNING_ALLOWED=NO build -quiet
    APP="$WORK/dd/Build/Products/Release-iphonesimulator/Reckon.app"
    ZIP="$OUT/Reckon-${VERSION}-ios-simulator.zip"
    rm -f "$ZIP"
    (cd "$(dirname "$APP")" && ditto -c -k --keepParent Reckon.app "$ZIP")
    echo "$ZIP"
    echo "Install: unzip it, then: xcrun simctl install booted Reckon.app"
    ;;
  ipa)
    : "${DEVELOPMENT_TEAM:?set DEVELOPMENT_TEAM to your Apple team ID}"
    ARCHIVE="$WORK/Reckon.xcarchive"
    xcodebuild -project Reckon.xcodeproj -scheme Reckon -configuration Release \
      -destination 'generic/platform=iOS' -archivePath "$ARCHIVE" \
      DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" -allowProvisioningUpdates archive -quiet
    cat > "$WORK/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key><string>${EXPORT_METHOD:-debugging}</string>
  <key>teamID</key><string>${DEVELOPMENT_TEAM}</string>
  <key>signingStyle</key><string>automatic</string>
</dict>
</plist>
PLIST
    xcodebuild -exportArchive -archivePath "$ARCHIVE" -exportPath "$WORK/export" \
      -exportOptionsPlist "$WORK/ExportOptions.plist" -allowProvisioningUpdates
    cp "$WORK/export/Reckon.ipa" "$OUT/Reckon-${VERSION}-ios.ipa"
    echo "$OUT/Reckon-${VERSION}-ios.ipa"
    ;;
  *) echo "usage: $0 [simulator|ipa]" >&2; exit 2 ;;
esac
