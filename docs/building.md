# Building

## Toolchain

`rust-toolchain.toml` pins Rust **1.97.1**, because GPUI's dependencies need
a recent compiler (1.88 or newer). With rustup installed, the right
toolchain is picked up automatically inside this folder.

```bash
cargo build                    # engine + CLI (default members)
```

```bash
cargo build -p reckon-app      # desktop app
```

```bash
cargo test -p reckon-core      # engine tests
```

## macOS

You need the Xcode command-line tools, or Xcode itself.

The app enables GPUI's `runtime_shaders` feature, which compiles the Metal
shaders when the app starts instead of at build time. That way the build
does not need Xcode's separate **Metal Toolchain** download. To precompile
the shaders instead (slightly faster startup), install the toolchain and
remove the feature in `crates/reckon-app/Cargo.toml`:

```bash
xcodebuild -downloadComponent MetalToolchain
```

The app runs as a plain binary (`target/release/reckon-app`). To make it a
double-clickable `.app`, wrap the binary in a bundle with an `Info.plist`.
Packaging and code signing are not set up yet.

## Linux

GPUI needs the X11/Wayland, Vulkan and font development packages. On
Debian/Ubuntu, for example:

```bash
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libfontconfig-dev libx11-xcb-dev libxcb1-dev
```

Time zones come from `/usr/share/zoneinfo`. Install the `tzdata` package in
minimal containers.

## Windows

Build with the MSVC toolchain. The engine reads the local UTC offset through
`GetTimeZoneInformation`. Named zones use fixed standard offsets because
Windows has no zoneinfo database. Exchange rates use the `curl.exe` that
ships with Windows 10 and later.

> Status: the engine and CLI are platform-neutral and build on all three
> systems. The desktop app has so far only been built and run on macOS.

## Release builds

```bash
cargo build --release -p reckon-cli -p reckon-app
```

The release profile uses thin LTO, one codegen unit and stripped symbols.

## iOS and Android

GPUI does not target phones. The mobile apps are native front ends over the
same engine, which is exposed through `crates/reckon-ffi` (a C interface;
see `include/reckon.h`):

- **iOS:** `ios/Reckon.xcodeproj`. Xcode builds the Rust static library in a
  build phase. You need `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.
- **Android:** `android/`, built by `packaging/android/build-apk.sh` with the
  SDK and NDK. You need
  `rustup target add aarch64-linux-android x86_64-linux-android`.

Phones have no `curl` command or `/etc/localtime`, so the apps download
exchange rates themselves and pass the JSON to the engine
(`reckon_apply_rates`). They also tell the engine the local time zone
(`reckon_set_local_zone`). Android's packed `tzdata` file is read directly,
so named zones get correct daylight saving time there too.

See [packaging.md](packaging.md) for building and signing every package.
