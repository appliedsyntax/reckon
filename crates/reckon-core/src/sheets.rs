//! Sheets: a folder of plain-text `.reckon` files, one per document.
//!
//! Every app (desktop, iOS, Android) stores sheets the same way, so a folder
//! kept in sync by iCloud Drive, Dropbox, OneDrive or Syncthing shows the
//! same sheets everywhere. There is no database and no index: the folder
//! listing is the source of truth.
//!
//! A sheet's title is its first non-empty line (without a leading `#` or
//! `//`). Files are named after the title when they are created or
//! [tidied](tidy), never while the user is typing, so sync tools do not see
//! a storm of renames.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const EXTENSION: &str = "reckon";
/// Deleted sheets are moved here (inside the sheets folder) so they can be
/// restored.
pub const TRASH: &str = ".trash";
const MAX_NAME: usize = 60;

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    /// File name inside the folder, e.g. `Trip budget.reckon`.
    pub file: String,
    pub title: String,
    /// The first line after the title, for list previews.
    pub preview: String,
    /// Last modification, in seconds since the Unix epoch.
    pub modified: u64,
}

/// The display title of a sheet: its first non-empty line without heading
/// or comment markers, or "Untitled".
pub fn title_of(text: &str) -> String {
    let line = text.lines().map(clean).find(|l| !l.is_empty());
    match line {
        Some(l) => truncate(&l, MAX_NAME),
        None => "Untitled".to_string(),
    }
}

fn preview_of(text: &str) -> String {
    let mut lines = text.lines().map(clean).filter(|l| !l.is_empty());
    lines.next();
    lines.next().map(|l| truncate(&l, 80)).unwrap_or_default()
}

fn clean(line: &str) -> String {
    let l = line.trim();
    let l = l.trim_start_matches('#').trim_start();
    let l = l.strip_prefix("//").unwrap_or(l).trim();
    l.to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

/// A file-system-safe version of `title` (no path separators or characters
/// Windows forbids, no leading dots).
pub fn safe_name(title: &str) -> String {
    let mut s: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    s = s
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .to_string();
    if s.is_empty() {
        s = "Untitled".to_string();
    }
    truncate(&s, MAX_NAME).replace('…', "")
}

fn is_sheet(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(EXTENSION)
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| !n.starts_with('.'))
}

fn modified(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// All sheets in `dir`, most recently modified first.
pub fn list(dir: &Path) -> Vec<Sheet> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut sheets: Vec<Sheet> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_sheet(p))
        .filter_map(|p| {
            let text = fs::read_to_string(&p).ok()?;
            Some(Sheet {
                file: p.file_name()?.to_str()?.to_string(),
                title: title_of(&text),
                preview: preview_of(&text),
                modified: modified(&p),
            })
        })
        .collect();
    sheets.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.title.cmp(&b.title)));
    sheets
}

