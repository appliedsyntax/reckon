//! Tables of data into sheet lines: a CSV or tab-separated file, cells
//! copied from a spreadsheet, or an Excel sheet (see `xlsx.rs`) become one
//! list per column of numbers, such as `reference = [5.2, 6.1, 4.8]`.

/// Cells, row by row, as text.
pub type Rows = Vec<Vec<String>>;

/// Most rows a table may have: one list holds at most 10,000 numbers.
pub const MAX_ROWS: usize = 10_000;
/// Most columns read from a table.
pub const MAX_COLUMNS: usize = 200;

/// Lines for a sheet, made from a table.
#[derive(Debug, PartialEq)]
pub struct Imported {
    /// A heading, a note and one line per column of numbers.
    pub text: String,
    /// Names given to the lists, in order.
    pub names: Vec<String>,
    /// Most values in one list.
    pub rows: usize,
}

/// Splits delimited text into rows of cells. The delimiter is a tab when
/// the text has one, else a semicolon when the first line has more of them
/// than commas, else a comma. Quoting follows RFC 4180: `"a, b"` is one
/// cell and `""` inside quotes is a quote.
pub fn parse_delimited(text: &str) -> Rows {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.lines().next().unwrap_or("");
    let delim = if text.contains('\t') {
        '\t'
    } else if first.matches(';').count() > first.matches(',').count() {
        ';'
    } else {
        ','
    };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = false,
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                if rows.len() > MAX_ROWS + 10 {
                    break;
                }
            }
            c if c == delim => row.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// Whether pasted text is cells copied from a spreadsheet: two or more
/// rows of tab-separated cells, all the same width, with a column of
/// numbers. Other text is pasted as it is.
pub fn looks_like_table(text: &str) -> bool {
    if !text.contains('\t') {
        return false;
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return false;
    }
    let width = lines[0].split('\t').count();
    if width < 2 || lines.iter().any(|l| l.split('\t').count() != width) {
        return false;
    }
    let rows = parse_delimited(text);
    columns(&rows).iter().any(|c| c.numbers() >= 2)
}

#[derive(Clone, Debug, PartialEq)]
enum Cell {
    Num(f64),
    Blank,
    Text,
}

struct Column {
    header: Option<String>,
    cells: Vec<Cell>,
    percent: bool,
    /// Why the column is not a list of numbers.
    unusable: Option<&'static str>,
}

impl Column {
    fn numbers(&self) -> usize {
        if self.unusable.is_some() {
            return 0;
        }
        self.cells
            .iter()
            .filter(|c| matches!(c, Cell::Num(_)))
            .count()
    }
}

/// Words that mean "no value here".
fn is_missing(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "" | "na" | "n/a" | "#n/a" | "nan" | "null" | "none" | "-" | "—" | "." | "?"
    )
}

/// Reads a cell as a number. `comma_decimal` says whether a comma is the
/// decimal mark (`1,5`) or separates thousands (`1,500`).
fn number(s: &str, comma_decimal: bool) -> Option<(f64, bool)> {
    let s = s.trim();
    let (s, percent) = match s.strip_suffix('%') {
        Some(rest) => (rest.trim_end(), true),
        None => (s, false),
    };
    let mut t: String = s
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\''))
        .map(|c| if c == '−' { '-' } else { c })
        .collect();
    if comma_decimal {
        t = t.replace('.', "").replace(',', ".");
    } else {
        // Commas only as thousands separators: 1,500 or 12,345.6.
        let int = t.split(['.', 'e', 'E']).next().unwrap_or("");
        let groups: Vec<&str> = int.trim_start_matches(['-', '+']).split(',').collect();
        if groups.len() > 1
            && (groups[0].is_empty()
                || groups[0].len() > 3
                || groups[1..].iter().any(|g| g.len() != 3))
        {
            return None;
        }
        t = t.replace(',', "");
    }
    if !t.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.')) {
        return None;
    }
    let v: f64 = t.parse().ok()?;
    v.is_finite().then_some((v, percent))
}

