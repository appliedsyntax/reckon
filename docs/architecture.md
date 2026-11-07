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

## Dependency policy

The engine and CLI use only `std`. The app adds GPUI and nothing else.
Things that would normally pull in a crate are implemented in-tree instead:
JSON reading, TZif parsing, calendar math, HTTP (through `curl`) and
Windows time-zone lookup (one FFI call).
