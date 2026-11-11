# Reckon

A natural-language calculator for macOS, Windows, Linux, iOS and Android,
plus a terminal CLI. Write math the way you think it, and results show up next to each line:

```text
Price: $20 + 56 EUR              $85.12
Discounted: prev - 5% discount   $80.86
20% of $75                       $15
1 meter 20 cm                    120 cm
100 °F in celsius                37.7778 °C
today + 2 weeks                  Sat, 21 Nov 2026
2:30 pm HKT in Berlin            7:30 am CET
Rent: $1,200
Food: $450
Total: sum                       $1,650
```

The calculation engine is written in Rust and uses **no third-party
crates**. The desktop app uses one: [GPUI](https://gpui.rs), a
GPU-accelerated native UI framework. GPUI does not run on phones, so the iOS
app (SwiftUI/UIKit) and the Android app (Java, framework views) are thin
native front ends over the same engine. None of the apps use web views.

## Features

- Arithmetic in symbols or words (`8 times 9`, `6 (3)`), bitwise operators,
  powers, factorial, and hex, octal, binary and scientific output
- Units: length, area, volume, mass, time, temperature, data (SI and binary
  prefixes), angles, speed, CSS `px`/`pt`/`em` with adjustable `ppi`,
  and compound units like `km/h` and `m²`
- Currencies: ISO codes, names and symbols. Fiat and crypto rates are
  refreshed from the network, with offline fallback rates.
- Percentages: `20% of`, `5% on`, `6% off`, `as a % of`, `5% of what is …`
- Dates and times: `today`, `now`, `3 days ago`, `March 5 2027 - today`,
  `fromunix(…)`, calendar-aware month math
- Time zones from the system zoneinfo database (with daylight saving time):
  `PST time`, `time in Madrid`, `2:30 pm HKT in Berlin`
- Variables, `prev`, `sum`/`total`, `avg`/`average`, labels, headers and comments
- Functions: `sqrt`, `root`, `log`, `ln`, `round`, trigonometry, …
- Plugins: plain-text `.reckon` files that add units, variables and functions
- Sheets: any number of pages with a searchable sidebar, stored as plain
  files in a folder you choose, so iCloud Drive, Dropbox or Syncthing sync
  them between devices
- Quick calc: ⌃⌥Space opens a small Reckon box over any app; Enter copies
  the answer
- Twelve original themes on every platform, from graph-paper Paper to
  green-screen Phosphor, with a live-preview picker and a match-system mode

The full syntax is in the [User guide](docs/user-guide.md).

## Install

With [Homebrew](https://brew.sh), add the tap first. Homebrew 7 and later
also needs the `brew trust` line; older versions can skip it.

```bash
brew tap appliedsyntax/tap
```

```bash
brew trust appliedsyntax/tap
```

The Mac app:

```bash
brew install --cask reckon
```

The command-line tool, on macOS or Linux:

```bash
brew install reckon-cli
```

The Mac app is not yet signed with an Apple Developer ID, so macOS blocks
the first launch: open it once and dismiss the warning, then go to System
Settings › Privacy & Security and click **Open Anyway** (on macOS 14 and
earlier, right-click the app and choose Open instead). Packages for Windows, Linux,
Android and the iOS simulator are on the
[releases page](https://github.com/appliedsyntax/reckon/releases).

## Build from source

```bash
cargo run -p reckon-cli -- '$30 in EUR'
```

```bash
cargo run -p reckon-app --release
```

```bash
cargo test -p reckon-core
```

See [Building](docs/building.md) for toolchain notes and per-platform setup,
and [Packaging](docs/packaging.md) to build the `.dmg`, `.apk`, iOS app,
`.deb`/`.tar.gz` and Windows installer.

## Repository layout

| Path | What it is |
|---|---|
| `crates/reckon-core` | Engine: lexer, parser, units, currency, dates, time zones, plugins (no dependencies) |
| `crates/reckon-cli` | `reckon` command-line tool and REPL (no dependencies) |
| `crates/reckon-app` | Desktop app built on GPUI |
| `crates/reckon-ffi` | C interface to the engine and themes (`include/reckon.h`), used by the mobile apps |
| `ios/` | iOS app (Xcode project, SwiftUI + UIKit) |
| `android/` | Android app (Java + JNI shim, built without Gradle) |
| `packaging/` | Packaging scripts and icons for every platform |
| `.github/workflows/` | CI that builds every package |
| `docs/` | User guide, plugin guide, architecture, building, packaging, licensing notes |
| `examples/` | Sample documents and plugins |

## Documentation

- [User guide](docs/user-guide.md): every feature and its syntax
- [Plugins](docs/plugins.md): writing `.reckon` extensions
- [Architecture](docs/architecture.md): how the engine works and the design choices behind it
- [Building](docs/building.md): toolchain and per-platform setup
- [Packaging](docs/packaging.md): installers and packages for every platform
- [Licensing](docs/licensing.md): release checklist for the MIT license

## License

MIT. See [LICENSE](LICENSE). The desktop app's binary also includes
third-party code under other permissive licenses, listed with their notices
in [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt); see
[docs/licensing.md](docs/licensing.md).
