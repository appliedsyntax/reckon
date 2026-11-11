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

- On 2026-11-10 the shipped set across macOS, Windows and Linux was 539
  crates. On Linux `self_cell` is offered as "Apache-2.0 OR GPL-2.0-only";
  Reckon uses it under Apache-2.0, so the GPL option never applies.

## Third-party notices

`THIRD_PARTY_LICENSES.txt` holds the license notice of every crate compiled
into the desktop app and CLI, plus the Rust standard library. Apache-2.0,
BSD, Unicode and Zlib require these notices to ship with binaries, so the
packaging scripts copy the file into:

- macOS: `Reckon.app/Contents/Resources/` and the DMG ("Third-Party
  Licenses.txt")
- Windows: the zip and the install folder
- Linux: `share/doc/reckon/` in the tarball and the `.deb`

The file is generated from `Cargo.lock` with only `cargo metadata` and the
crate sources cargo has already downloaded:

```bash
python3 packaging/third-party-licenses.py
```

- It follows normal dependencies from `reckon-app` and `reckon-cli` for all
  desktop targets. Procedural macros and build-only dependencies are not in
  the binaries and are left out.
- Where a crate offers a choice ("MIT OR Apache-2.0"), it records the
  license used (MIT first) and includes only that license's text.
- 41 crates publish no license file. For those it uses the standard license
  text with the crate's authors as the copyright holders.
- `option-ext` is MPL-2.0. It is used unmodified, and its entry links to its
  source, which is what MPL asks of a binary distribution.
- **FreeType.** The `freetype-sys` crate (Linux only) links the system
  FreeType when it is 2.12.1 or newer and otherwise compiles its own copy
  into the app; the CI runner (Ubuntu 22.04) is in the second group.
  FreeType is used under the FreeType License, which requires a credit in
  the product's documentation, so the file always carries that credit, the
  FTL text and the notices of FreeType's PCF, BDF and zlib parts.
- Re-checked on 2026-11-11 after adding sheets and quick calc: no new
  crates. The global shortcut and sheet storage use only the operating
  system's APIs and the standard library.

Run it whenever `Cargo.lock` changes. The Linux CI job regenerates it and
fails if the committed file is out of date.

## Colour themes

All twelve themes were designed for Reckon. Their names, colours and
textures are original, so no theme licence applies.

## Fonts

No font is bundled, so no font license applies to the packages. The editor
uses each system's own monospaced font:

| Platform | Font | Fallback |
|---|---|---|
| macOS | SF Mono, loaded from the system at runtime | Menlo |
| iOS | SF Mono (`monospacedSystemFont`) | — |
| Android | System monospace (`Typeface.MONOSPACE`) | — |
| Windows | Cascadia Mono | Consolas |
| Linux | DejaVu Sans Mono (the `.deb` depends on `fonts-dejavu-core`) | Liberation Mono, Noto Sans Mono |

SF Mono's license allows use only on Apple platforms and forbids
redistributing it, which is why it is never copied into a package. For an
identical look everywhere, bundle an SIL Open Font License font (for example
JetBrains Mono or IBM Plex Mono) and add its license to the notices.

## Before a public release

1. **Third-party notices.** Done: see above. Keep the file current.
2. **Copyright line.** `LICENSE` currently says "The Reckon Authors".
   Replace it with the real copyright holder (a person or company).
3. **Name.** "Reckon" is a working name and has not been checked for
   trademark conflicts. An existing accounting-software company is called
   Reckon. Clear the name, or pick another, before publishing.
4. **Exchange-rate services.** Their terms of service, not our code
   license, govern the data the app downloads; the MIT license on the source
   is unaffected.
   - ExchangeRate-API's open-access endpoint (`open.er-api.com`) allows
     personal and commercial use with attribution and forbids redistributing
     the data. The app names it in the status bar, and the phone apps'
     About screen links back to it as their docs ask. The desktop app has no
     About screen yet; add the link there too.
   - CoinGecko's keyless public API requires attribution and is described as
     being for prototyping, open-source and educational use; commercial use
     needs one of their paid plans. Before selling Reckon, or if they retire
     keyless access, get a plan, make crypto rates optional, or drop them.
5. **Mobile notices.** Done. The iOS and Android apps contain the Rust
   standard library (MIT OR Apache-2.0) and nothing else from third parties.
   The generator also writes `MOBILE_LICENSES.txt` (Reckon's license and the
   standard library's notice), which is bundled in both apps and shown under
   Theme › About & Licenses, next to links crediting ExchangeRate-API and
   CoinGecko.
6. **Time-zone data** is read at runtime from the operating system and not
   redistributed. The built-in city table contains only facts (names and
   offsets).