/// Whether a column writes its decimals with a comma. Any cell such as
/// `1,5` or `0,25` (a comma not followed by exactly three digits) says yes;
/// `1,234.5` says no; with neither, `1,500` is read as fifteen hundred.
fn comma_decimal(cells: &[&str]) -> bool {
    for c in cells {
        let c = c.trim().trim_end_matches('%').trim_end();
        if let Some((_, after)) = c.rsplit_once(',') {
            if c.contains('.') {
                return c.rfind(',') > c.rfind('.');
            }
            if after.len() != 3 && after.chars().all(|d| d.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

fn columns(rows: &Rows) -> Vec<Column> {
    let mut rows: Vec<&Vec<String>> = rows
        .iter()
        .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
        .collect();
    let width = rows
        .iter()
        .map(|r| r.len())
        .max()
        .unwrap_or(0)
        .min(MAX_COLUMNS);
    let filled = |r: &Vec<String>| r.iter().filter(|c| !c.trim().is_empty()).count();
    // A title above the headings: one filled cell, then a wider row.
    while rows.len() > 2 && width > 1 && filled(rows[0]) == 1 && filled(rows[1]) > 1 {
        rows.remove(0);
    }
    let cell = |r: &Vec<String>, i: usize| r.get(i).map_or("", |c| c.as_str()).trim().to_string();
    let text_cell = |s: &str, comma: bool| !is_missing(s) && number(s, comma).is_none();
    // The first row holds headings when any of its cells is not a number.
    let header = rows.first().is_some_and(|r| {
        (0..width).any(|i| {
            let s = cell(r, i);
            let col: Vec<String> = rows.iter().map(|r| cell(r, i)).collect();
            let refs: Vec<&str> = col.iter().map(|s| s.as_str()).collect();
            text_cell(&s, comma_decimal(&refs))
        })
    });
    let body = &rows[usize::from(header)..];
    (0..width)
        .map(|i| {
            let raw: Vec<String> = body.iter().map(|r| cell(r, i)).collect();
            let refs: Vec<&str> = raw.iter().map(|s| s.as_str()).collect();
            let comma = comma_decimal(&refs);
            let mut percents = (0, 0);
            let cells: Vec<Cell> = raw
                .iter()
                .map(|s| {
                    if is_missing(s) {
                        return Cell::Blank;
                    }
                    match number(s, comma) {
                        Some((v, p)) => {
                            if p {
                                percents.0 += 1;
                            } else {
                                percents.1 += 1;
                            }
                            Cell::Num(v)
                        }
                        None => Cell::Text,
                    }
                })
                .collect();
            let unusable = if cells.contains(&Cell::Text) {
                Some("not numbers")
            } else if percents.0 > 0 && percents.1 > 0 {
                Some("mixes percentages and numbers")
            } else if percents.0 + percents.1 == 0 {
                Some("empty")
            } else {
                None
            };
            Column {
                header: header.then(|| cell(rows[0], i)).filter(|h| !h.is_empty()),
                cells,
                percent: percents.0 > 0,
                unusable,
            }
        })
        .collect()
}

/// A name for a list made from a column heading: `Reference (mg/dL)` gives
/// `reference_mg_dl`.
fn name_from(header: &str) -> String {
    let mut name = String::new();
    for c in header.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            name.push(c);
        } else if !name.ends_with('_') && !name.is_empty() {
            name.push('_');
        }
    }
    let mut name: String = name.trim_end_matches('_').chars().take(32).collect();
    name = name.trim_end_matches('_').to_string();
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert_str(0, "col_");
    }
    name
}

/// Writes a number the way a sheet reads it, to the 15 significant digits
/// a spreadsheet shows, so stored noise such as 0.30000000000000004 goes.
fn write_number(v: f64) -> String {
    let v: f64 = format!("{v:.14e}").parse().unwrap_or(v);
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}")
}

