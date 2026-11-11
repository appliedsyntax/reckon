# User guide

Reckon evaluates a document line by line. Each line can be a calculation, a
header, a comment or blank. A line can use variables and results from the
lines above it.

## Writing lines

| You write | Meaning |
|---|---|
| `# Groceries` | Header (no result; also starts a new block for `sum`) |
| `// note` | Comment line |
| `Rent: $1,200` | `Rent:` is a label and is not evaluated |
| `$275 for the "Model 227"` | Text in double quotes is ignored |
| `prev - 5% discount` | Unknown words are ignored, so prose is fine |
| `1 m 20 cm = 120 cm` | Anything after a top-level `=` is ignored, so you can paste annotated examples |

A colon between digits (`2:30`) is a time, not a label.

## Operators

| Operation | Symbols and words |
|---|---|
| Addition | `+`, `plus`, `and`, `with` |
| Subtraction | `-`, `minus`, `subtract`, `without` |
| Multiplication | `*`, `×`, `times`, `multiplied by`, `mul` |
| Division | `/`, `÷`, `divide`, `divide by`, `divided by`, `per` |
| Exponent | `^`, `**` |
| Modulo | `mod` |
| Bitwise | `&`, `|`, `xor`, `<<`, `>>` |
| Factorial | `5!` or `fact 5` |

Two values written next to each other are combined:

- With compatible units they are **added**, using the smaller unit:
  `1 meter 20 cm` → `120 cm`, `5 ft 3 inches` → `63 in`.
- Otherwise they are **multiplied**: `6 (3)` → `18`, `2 pi` → `6.2832`.

## Numbers

- Prefixes: `0x` hex, `0o` octal, `0b` binary
- Grouping: `5 300`, `1,000,000`, `1_000`
- Exponents: `1.5e3`
- Scales: `k`/`thousand`, `M`/`million`, `billion`/`bn`, `trillion`.
  `$2k` → `$2,000`, `2M eur` → `€2,000,000`.
  `m` is meters and `K` is kelvin, so the one-letter scales are case-sensitive.
- Output formats: `in hex`, `in oct`, `in bin`, `in sci` (or `scientific`), `in dec`
- Constants: `pi` (also `π`), `e`

## Conversion

Use `in`, `into`, `to` or `as`. Conversions can be chained.

```text
5 km in miles
$30 in Euro
60 mph in km/h
1 acre in sq m
0.25 in %
```

## Units

| Kind | Units |
|---|---|
| Length | meter (all SI prefixes: mm, cm, km, …), mil, point/pt, line, inch, hand, foot/ft, yard/yd, rod, chain, furlong, mile/mi, cable, nautical mile/nmi, league, light year |
| CSS | `px`, `pt`, `em`. `em` defaults to 16 px and `ppi` to 96. Change them with `em = 20px` and `ppi = 326`. |
| Area | hectare/ha, are, acre, plus any squared length: `sq cm`, `square inches`, `sqm`, `m2`, `m²` |
| Volume | liter/L (ml, cl, dl, …), pint, quart/qt, gallon/gal, teaspoon/tsp (`tea spoon`), tablespoon/tbsp, cup, plus any cubed length: `cu cm`, `cubic inches`, `cbm`, `m³` |
| Mass | gram (all SI prefixes: mg, kg, …), tonne/t, carat/ct, centner, pound/lb, stone/st, ounce/oz |
| Time | second (ms, µs, ns), minute/min, hour/h/hr, day, week, month, year. A year is 365 days and a month is 1/12 of a year. |
| Temperature | kelvin/K, celsius/°C/C, fahrenheit/°F/F |
| Data | bit/b and byte/B with SI prefixes (kB = 1000 B) and binary prefixes (KiB = 1024 B). Lowercase `b` is bits and uppercase `B` is bytes. |
| Angle | radian/rad, degree/deg/° |
| Speed | mph, kph (km/h), knot/kn, or any compound such as `km/h` or `m/s` |

