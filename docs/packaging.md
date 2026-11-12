# Packaging

Every package is built by a script in `packaging/` and lands in `dist/`.

| Platform | Command | Output | Built on |
|---|---|---|---|
| macOS | `packaging/macos/build-dmg.sh` | `Reckon-<v>-macos-<arch>.dmg` | macOS |
| iOS (simulator) | `packaging/ios/build.sh` | `Reckon-<v>-ios-simulator.zip` | macOS + Xcode |
| iOS (device) | `DEVELOPMENT_TEAM=… packaging/ios/build.sh ipa` | `Reckon-<v>-ios.ipa` | macOS + Xcode |
| Android | `packaging/android/build-apk.sh` | `Reckon-<v>-android.apk` | macOS or Linux + Android SDK/NDK |
| Linux | `packaging/linux/build.sh` | `reckon-<v>-linux-<arch>.tar.gz`, `reckon_<v>_<arch>.deb` | Linux |
| Windows | `pwsh packaging/windows/build.ps1` | `Reckon-<v>-windows-x64.zip`, `…-setup.exe` | Windows |

`.github/workflows/release.yml` runs all of these on GitHub's macOS, Linux
and Windows runners. Pushing a `v*` tag also publishes a GitHub release with
the files attached.

## What each package contains

### macOS (`.dmg`)

- `Reckon.app` (GPUI desktop app), with the `reckon` CLI inside at
  `Contents/MacOS/reckon`
- a shortcut to `/Applications`, a short readme and the third-party license
  notices (also inside the app at `Contents/Resources/`)

By default the app is **ad-hoc signed**. That works on the Mac that built
it, but on other Macs Gatekeeper blocks it. On macOS 15 and later the user
has to open it once, then allow it under System Settings › Privacy &
Security › Open Anyway; on older versions right-click › Open is enough. For public distribution you need an Apple **Developer ID
Application** certificate (from the paid Apple Developer Program) and
notarization:

```bash
xcrun notarytool store-credentials reckon-notary --apple-id you@example.com --team-id TEAMID
```

```bash
SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" NOTARY_PROFILE=reckon-notary UNIVERSAL=1 packaging/macos/build-dmg.sh
```

`UNIVERSAL=1` builds for both Apple Silicon and Intel and needs
`rustup target add x86_64-apple-darwin`.

### iOS

GPUI does not run on iOS, so `ios/` holds a native app: SwiftUI with a
UIKit text view, so results line up with wrapped lines. It calls the same
Rust engine through `crates/reckon-ffi` (a C interface plus a bridging
header). Xcode's "Build Rust engine" step (`ios/build-rust.sh`) compiles
the engine for the platform being built.

- **Simulator:** unzip, then run
  `xcrun simctl install booted Reckon.app`.
- **Device or TestFlight:** open `ios/Reckon.xcodeproj`, choose your team
  under Signing, and run it. Or use the script's `ipa` mode, which passes
  `-allowProvisioningUpdates` so Xcode creates the profiles. Use
  `EXPORT_METHOD=app-store-connect` for TestFlight and App Store builds.
- The bundle id is `local.reckon.ios`. Change it to one you own before
  publishing.
- `MOBILE_LICENSES.txt` is bundled as a resource and shown under
  Theme › About & Licenses.

### Android (`.apk`)

`android/` is a plain Java app using framework views; there are no AndroidX
or Kotlin libraries. It is built **without Gradle**, using only the SDK's
own tools:

```text
cargo + NDK clang  →  lib/<abi>/libreckon.so  (Rust engine + JNI shim)
javac → d8         →  classes.dex
aapt2              →  manifest + launcher icons
zip → zipalign → apksigner
```

- ABIs: `arm64-v8a` and `x86_64` (set `ANDROID_ABIS` to change).
- `MOBILE_LICENSES.txt` is packed as `assets/licenses.txt` and shown under
  Theme › About & licenses.
