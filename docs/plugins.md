# Plugins

A plugin is a plain-text file with the `.reckon` extension in the `plugins`
folder inside the data folder:

| Platform | Plugin folder |
|---|---|
| macOS | `~/Library/Application Support/Reckon/plugins` |
| Windows | `%APPDATA%\Reckon\plugins` |
| Linux | `~/.config/reckon/plugins` |

Plugins load when the app or CLI starts, in file-name order. To list what
loaded and any errors, run:

```bash
reckon --plugins
```

The app also shows the first plugin error in its status bar.

Plugins use Reckon's own expression language rather than JavaScript. That
keeps the engine free of an embedded script runtime, and anything you can
type in a document works in a plugin.

## Syntax

Each non-empty line is one definition. Lines starting with `#` or `//` are
comments.

### Units

```text
unit <id> "<phrase>, <phrase>, ..." = <value with a unit>
```

```text
unit floz  "fl oz, fluid ounce, fluid ounces" = 29.5735 ml
unit horse "horse, horses" = 2.4 m
unit grand "grand, grands" = 1000 USD
unit dozen "dozen, dozens" = 12
```

- `id` must be unique. Using a built-in id such as `m` is an error.
- The first phrase is used to display results.
- Phrases match exactly first, then case-insensitively. A phrase that
  already belongs to another unit is skipped.
- The value fixes the ratio to the unit on the right. Conversion works
  through base units, so `3 horses in ft` works.
- Units must be ratio-based. They cannot be defined from temperatures
  (offset units) or from `px`/`em` (which depend on `ppi`).
- A unit defined from a currency uses the exchange rate at load time.

### Variables

```text
var <name> = <expression>
```

```text
var tax = 8.25%
var rent = $1,450
```

`let` works too. Variables are visible in every document. A variable defined
in the document itself takes priority.

### Functions

```text
fn <name>(<param>; <param>; ...) = <expression>
```

```text
fn hypot(a; b) = sqrt(a^2 + b^2)
fn compound(principal; rate; years) = principal * (1 + rate) ^ years
fn tip(bill) = bill * 18%
```

Call them like built-ins: `hypot(3; 4)`, `compound($1000; 5%; 10)`. Note
that inside an arithmetic expression, `5%` used with `+` adds a percentage of
the left side (`1 + 5%` is `1.05`), so `(1 + rate)` above works as expected.

Parameters are evaluated before the call, keep their units, and are visible
only in the function body. Functions can call other functions; nesting is
limited to 32 levels.

## Examples

See [`examples/plugins`](../examples/plugins). To install one:

```bash
mkdir -p ~/Library/Application\ Support/Reckon/plugins
```

```bash
cp examples/plugins/*.reckon ~/Library/Application\ Support/Reckon/plugins/
```
