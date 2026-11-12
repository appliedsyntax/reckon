# Architecture

## Crates

```text
reckon-core   engine library                  std only
   ▲  ▲  ▲
   │  │  └── reckon-cli   `reckon` binary      std only
   │  └───── reckon-app   desktop app          gpui
   └──────── reckon-ffi   C interface          std only
                ▲    ▲
                │    └── android/  Java + JNI shim (framework views)
                └─────── ios/      Swift (SwiftUI + UIKit)
```

The engine has no UI and no dependencies, so every front end shares it.

## Evaluating a line

```text
text ─► prepare ─► lexer ─► classify ─► parser ─► eval ─► format
        (label,    (tokens   (units,     (AST)     (Value)  (string)
        quotes,    + spans)  keywords,
        comments)            zones…)
```

1. **prepare** (`lib.rs`) decides whether a line is a header, comment or
   expression. It blanks out labels (`Price:`), quoted text and trailing
   `//` comments with spaces, so byte offsets still match the original line
   and highlighting stays accurate.
2. **lexer** (`lexer.rs`) produces numbers (including `0x`, `5 300`,
   `1.5e3`), words, symbols, `2:30`-style times and ISO dates.
3. **classify** (`parser.rs`) gives words meaning by matching the longest
   phrase first (`nautical miles`, `New York`, `multiplied by`). Its
   priority order is: assignment target, keyword, variable, function, scale,
   constant, unit, zone, month. Words that match nothing are **dropped**,
   which is why prose like `prev - 5% discount` works.
4. **parser** (`parser.rs`) is recursive descent. From loosest to tightest
   binding:
   conversion (`in`/`as`, `as a % of`) → `|` → `xor` → `&` → shifts →
   `+ -` → `* / mod` and implicit juxtaposition → `of / on / off` → unary
   minus → `^` → postfix (units, `%`, `!`, scales, `am/pm`, zones, `ago`,
   `from now`) → primary.
5. **eval** (`eval.rs`) works on three kinds of `Value`: `Num(Quantity)`
   (a number with a compound unit and a display format), `Percent`, and
   `Time(DateTime)`.
6. **format** (`value.rs`) handles grouping, precision, currency symbols,
   radix formats and date display.

`DocState` carries variables, `prev`, the current `sum` block and the
`ppi`/`em` values from line to line.

## Units

A unit (`units.rs`) has a 7-part dimension vector (length, mass, time,
temperature, data, angle, money) and a conversion:

- `Linear(factor)` for most units
- `Affine { factor, offset }` for temperatures
- `Currency(code)`, whose factor comes from exchange rates (the money base
  is USD)
- `Px` and `Em`, whose factors depend on the document's `ppi` and `em`

A compound `Unit` is a list of `(unit id, power)`. Multiplying adds the
powers, and two values are compatible when their dimension vectors are
equal. This lets `km/h`, `m²`, `USD/h` and `$/€` all work without special
cases.

SI and binary prefixes are generated when the registry is built. Unit
symbols are case-sensitive (`MB` and `mb` differ) and unit names are not
(`Kilometers` works).

## Time and time zones

`time.rs` implements civil-date arithmetic using Howard Hinnant's
algorithms. `tz.rs` reads the system **TZif** files directly (v1 and v2+)
and evaluates the **POSIX TZ footer** for dates after the last transition,
which modern "slim" zoneinfo files rely on. The local zone comes from `TZ`,
then `/etc/localtime`, and on Windows from `GetTimeZoneInformation` through
a small FFI declaration.

City and abbreviation tables map names to IANA zones, each with a fallback
standard offset for systems that have no zoneinfo database. Tests use
`Engine::with_fixed_clock` with fixed-offset zones so they don't depend on
the machine running them.

## Currency rates

`currency.rs` stores rates as units per USD. Refreshing runs the system
`curl` binary instead of linking an HTTP and TLS stack. `curl` ships with
macOS, Windows 10 and later, and nearly every Linux distribution. A small
JSON reader (`json.rs`) parses the responses. Rates are cached as plain text
in `rates.txt`.

## Desktop app

`reckon-app` is one GPUI view (`calc.rs`):

- A custom `Element` lays out and paints the editor: highlighted text runs,
  the selection, the cursor and the active line. It implements
  `EntityInputHandler`, so IME composition, the character palette and dead
  keys work.
