//! Reads the first worksheet of an Excel workbook (`.xlsx`) as rows of cell
//! text, for [`crate::table`]. Only what that needs: cell values, not
//! formatting. A workbook is a zip archive of XML files.
//!
//! Workbooks come from anywhere, so every size is bounded: the file, each
//! part once decompressed, and the number of rows and columns kept.

use std::collections::HashMap;

use crate::table::{Rows, MAX_COLUMNS, MAX_ROWS};

/// Largest workbook read.
pub const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
/// Largest part of a workbook once decompressed.
const MAX_PART_BYTES: usize = 200 * 1024 * 1024;

/// The first worksheet of a workbook.
#[derive(Debug, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub rows: Rows,
    /// Worksheets in the workbook, this one included.
    pub sheets: usize,
}

pub fn read(bytes: &[u8]) -> Result<Sheet, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("the workbook is larger than 50 MB".into());
    }
    if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
        return Err("this is an older Excel file (.xls); save it as .xlsx or .csv".into());
    }
    let zip = Zip::new(bytes).ok_or("this is not an Excel workbook")?;
    let text = |name: &str| -> Result<String, String> {
        let data = zip
            .read(name)?
            .ok_or_else(|| format!("the workbook has no {name}"))?;
        String::from_utf8(data).map_err(|_| format!("{name} is not text"))
    };
    let workbook = text("xl/workbook.xml")?;
    let rels = text("xl/_rels/workbook.xml.rels")?;
    let (name, target, sheets) = first_sheet(&workbook, &rels)?;
    let path = match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => format!("xl/{target}"),
    };
    let shared = match zip.read("xl/sharedStrings.xml")? {
        Some(data) => shared_strings(&String::from_utf8(data).map_err(|_| "bad shared strings")?)?,
        None => Vec::new(),
    };
    let rows = cells(&text(&path)?, &shared)?;
    Ok(Sheet { name, rows, sheets })
}

fn parse(xml: &str) -> Result<roxmltree::Document<'_>, String> {
    let opt = roxmltree::ParsingOptions {
        allow_dtd: false,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(xml, opt).map_err(|_| "the workbook is damaged".into())
}

/// The first worksheet's name and path, and how many there are.
fn first_sheet(workbook: &str, rels: &str) -> Result<(String, String, usize), String> {
    let wb = parse(workbook)?;
    let sheets: Vec<roxmltree::Node> = wb
        .descendants()
        .filter(|n| n.has_tag_name("sheet") && n.parent().is_some_and(|p| p.has_tag_name("sheets")))
        .collect();
    let first = sheets.first().ok_or("the workbook has no worksheet")?;
    let name = first.attribute("name").unwrap_or("Sheet1").to_string();
    let id = first
        .attributes()
        .find(|a| a.name() == "id")
        .map(|a| a.value())
        .ok_or("the workbook is damaged")?;
    let rels = parse(rels)?;
    let target = rels
        .descendants()
        .find(|n| n.has_tag_name("Relationship") && n.attribute("Id") == Some(id))
        .and_then(|n| n.attribute("Target"))
        .ok_or("the workbook is damaged")?;
    Ok((name, target.to_string(), sheets.len()))
}

/// Text of a string item: its `<t>` elements, leaving out phonetic runs.
fn item_text(si: roxmltree::Node) -> String {
    si.descendants()
        .filter(|n| n.has_tag_name("t") && !n.ancestors().any(|a| a.has_tag_name("rPh")))
        .filter_map(|n| n.text())
        .collect()
}

fn shared_strings(xml: &str) -> Result<Vec<String>, String> {
    let doc = parse(xml)?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("si"))
        .map(item_text)
        .collect())
}

/// Column index from a cell reference: `C7` is 2.
fn column_of(r: &str) -> Option<usize> {
    let letters: String = r.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    Some(
        letters.chars().fold(0usize, |a, c| {
            a * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)
        }) - 1,
    )
}