/// Sheets whose title or text contains `query` (case-insensitive).
pub fn search(dir: &Path, sheets: &[Sheet], query: &str) -> Vec<Sheet> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return sheets.to_vec();
    }
    sheets
        .iter()
        .filter(|s| {
            s.title.to_lowercase().contains(&q)
                || fs::read_to_string(dir.join(&s.file))
                    .map(|t| t.to_lowercase().contains(&q))
                    .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// A file name for `title` that is not taken in `dir` (`keep` may be the
/// sheet's own current name).
fn free_name(dir: &Path, title: &str, keep: Option<&str>) -> String {
    let base = safe_name(title);
    for n in 1.. {
        let name = if n == 1 {
            format!("{base}.{EXTENSION}")
        } else {
            format!("{base} {n}.{EXTENSION}")
        };
        if Some(name.as_str()) == keep || !dir.join(&name).exists() {
            return name;
        }
    }
    unreachable!()
}

pub fn read(dir: &Path, file: &str) -> io::Result<String> {
    fs::read_to_string(dir.join(file))
}

/// Writes a sheet through a temporary file and a rename, so a crash or a
/// sync tool never sees a half-written file.
pub fn write(dir: &Path, file: &str, text: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{file}.tmp"));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, dir.join(file))
}

/// Creates a sheet holding `text`, named after its title. Returns the file
/// name.
pub fn create(dir: &Path, text: &str) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    let name = free_name(dir, &title_of(text), None);
    write(dir, &name, text)?;
    Ok(name)
}

/// Renames `file` to match its current title, if they differ. Returns the
/// (possibly new) file name.
pub fn tidy(dir: &Path, file: &str) -> io::Result<String> {
    let text = read(dir, file)?;
    let wanted = free_name(dir, &title_of(&text), Some(file));
    if wanted != file {
        fs::rename(dir.join(file), dir.join(&wanted))?;
    }
    Ok(wanted)
}

/// Moves a sheet into the folder's `.trash`. Returns its path there, for
/// [`restore`].
pub fn trash(dir: &Path, file: &str) -> io::Result<PathBuf> {
    let bin = dir.join(TRASH);
    fs::create_dir_all(&bin)?;
    let mut dest = bin.join(file);
    let mut n = 2;
    while dest.exists() {
        dest = bin.join(format!(
            "{} {n}.{EXTENSION}",
            file.trim_end_matches(".reckon")
        ));
        n += 1;
    }
    fs::rename(dir.join(file), &dest)?;
    Ok(dest)
}

/// Moves a trashed sheet back. Returns its file name.
pub fn restore(dir: &Path, trashed: &Path) -> io::Result<String> {
    let text = fs::read_to_string(trashed)?;
    let name = free_name(dir, &title_of(&text), None);
    fs::rename(trashed, dir.join(&name))?;
    Ok(name)
}

/// Sets up a sheets folder: moves an old single-document file into it, or
/// creates a first sheet from `welcome` when the folder has none. Returns
/// the sheet to open.
pub fn prepare(dir: &Path, legacy: Option<&Path>, welcome: &str) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    if let Some(old) = legacy.filter(|p| p.exists()) {
        let text = fs::read_to_string(old)?;
        let name = create(dir, &text)?;
        fs::remove_file(old)?;
        return Ok(name);
    }
    match list(dir).into_iter().next() {
        Some(s) => Ok(s.file),
        None => create(dir, welcome),
    }
}

/// [`list`] as JSON, for the C interface.
pub fn list_json(dir: &Path) -> String {
    let esc = |s: &str| {
        let mut o = String::new();
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o
    };
    let items: Vec<String> = list(dir)
        .iter()
        .map(|s| {
            format!(
                "{{\"file\":\"{}\",\"title\":\"{}\",\"preview\":\"{}\",\"modified\":{}}}",
                esc(&s.file),
                esc(&s.title),
                esc(&s.preview),
                s.modified
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("reckon-sheets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn titles() {
        assert_eq!(title_of("# Trip budget\nFlights: $640"), "Trip budget");
        assert_eq!(title_of("\n\n// notes\n1+1"), "notes");
        assert_eq!(title_of("   \n"), "Untitled");
        assert_eq!(safe_name("a/b: c?"), "a-b- c-");
        assert_eq!(safe_name("..hidden"), "hidden");
    }

    #[test]
    fn lifecycle() {
        let d = temp("life");
        let a = create(&d, "# Rent split\n$1,200 / 3").unwrap();
        assert_eq!(a, "Rent split.reckon");
        let b = create(&d, "# Rent split\nagain").unwrap();
        assert_eq!(b, "Rent split 2.reckon");
        write(&d, &b, "# Groceries\n$40").unwrap();
        assert_eq!(tidy(&d, &b).unwrap(), "Groceries.reckon");
        let sheets = list(&d);
        assert_eq!(sheets.len(), 2);
        assert_eq!(search(&d, &sheets, "1,200").len(), 1);
        assert_eq!(search(&d, &sheets, "grocer")[0].title, "Groceries");
        let t = trash(&d, "Groceries.reckon").unwrap();
        assert_eq!(list(&d).len(), 1);
        assert_eq!(restore(&d, &t).unwrap(), "Groceries.reckon");
        assert_eq!(list(&d).len(), 2);
        assert!(list_json(&d).contains("\"title\":\"Rent split\""));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn prepare_migrates_old_document() {
        let d = temp("prep");
        let old = d.join("document.reckon");
        fs::write(&old, "# Budget\n1+1").unwrap();
        let sheets = d.join("sheets");
        assert_eq!(prepare(&sheets, Some(&old), "x").unwrap(), "Budget.reckon");
        assert!(!old.exists());
        let empty = d.join("empty");
        assert_eq!(
            prepare(&empty, None, "# Welcome").unwrap(),
            "Welcome.reckon"
        );
        let _ = fs::remove_dir_all(&d);
    }
}