- The results column is a list of normal GPUI `div`s, one per line at the
  same line height. Clicking a result copies it.
- Every edit re-evaluates the whole document, which takes well under a
  millisecond for typical documents, and autosaves it.
- Exchange rates refresh on a background executor when stale.

Keyboard shortcuts use `cmd` on macOS and `ctrl` elsewhere (see `main.rs`).

### Window, menus and settings

The window chrome is in `calc/chrome.rs`.

- **Toolbar.** A 38-point bar with the sidebar toggle, the sheet title, New
  Sheet and Settings. On macOS the window has a transparent title bar and
  the toolbar is drawn under the window buttons (`traffic_light_position`
  centres them in it), which is how native unified toolbars look. AppKit
  still moves the window when the title-bar strip is dragged; a
  double-click calls `titlebar_double_click`, which reads the user's
  "double-click a window's title bar to" setting. Windows and Linux keep
  the system title bar, so snapping and the caption buttons are the
  system's own.
- **Menus (macOS).** `app_menus` builds the standard set: the application
  menu (About, Settings…, Services, Hide, Hide Others, Show All, Quit),
  File, Edit, View, Window and Help. A menu named "Window" is registered as
  the app's windows menu, so macOS adds its tiling commands and the window
  list to it. The Edit items are tagged with `OsAction`, so they also work
  in system text fields such as the Open and Save dialogs. The menus are
  rebuilt when a label or tick changes (theme, Show/Hide Sidebar,
  Enter/Exit Full Screen). GPUI has no menu bar on Windows and Linux; the
  toolbar and the shortcuts cover the same commands there.
- **Shortcuts.** App and window shortcuts are bound without a key context,
  so they work wherever the focus is; editing shortcuts are bound in the
  `Calc` context and are off while Settings or the theme picker is open.
- **Text size.** `font_size` drives the editor's font size, the line height
  (1.93 × size, rounded, which is 28 at the default 14.5) and the width of
  the results column. The app's own chrome (toolbar, sidebar, status bar,
  panels) sets a fixed size and line height, so it does not scale.
- **Settings.** A panel inside the main window rather than a second window:
  it previews changes on the document behind it and behaves the same on
  every platform. The shortcut recorder uses `intercept_keystrokes`, which
  sees a keystroke before any key binding does, and `hotkey::rebind`
  registers the new shortcut before releasing the old one.
- **Window frame.** The size and position are saved in `app.conf` when they
  change (written with the next poll, on close and on quit). On launch
  `fit_frame` clamps the saved frame to the minimum size and the main
  display.
- **Icons.** Four SVGs drawn for Reckon, embedded with `include_str!` and
  served through GPUI's `AssetSource` (`assets.rs`).

## Sheets

`reckon_core::sheets` stores sheets as a folder of plain `.reckon` files,
and every app (desktop, iOS through `reckon_sheets_*` in the C interface,
Android through the same functions over JNI) uses it, so names, titles,
trash and listing behave the same everywhere.

- The folder listing is the only index: no database, nothing to corrupt or
  migrate, and any sync tool can carry the folder between devices.
- A sheet's title is its first line. Files are named after the title when
  created and renamed only when the user leaves the sheet (`tidy`), so sync
  tools do not see a stream of renames while someone types.
- Writes go to a temporary file that is renamed over the sheet, so a sync
  tool never uploads a half-written file.
- The apps poll the open sheet's modification time (every 1.5–2 s) and
  reload it when another device changed it; the folder's own modification
  time tells them when sheets were added, removed or renamed.
- Deleting moves a sheet into `.trash` inside the folder; Undo moves it back.

## Quick calc (desktop)

The quick box is the same editor view (`Calc`) in a one-line mode: no sheet,
no status bar, Enter copies the answer and closes the window, losing focus
closes it. It opens as a GPUI pop-up window, which on macOS is a
non-activating panel, so Reckon's main window does not jump in front of the
app you were using.

The global shortcut is registered without extra crates (`hotkey.rs`):
Carbon's `RegisterEventHotKey` on macOS (no accessibility permission),
`RegisterHotKey` on a message-loop thread on Windows. Linux desktops do not
offer global shortcuts to apps, so `reckon-app --quick` signals the running
app over a Unix socket in the data folder; users bind that command in their
desktop's keyboard settings.

## Themes

The twelve themes live in the engine (`reckon_core::themes`), so every app
draws the same colours from one table:

- Each theme has 23 colours (background, results column, text, result,
  the nine highlight classes, cursor, selection, rules, status bar, picker
  surfaces…) plus its finishing touches: a texture (`grid`, `ruled`,
  `scanlines` or none), a single or double divider, the result weight and
  a bar or block cursor.
- Tests in `themes.rs` check WCAG contrast for every theme: text at least
  7:1, results and highlights at least 4.5:1.
- The desktop app reads the table directly. The iOS and Android apps get it
  as JSON from `reckon_themes()`.
- `reckon_evaluate_highlighted()` returns each line's result plus its
  highlight spans, with offsets in UTF-16 units (what `NSString` and Java
  strings index by), so the mobile editors colour text the same way as the
  desktop one.

Each app draws the textures itself: GPUI quads on the desktop, a backdrop
view behind the `UITextView` on iOS, and `onDraw` on Android. The choice
(follow the system with a light and a dark theme, or one fixed theme) is
saved in `app.conf` on the desktop, in `UserDefaults` on iOS and in
`SharedPreferences` on Android.

## Untrusted input and limits

A sheets folder can be shared or synced, so another device or person may
write to it, and the open sheet is evaluated as soon as the app starts.
Anything in that folder, and anything a rate service sends, is treated as
untrusted.

- **Only regular files.** `sheets.rs` never reads or replaces a symbolic
  link or a device (`symlink_metadata`, checked again on the open file).
  A link named like a sheet could otherwise show the contents of any file
  the user can read, and a save would write through it.
- **Exclusive temporary files.** A save writes to a temporary file with an
  unpredictable name, opened with `create_new`, then renames it over the
  sheet. A predictable name could be pre-created as a link to another file.
- **Sizes.** Sheets over 4 MB are not opened or listed. The list reads only
  the first 4 KB of each sheet (title and preview).
- **File names.** A sheet name with a path separator or a leading dot is
  refused, including through the C interface.
- **Lines.** A line over 8 KB, or with more than 400 numbers, words and
  operators, is not evaluated. The parser stops at 96 levels of nesting
  (parentheses, signs, powers). These bound the recursion depth: the
  deepest line allowed needs about 230 KB of stack in a release build,
  and the smallest main-thread stack Reckon runs on is 1 MB (iOS,
  Windows). `tests/hostile.rs` checks this on a 512 KB stack.
- **Plugin functions.** A line may make 10,000 user-function calls; the
  nesting limit of 32 alone would allow functions that call each other
  twice to do billions of evaluations.
- **Rates.** `curl` is restricted to HTTPS (also after redirects) and to a
  1 MB response. The JSON reader stops at 32 levels and 1 MB. Rates must be
  positive, finite numbers with alphanumeric codes, and the update time is
  never later than now, so bad data cannot stop later refreshes.
- **Settings.** Control characters are removed from values written to
  `app.conf`, so a value cannot add another setting on a new line.
- **Quick-calc socket.** When the data folder's path is too long for a
  socket, the socket goes in `$XDG_RUNTIME_DIR` or in a folder under the
  temp folder that is checked to be private to the user.
- **Builds.** CI build jobs have read-only access to the repository; only
  the release job can write. Actions are pinned to commits and cargo runs
  with `--locked`.

## Performance

- **Start-up.** The editor font is asked for by name. Listing every
  installed font to pick one took about 150 ms on macOS, half of the
  start-up time. The window is opened before the menu bar is built.
  `RECKON_DEBUG=1` prints a timeline of start-up stages.
- **Large sheets.** Only the lines in view (plus 30 rows either side, and
  the rows the cursor and selection ends are on) are shaped, and only the
  result rows in view become elements; spacers stand in for the rest. A
  3,000-line sheet redraws in about 6 ms; before, it took about 33 ms.
- **Idle.** Nothing in Reckon runs on a timer except the sheets poll every
  1.5 s (two `stat` calls; the folder is re-listed every 12 s, reading 4 KB
  per sheet). GPUI's display link runs while the window is visible, which
  accounts for the app's idle CPU use of under 1%.
- **Memory.** The undo history is capped at 500 steps and 8 MB.

## Dependency policy

The engine and CLI use only `std`. The app adds GPUI and nothing else.
Things that would normally pull in a crate are implemented in-tree instead:
JSON reading, TZif parsing, calendar math, HTTP (through `curl`) and
Windows time-zone lookup (one FFI call).