fn row_of(r: &str) -> Option<usize> {
    r.trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
}

fn cells(xml: &str, shared: &[String]) -> Result<Rows, String> {
    let doc = parse(xml)?;
    let mut rows: Rows = Vec::new();
    let data = doc
        .descendants()
        .find(|n| n.has_tag_name("sheetData"))
        .ok_or("the worksheet is damaged")?;
    for (k, row) in data
        .children()
        .filter(|n| n.has_tag_name("row"))
        .enumerate()
    {
        let r = row
            .attribute("r")
            .and_then(|r| r.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .unwrap_or(k.max(rows.len()));
        // Leave room for a title and headings above the data.
        if r > MAX_ROWS + 10 {
            break;
        }
        for (j, c) in row.children().filter(|n| n.has_tag_name("c")).enumerate() {
            let reference = c.attribute("r");
            let col = reference.and_then(column_of).unwrap_or(j);
            let at = reference.and_then(row_of).unwrap_or(r);
            if col >= MAX_COLUMNS || at != r {
                continue;
            }
            let v = c
                .children()
                .find(|n| n.has_tag_name("v"))
                .and_then(|n| n.text())
                .unwrap_or("");
            let value = match c.attribute("t") {
                Some("s") => v
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| shared.get(i))
                    .cloned()
                    .unwrap_or_default(),
                Some("inlineStr") => c
                    .children()
                    .find(|n| n.has_tag_name("is"))
                    .map(item_text)
                    .unwrap_or_default(),
                // Booleans and errors (#N/A, #DIV/0!) are not data.
                Some("b") | Some("e") => String::new(),
                _ => v.trim().to_string(),
            };
            if value.is_empty() {
                continue;
            }
            if rows.len() <= r {
                rows.resize(r + 1, Vec::new());
            }
            let cells = &mut rows[r];
            if cells.len() <= col {
                cells.resize(col + 1, String::new());
            }
            cells[col] = value;
        }
    }
    Ok(rows)
}

/// Just enough of the zip format to read named parts: the central
/// directory, stored and deflated entries. No zip64, no encryption.
struct Zip<'a> {
    data: &'a [u8],
    /// Name → (method, compressed size, uncompressed size, local header).
    entries: HashMap<String, (u16, usize, usize, usize)>,
}

fn u16_at(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(i..i + 4)?.try_into().ok()?) as usize)
}