/// Turns rows of cells into sheet lines: a heading naming `source`, a note
/// on what was read, and one list per column of numbers. Columns that are
/// not numbers are left out, and said so. Rows are pairs (one sample per
/// row), so a row missing any of its numbers is left out of every list.
/// `free` says whether a name can be used for a list.
pub fn to_sheet(
    rows: &Rows,
    source: &str,
    free: &dyn Fn(&str) -> bool,
) -> Result<Imported, String> {
    let cols = columns(rows);
    if cols.first().map_or(0, |c| c.cells.len()) > MAX_ROWS {
        return Err("the table has more than 10,000 rows".into());
    }
    let numeric: Vec<usize> = (0..cols.len()).filter(|i| cols[*i].numbers() > 0).collect();
    if numeric.is_empty() {
        return Err("no column holds numbers".into());
    }
    // Each row is one sample measured in each column, so a row missing any
    // of its numbers is left out of every list, and the lists stay paired.
    let dropped = (0..cols[numeric[0]].cells.len())
        .filter(|r| {
            numeric.iter().any(|i| cols[*i].cells[*r] == Cell::Blank)
                && numeric.iter().any(|i| cols[*i].cells[*r] != Cell::Blank)
        })
        .count();
    let keep = |r: usize| {
        numeric
            .iter()
            .all(|i| matches!(cols[*i].cells[r], Cell::Num(_)))
    };

    let mut names: Vec<String> = Vec::new();
    let mut rows_kept = 0;
    let mut lines = Vec::new();
    for (k, i) in numeric.iter().enumerate() {
        let c = &cols[*i];
        let mut name = c.header.as_deref().map(name_from).unwrap_or_default();
        if name.is_empty() {
            name = format!("col{}", k + 1);
        }
        if !free(&name) {
            name = format!("col_{name}");
        }
        let base = name.clone();
        let mut n = 2;
        while names.contains(&name) || !free(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        let values: Vec<String> = c
            .cells
            .iter()
            .enumerate()
            .filter(|(r, _)| keep(*r))
            .filter_map(|(_, v)| match v {
                Cell::Num(x) => Some(write_number(*x) + if c.percent { "%" } else { "" }),
                _ => None,
            })
            .collect();
        rows_kept = values.len();
        lines.push(format!("{name} = [{}]", values.join(", ")));
        names.push(name);
    }

    let mut note = format!(
        "// {rows_kept} {}",
        if rows_kept == 1 { "row" } else { "rows" }
    );
    let mut left_out: Vec<String> = cols
        .iter()
        .enumerate()
        .filter(|(i, c)| !numeric.contains(i) && c.unusable != Some("empty"))
        .map(|(i, c)| {
            let label = c
                .header
                .clone()
                .unwrap_or_else(|| format!("column {}", i + 1));
            format!("“{label}” ({})", c.unusable.unwrap_or("not numbers"))
        })
        .collect();
    if dropped > 0 {
        left_out.push(format!(
            "{dropped} {} with empty cells",
            if dropped == 1 { "row" } else { "rows" }
        ));
    }
    if left_out.is_empty() {
        note.push('.');
    } else {
        note.push_str(". Left out: ");
        note.push_str(&left_out.join(", "));
        note.push('.');
    }
    let title = source.replace('\n', " ");
    Ok(Imported {
        text: format!("# Data from {title}\n{note}\n{}", lines.join("\n")),
        names,
        rows: rows_kept,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(text: &str) -> String {
        to_sheet(&parse_delimited(text), "test.csv", &|n| n != "taken")
            .map(|i| i.text)
            .unwrap_or_else(|e| format!("error: {e}"))
    }

    #[test]
    fn delimiters_and_quotes() {
        assert_eq!(
            parse_delimited("a,b\n1,\"2, 3\"\n\"say \"\"hi\"\"\",4\r\n"),
            vec![
                vec!["a".to_string(), "b".into()],
                vec!["1".into(), "2, 3".into()],
                vec!["say \"hi\"".into(), "4".into()],
            ]
        );
        assert_eq!(parse_delimited("a;b\n1,5;2,5")[1], vec!["1,5", "2,5"]);
        assert_eq!(parse_delimited("a\tb\n1,5\t2")[1], vec!["1,5", "2"]);
        assert_eq!(parse_delimited("\u{feff}x\n1")[0], vec!["x"]);
    }

    #[test]
    fn columns_of_numbers_become_lists() {
        assert_eq!(
            import("Sample,Reference (mg/dL),New method\nA1,5.2,5.4\nA2,6.1,6\nA3,4.8,5.1\n"),
            "# Data from test.csv\n\
             // 3 rows. Left out: “Sample” (not numbers).\n\
             reference_mg_dl = [5.2, 6.1, 4.8]\n\
             new_method = [5.4, 6, 5.1]"
        );
        // No headings.
        assert_eq!(
            import("1\t2\n3\t4\n"),
            "# Data from test.csv\n// 2 rows.\ncol1 = [1, 3]\ncol2 = [2, 4]"
        );
    }

    #[test]
    fn numbers_in_many_spellings() {
        assert_eq!(
            import("a;b\n1,5;10\n2,25;-3\n"),
            "# Data from test.csv\n// 2 rows.\na = [1.5, 2.25]\nb = [10, -3]"
        );
        assert_eq!(
            import("a,b\n\"1,500\",12%\n\"12,345.6\",7.5%\n−2,1e3%\n"),
            "# Data from test.csv\n// 3 rows.\na = [1500, 12345.6, -2]\nb = [12%, 7.5%, 1000%]"
        );
        // Spreadsheet noise is rounded away.
        assert_eq!(write_number(0.1 + 0.2), "0.3");
        assert_eq!(write_number(-0.0), "0");
        assert_eq!(write_number(1e-7), "0.0000001");
    }

    #[test]
    fn missing_cells() {
        // Rows are pairs, so a row missing a number goes from every list.
        assert_eq!(
            import("x,y\n1,2\n3,\n5,6\n7,NA\n9,10\n11\n"),
            "# Data from test.csv\n// 3 rows. Left out: 3 rows with empty cells.\n\
             x = [1, 5, 9]\ny = [2, 6, 10]"
        );
        // Empty rows and a title above the headings are skipped.
        assert_eq!(
            import("Method study,,\n\nx,y,z\n1,2,\n\n3,4,\n"),
            "# Data from test.csv\n// 2 rows.\nx = [1, 3]\ny = [2, 4]"
        );
    }

    #[test]
    fn names() {
        assert_eq!(name_from("Reference (mg/dL)"), "reference_mg_dl");
        assert_eq!(name_from("  2nd run "), "col_2nd_run");
        assert_eq!(name_from("Größe"), "größe");
        assert_eq!(name_from("%%"), "");
        assert_eq!(
            import("taken,a,a\n1,2,3\n"),
            "# Data from test.csv\n// 1 row.\ncol_taken = [1]\na = [2]\na_2 = [3]"
        );
    }

    #[test]
    fn what_is_not_a_table() {
        assert_eq!(import("a,b\nx,y\n"), "error: no column holds numbers");
        assert_eq!(import(""), "error: no column holds numbers");
        assert_eq!(
            import("a,b\n10%,2\n3,4\n"),
            "# Data from test.csv\n// 2 rows. Left out: “a” (mixes percentages and numbers).\n\
             b = [2, 4]"
        );
        let big = format!("x\n{}", "1\n".repeat(MAX_ROWS + 1));
        assert_eq!(import(&big), "error: the table has more than 10,000 rows");
        assert!(looks_like_table("Ref\tNew\n1.2\t1.3\n1.4\t1.5\n"));
        assert!(!looks_like_table("1.2\n1.3\n"));
        assert!(!looks_like_table("a\tb\nc\td\n"));
        assert!(!looks_like_table("x = 1\ty\nnext"));
        assert!(!looks_like_table("1\t2\n3\n"));
    }
}