Units multiply and divide: `2 m * 3 m` → `6 m²`, `10 km / 2 h` → `5 km/h`,
`$120 / 4 h` → `30 USD/h`. SI prefixes are case-sensitive: `MB` is megabytes,
`mb` is not recognised.

## Currency

Use ISO codes (`USD`, `EUR`, `JPY`, …), names (`dollars`, `euros`, `yen`,
`rupees`, `bitcoin`) or signs (`$ € £ ¥ ₹ ₽ ₩ ₺ ₪ ฿ ₫ ₱ ₴ ₦ ₸ ₾ ₿`).
`$30 CAD` means 30 Canadian dollars. In a sum of mixed currencies, the result
is in the first value's currency: `$30 CAD + 5 USD - 7 EUR` → `25.64 CAD`.

The app refreshes rates when they are more than 12 hours old. The CLI
refreshes them with `reckon --update-rates`. Until the first refresh,
built-in approximate rates are used. Rates are cached in the data folder (see
[Files](#files)).

## Percentages

| Operation | Example | Result |
|---|---|---|
| Add or subtract a percentage | `$10 - 40%` | `$6` |
| Percentage of a value | `20% of $10` | `$2` |
| Add a percentage | `5% on $30` | `$31.50` |
| Subtract a percentage | `6% off 40 EUR` | `€37.60` |
| One value as a % of another | `$50 as a % of $100` | `50%` |
| Increase as a % | `$70 as a % on $20` | `250%` |
| Decrease as a % | `$20 as a % off $70` | `71.4286%` |
| Value from its percentage | `5% of what is 6 EUR` | `€120` |
| Value before an increase | `5% on what is 6 EUR` | `€5.71` |
| Value before a decrease | `5% off what is 6 EUR` | `€6.32` |

## Variables, previous result, sum and average

```text
v = $20
v2 = 5%
v times 7 - v2        → $133
prev + $10            → $143
```

`sum` (or `total`) adds every result since the last blank line or header.
`avg` (or `average`, `mean`) averages them. Lines that are themselves
`sum`/`avg` are not counted again.

Variable names start with a letter and contain letters, digits or `_`. A
variable shadows a unit or function with the same name.

## Functions

Arguments go in parentheses separated by `;` (or `,`), or follow the
function name directly.

| Function | Example |
|---|---|
| `sqrt`, `cbrt` | `sqrt 16`, `sqrt(16 m^2)` → `4 m` |
| `root n (x)` | `root 2 (8)` |
| `log x` (base 10), `log b (x)` | `log 1000`, `log 2 (8)` |
| `ln`, `exp` | `ln 3` |
| `abs`, `round`, `ceil`, `floor` | `round(3.456; 2)` → `3.46` (keeps units) |
| `fact` / `factorial` | `fact 5` |
| `sin`, `cos`, `tan` | `sin 45°`. Plain numbers are radians. |
| `arcsin`, `arccos`, `arctan`, `sinh`, `cosh`, `tanh` | `arctan 1` |
| `fromunix` | `fromunix(1446587186)` |

Plugins can add more; see [plugins.md](plugins.md).

## Dates and times

```text
today, tomorrow, yesterday, now, time
today + 2 weeks           calendar-aware: Jan 31 + 1 month → Feb 28
3 days ago
2 hours from now
March 5 2027 - today      → 118 days
2026-12-25 - today
5 March 2025
2:30 pm - 9:15 am         → 5.25 h
fromunix(1446587186)
```

Adding whole days, weeks, months or years moves the calendar date, so it
handles daylight saving time and month lengths. Fractional amounts are
converted to seconds.

## Time zones

```text
PST time
New York time
time in Madrid
now in Tokyo
2:30 pm HKT in Berlin
3pm UTC+2
```

Zones can be city or country names (about 180 built in), abbreviations
(`PST`, `CET`, `IST`, `JST`, …; uppercase only) or IANA names
(`Europe/Berlin`). On macOS and Linux, offsets and daylight saving time come
from the system zoneinfo database. Abbreviations follow the region's current
clock, so `PST` gives Pacific time all year. On Windows, named zones use fixed
standard offsets.

Subtracting two "current time" values gives the wall-clock difference:
`Tokyo time - London time` → `9 h`.

## Desktop app shortcuts

On Windows and Linux, use Ctrl instead of ⌘.

| Action | Shortcut |
|---|---|
| Copy result of current line | ⇧⌘C (or click a result) |
| Copy all lines with results | ⌥⌘C |
| Wrap line or selection in parentheses | ⇧⌘0 |
| Delete all | ⌥⌘⌫ |
| Open (import) a file | ⌘O |
| Export to a file | ⌘S |
| Undo / redo | ⌘Z / ⇧⌘Z |
| Choose a theme | ⌘K then ⌘T (or click the theme name in the status bar) |

The document saves automatically. Opening a file copies its text into the
document but never modifies the original; use Export to write a file.

## Themes

Every Reckon app (macOS, Windows, Linux, iOS and Android) has the same
twelve themes, each built around one idea:

| Light | | Dark | |
|---|---|---|---|
| **Paper** (default) | Warm graph paper and fountain-pen ink | **Graphite** (default) | Soft pencil on charcoal, mint answers |
| **Ledger** | Bookkeeper's green paper with red rules | **Chalkboard** | Slate green board, coloured chalk |
| **Dune** | Sand, terracotta and desert sage | **Blueprint** | Drafting-table blue with a white grid |
| **Fjord** | Crisp Nordic air, slate and glacier teal | **Nocturne** | Indigo midnight, violet and aqua |
| **Blossom** | Petal pink with plum ink and jade | **Ember** | Espresso dark with glowing amber |
| **Delft** | Porcelain white glazed in cobalt | **Phosphor** | Green-screen terminal, scanlines and a block cursor |

Some themes add a texture (Paper and Blueprint draw a grid, Ledger rules
each line, Phosphor has scanlines), a double rule before the results, or a
heavier result weight. In every theme, text has at least 7:1 contrast with
the background and results and highlights at least 4.5:1; a test checks
this.

On the desktop, press ⌘K then ⌘T (Ctrl+K then Ctrl+T on Windows and
Linux), click the theme name at the right of the status bar, or use
View › Theme on macOS. In the picker, hovering over a theme or pressing ↑
and ↓ previews it; clicking or Enter keeps it, and Esc (or clicking
outside) goes back. The current theme has a ✓, in the picker and in the
menu.

On iOS and Android, tap the theme name at the right of the status bar. The
list opens over the lower part of the screen without dimming the document,
so tapping a theme shows it straight away; the ✓ marks the current one.
Phosphor's block cursor is drawn on the desktop and Android; iOS draws its
own caret, so there it is a green bar.

**Match System** switches between a light and a dark theme with the
system's appearance. It uses the last light theme and the last dark theme
you picked (Paper and Graphite at first). Picking any other theme uses that
theme regardless of the system's appearance.

On the desktop the choice is saved in `app.conf` in the data folder (the
phone apps keep it in their own settings):

```text
theme_mode = system        # or "fixed"
theme = Blueprint          # used when theme_mode = fixed
light_theme = Ledger       # used when theme_mode = system
dark_theme = Nocturne
```

## Files

| Platform | Data folder |
|---|---|
| macOS | `~/Library/Application Support/Reckon` |
| Windows | `%APPDATA%\Reckon` |
| Linux | `$XDG_CONFIG_HOME/reckon` or `~/.config/reckon` |

The folder contains `document.reckon` (the app's document), `app.conf`
(the theme choice), `rates.txt` (cached exchange rates) and `plugins/`. Set `RECKON_HOME` to use a
different folder.

## CLI

```text
reckon '20 inches in cm'
reckon -a -f budget.reckon      print each line next to its result
cat notes.reckon | reckon       one result per input line
reckon                          interactive REPL
reckon --update-rates           download current exchange rates
reckon --rates | --plugins      show rate source / loaded plugins
```

Run `reckon --help` for all options (`--precision`, `--24h`, …).