- Android 11+ (minSdk 30), targetSdk 35. Native libraries are aligned for
  16 KB-page devices.
- Signed with a generated **debug key** by default. For a release, set
  `KEYSTORE`, `KEYSTORE_PASS`, `KEY_ALIAS` and `KEY_PASS`. Google Play
  requires an `.aab` (Android App Bundle) rather than an APK; build one with
  `bundletool` from the same pieces, or move to Gradle for Play releases.
- Install on a device or emulator:

```bash
adb install dist/Reckon-0.2.1-android.apk
```

### Linux

- The tarball holds `bin/reckon-app`, `bin/reckon`, a `.desktop` file and an
  icon. Extract it anywhere, or copy its contents into `~/.local`.
- The `.deb` installs into `/usr` and declares the X11/Wayland, Vulkan and
  font libraries it needs, plus `fonts-dejavu-core` for the editor font.
- Both include `share/doc/reckon/THIRD_PARTY_LICENSES.txt`.

### Windows

- `Reckon.exe` (the app, without a console window) and `bin\reckon.exe` (the
  CLI; add `bin` to `PATH` to use it). They are in separate folders because
  Windows file names are case-insensitive.
- `LICENSE.txt` and `THIRD_PARTY_LICENSES.txt`, in the zip and the install
  folder.
- The installer is built with Inno Setup (preinstalled on GitHub's Windows
  runners). It adds a Start-menu shortcut and an uninstaller.
- The `.exe` files have no embedded icon; the shortcuts and installer use
  `reckon.ico`. Embedding the icon needs a resource step (for example
  `rc.exe` from a `build.rs`).
- Windows SmartScreen warns about unsigned installers. Sign them with an
  Authenticode certificate (`signtool sign`) before distributing.

## Homebrew

The tap lives in a separate repository,
[appliedsyntax/homebrew-tap](https://github.com/appliedsyntax/homebrew-tap):

- `Casks/reckon.rb` installs `Reckon.app` from the release's universal DMG.
- `Formula/reckon-cli.rb` builds the `reckon` command from the tagged source.

For each release, after the `v*` tag's CI run has published the files,
update both with the new version and checksums:

```bash
shasum -a 256 Reckon-<v>-macos-universal.dmg
```

```bash
curl -sL https://github.com/appliedsyntax/reckon/archive/refs/tags/v<v>.tar.gz | shasum -a 256
```

Homebrew 6 and later will not load a third-party tap until it is trusted.
Installing by full name (`brew install --cask appliedsyntax/tap/reckon`)
adds the tap and trusts that one package, so the install instructions use
full names. Short names need `brew tap` and `brew trust appliedsyntax/tap`
first.

The official Homebrew catalog (`brew install --cask reckon` with no tap)
needs a Developer ID-signed, notarized app and an established project;
apply once both are true.

## Verified so far

| Package | Status |
|---|---|
| macOS `.dmg` | Built and mounted on macOS 27 (arm64). The app launches from the bundle and the bundled CLI runs. |
| iOS simulator | Built with Xcode 27 and run on an iPhone 16 Pro simulator (iOS 26). Typing, results, live rates and time zones checked. |
| Android `.apk` | Built with build-tools 36 / NDK 29 and run on an API 35 arm64 emulator. Typing, results and live rates checked. |
| Windows | Built by CI (zip and installer). The first run's package had the app overwritten by the CLI; fixed by moving the CLI to `bin\`. |
| Linux | The engine, CLI and C interface type-check; the package has not been built by CI yet. |
| iOS `.ipa` | Not built: needs your Apple team and provisioning. |

## Icons

`packaging/icon/make-icon.swift` draws the icon with AppKit and writes
`icon-mac-1024.png` (rounded, for macOS) and `icon-square-1024.png` (iOS,
Android). The Linux PNG, Windows `.ico` and mobile launcher icons are
derived from these and committed.