impl<'a> Zip<'a> {
    fn new(data: &'a [u8]) -> Option<Zip<'a>> {
        // The end-of-directory record is in the last 64 KB + 22 bytes.
        let from = data.len().saturating_sub(65_557);
        let eocd = (from..data.len().saturating_sub(21))
            .rev()
            .find(|i| data[*i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]))?;
        let count = u16_at(data, eocd + 10)? as usize;
        let mut at = u32_at(data, eocd + 16)?;
        let mut entries = HashMap::new();
        for _ in 0..count.min(10_000) {
            if !data.get(at..)?.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
                return None;
            }
            let method = u16_at(data, at + 10)?;
            let packed = u32_at(data, at + 20)?;
            let size = u32_at(data, at + 24)?;
            let name_len = u16_at(data, at + 28)? as usize;
            let extra = u16_at(data, at + 30)? as usize;
            let comment = u16_at(data, at + 32)? as usize;
            let local = u32_at(data, at + 42)?;
            let name = String::from_utf8_lossy(data.get(at + 46..at + 46 + name_len)?).into_owned();
            entries.insert(name, (method, packed, size, local));
            at += 46 + name_len + extra + comment;
        }
        Some(Zip { data, entries })
    }

    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let Some(&(method, packed, size, local)) = self.entries.get(name) else {
            return Ok(None);
        };
        let damaged = || "the workbook is damaged".to_string();
        let d = self.data;
        if !d
            .get(local..)
            .ok_or_else(damaged)?
            .starts_with(&[0x50, 0x4b, 0x03, 0x04])
        {
            return Err(damaged());
        }
        let name_len = u16_at(d, local + 26).ok_or_else(damaged)? as usize;
        let extra = u16_at(d, local + 28).ok_or_else(damaged)? as usize;
        let start = local + 30 + name_len + extra;
        let body = d.get(start..start + packed).ok_or_else(damaged)?;
        if size > MAX_PART_BYTES {
            return Err("the workbook is too large".into());
        }
        match method {
            0 => Ok(Some(body.to_vec())),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(body, MAX_PART_BYTES)
                .map(Some)
                .map_err(|_| damaged()),
            _ => Err("the workbook uses a compression Reckon cannot read".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored (uncompressed) zip of the given parts.
    pub(crate) fn zip(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut dir = Vec::new();
        for (name, body) in parts {
            let local = out.len() as u32;
            let (n, b) = (name.as_bytes(), body.as_bytes());
            out.extend([0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend(0u32.to_le_bytes()); // crc, unchecked
            out.extend((b.len() as u32).to_le_bytes());
            out.extend((b.len() as u32).to_le_bytes());
            out.extend((n.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(n);
            out.extend(b);
            dir.extend([0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            dir.extend(0u32.to_le_bytes());
            dir.extend((b.len() as u32).to_le_bytes());
            dir.extend((b.len() as u32).to_le_bytes());
            dir.extend((n.len() as u16).to_le_bytes());
            dir.extend([0u8; 12]);
            dir.extend(local.to_le_bytes());
            dir.extend(n);
        }
        let at = out.len() as u32;
        out.extend(&dir);
        out.extend([0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend((parts.len() as u16).to_le_bytes());
        out.extend((parts.len() as u16).to_le_bytes());
        out.extend((dir.len() as u32).to_le_bytes());
        out.extend(at.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    const WORKBOOK: &str = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId2"/><sheet name="Notes" sheetId="2" r:id="rId1"/></sheets></workbook>"#;
    const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="worksheet" Target="/xl/worksheets/sheet2.xml"/></Relationships>"#;
    const STRINGS: &str = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>Sample</t></si><si><r><t>Refer</t></r><r><t>ence</t></r><rPh><t>ignored</t></rPh></si></sst>"#;
    const SHEET: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
        <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="D1" t="inlineStr"><is><t>New</t></is></c></row>
        <row r="2"><c r="A2" t="str"><f>"A"&amp;1</f><v>A1</v></c><c r="B2"><v>5.2</v></c><c r="D2"><f>B2*1.1</f><v>5.7200000000000006</v></c></row>
        <row r="4"><c r="A4" t="s"><v>0</v></c><c r="B4"><v>6.1</v></c><c r="C4" t="b"><v>1</v></c><c r="D4" t="e"><v>#N/A</v></c></row>
        </sheetData></worksheet>"#;

    #[test]
    fn reads_the_first_sheet() {
        let file = zip(&[
            ("xl/workbook.xml", WORKBOOK),
            ("xl/_rels/workbook.xml.rels", RELS),
            ("xl/sharedStrings.xml", STRINGS),
            ("xl/worksheets/sheet1.xml", "<worksheet/>"),
            ("xl/worksheets/sheet2.xml", SHEET),
        ]);
        let s = read(&file).unwrap();
        assert_eq!((s.name.as_str(), s.sheets), ("Data", 2));
        let row = |v: &[&str]| v.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        assert_eq!(
            s.rows,
            vec![
                row(&["Sample", "Reference", "", "New"]),
                row(&["A1", "5.2", "", "5.7200000000000006"]),
                vec![],
                row(&["Sample", "6.1"]),
            ]
        );
        let lines = crate::table::to_sheet(&s.rows, "data.xlsx", &|_| true).unwrap();
        assert_eq!(
            lines.text,
            "# Data from data.xlsx\n// 1 row. Left out: “Sample” (not numbers), 1 row with empty cells.\n\
             reference = [5.2]\nnew = [5.72]"
        );
    }

    #[test]
    fn deflated_parts() {
        let body = SHEET.to_string();
        let packed = miniz_oxide::deflate::compress_to_vec(body.as_bytes(), 6);
        // Build a stored zip, then swap in the deflated sheet.
        let mut file = zip(&[
            ("xl/workbook.xml", WORKBOOK),
            ("xl/_rels/workbook.xml.rels", RELS),
            ("xl/worksheets/sheet2.xml", "x"),
        ]);
        let z = Zip::new(&file).unwrap();
        let (_, _, _, local) = z.entries["xl/worksheets/sheet2.xml"];
        let start = local + 30 + "xl/worksheets/sheet2.xml".len();
        let mut rebuilt = file[..start].to_vec();
        rebuilt.extend(&packed);
        let shift = packed.len() - 1;
        let tail_from = start + 1;
        rebuilt.extend(&file[tail_from..]);
        // Patch sizes and method in the local header, the directory and
        // the end record.
        rebuilt[local + 8] = 8;
        rebuilt[local + 18..local + 22].copy_from_slice(&(packed.len() as u32).to_le_bytes());
        rebuilt[local + 22..local + 26].copy_from_slice(&(body.len() as u32).to_le_bytes());
        let eocd = rebuilt.len() - 22;
        let old_dir = u32_at(&rebuilt, eocd + 16).unwrap();
        rebuilt[eocd + 16..eocd + 20].copy_from_slice(&((old_dir + shift) as u32).to_le_bytes());
        let mut at = old_dir + shift;
        while rebuilt[at..].starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            let n = u16_at(&rebuilt, at + 28).unwrap() as usize;
            if &rebuilt[at + 46..at + 46 + n] == b"xl/worksheets/sheet2.xml" {
                rebuilt[at + 10] = 8;
                rebuilt[at + 20..at + 24].copy_from_slice(&(packed.len() as u32).to_le_bytes());
                rebuilt[at + 24..at + 28].copy_from_slice(&(body.len() as u32).to_le_bytes());
            }
            at += 46 + n;
        }
        file = rebuilt;
        let s = read(&file).unwrap();
        assert_eq!(s.rows[1][1], "5.2");
    }

    #[test]
    fn what_is_not_a_workbook() {
        assert_eq!(
            read(b"a,b\n1,2").unwrap_err(),
            "this is not an Excel workbook"
        );
        assert_eq!(
            read(&[0xd0, 0xcf, 0x11, 0xe0, 0, 0]).unwrap_err(),
            "this is an older Excel file (.xls); save it as .xlsx or .csv"
        );
        let no_sheet = zip(&[
            ("xl/workbook.xml", "<workbook/>"),
            ("xl/_rels/workbook.xml.rels", RELS),
        ]);
        assert_eq!(
            read(&no_sheet).unwrap_err(),
            "the workbook has no worksheet"
        );
        // A document type declaration (the way to an entity bomb) is refused.
        let dtd = zip(&[
            (
                "xl/workbook.xml",
                "<!DOCTYPE a [<!ENTITY x \"y\">]><workbook/>",
            ),
            ("xl/_rels/workbook.xml.rels", RELS),
        ]);
        assert_eq!(read(&dtd).unwrap_err(), "the workbook is damaged");
        // Truncated anywhere, it fails cleanly.
        let file = zip(&[
            ("xl/workbook.xml", WORKBOOK),
            ("xl/_rels/workbook.xml.rels", RELS),
            ("xl/worksheets/sheet2.xml", SHEET),
        ]);
        for cut in 0..file.len() {
            let _ = read(&file[..cut]);
        }
        assert_eq!(column_of("AA10"), Some(26));
        assert_eq!(row_of("AA10"), Some(9));
    }
}
