# Licensing

Reckon's own code is released under the **MIT License** (see `LICENSE`).
These notes explain why that works and what a public release still needs.

## Our code

All of the engine, CLI and app code was written for this project. Nothing
was copied from Numi: its application source is not public, and its GitHub
repository holds only documentation, an Alfred workflow and sample plugins.
The feature set was implemented from Numi's public user documentation.
Behaviour and features are not copyrightable, and none of its text, code,
name or artwork is used here.

## Dependencies

- **reckon-core**, **reckon-cli** and **reckon-ffi** use no third-party
  crates. The iOS app uses only Apple frameworks, and the Android app only
  the Android framework (no AndroidX or other libraries).
- So the CLI and the mobile apps contain only our MIT code, the Rust
  standard library (MIT OR Apache-2.0) and the operating system's own
  frameworks.
- **reckon-app** depends on GPUI (Apache-2.0), which brings in about 490
  crates on macOS. On 2026-11-07 their licenses were all permissive:

  | License | Crates |
  |---|---|
  | MIT and/or Apache-2.0 (in various combinations) | ~410 |
  | Unicode-3.0 | 18 |
  | BSD-3-Clause / BSD-2-Clause | 13 |
  | Zlib, ISC, BSL-1.0, CC0, 0BSD, Unlicense (alone or as alternatives) | ~25 |
  | MPL-2.0 | 2 (`cbindgen`, a build tool that is not shipped; `option-ext`) |

  There are **no GPL, LGPL or AGPL** crates. Linking permissive code into
  an MIT application is fine. MPL-2.0 is file-level copyleft: it only
  requires that `option-ext`'s own files stay available under MPL, and they
  are, unmodified, on crates.io.

To re-check the licenses:

```bash
cargo metadata --format-version 1
```

Or use `cargo deny check licenses` if you have cargo-deny.

## Before a public release

1. **Third-party notices.** Apache-2.0, BSD, Unicode and Zlib all require
   their copyright and license text to be shipped with binaries. Generate a
   `THIRD_PARTY_LICENSES` file (for example with `cargo about generate`) and
   include it with the app bundle and installers. The CLI only needs Rust's
   standard library notice.
2. **Copyright line.** `LICENSE` currently says "The Reckon Authors".
   Replace it with the real copyright holder (a person or company).
3. **Name.** "Reckon" is a working name and has not been checked for
   trademark conflicts. An existing accounting-software company is called
   Reckon. Clear the name, or pick another, before publishing.
4. **Exchange-rate services.** The rate refresh calls the free
   ExchangeRate-API open-access endpoint (`open.er-api.com`) and the
   CoinGecko public API. Their terms of service, not our code license,
   govern that data. Both ask for attribution on their free tiers and limit
   request rates. The app names them in its status bar ("Rates:
   ExchangeRate-API + CoinGecko"). Check their current terms and add links
   in an About screen if required, or switch providers.
5. **Time-zone data** is read at runtime from the operating system and not
   redistributed. The built-in city table contains only facts (names and
   offsets).
