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

//!
//! The folder may be shared or synced, so its contents are not trusted:
//! only regular files are read or replaced (never symbolic links or
//! devices), sizes are capped, and writes go through a temporary file that
//! is created exclusively.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const EXTENSION: &str = "reckon";
/// Deleted sheets are moved here (inside the sheets folder) so they can be
/// restored.
pub const TRASH: &str = ".trash";
const MAX_NAME: usize = 60;
/// Largest sheet that is opened or listed. Sheets are hand-typed text; this
/// keeps a huge or endless file in the folder from being read into memory.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// How much of a sheet is read to find its title and preview.
const HEAD_BYTES: u64 = 4096;

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

/// A sheet's file name must be a plain name inside the folder.
fn check_name(file: &str) -> io::Result<()> {
    if file.is_empty() || file.starts_with('.') || file.contains(['/', '\\', '\0']) {
        return Err(invalid("not a sheet file name"));
    }
    Ok(())
}

/// Opens `path` only if it is an ordinary file of a sensible size. Symbolic
/// links are refused: in a shared folder one could point at any file the
/// user can read, or at a device that never ends.
fn open_regular(path: &Path) -> io::Result<fs::File> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid("not a regular file"));
    }
    let file = fs::File::open(path)?;
    // Checked again on the open file, in case the path was swapped.
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(invalid("not a regular file"));
    }
    if meta.len() > MAX_BYTES {
        return Err(invalid("the sheet is too large"));
    }
    Ok(file)
}

fn read_path(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    open_regular(path)?
        .take(MAX_BYTES)
        .read_to_string(&mut text)?;
    Ok(text)
}

/// The first few kilobytes of a sheet: enough for its title and preview
/// without reading every sheet in full each time the folder is listed.
fn read_head(path: &Path) -> io::Result<String> {
    let mut head = Vec::new();
    open_regular(path)?
        .take(HEAD_BYTES)
        .read_to_end(&mut head)?;
    Ok(String::from_utf8_lossy(&head).into_owned())
}

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
            let text = read_head(&p).ok()?;
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
                || read(dir, &s.file)
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
    check_name(file)?;
    read_path(&dir.join(file))
}

/// Writes a sheet through a temporary file and a rename, so a crash or a
/// sync tool never sees a half-written file.
///
/// The temporary file has an unpredictable name and is created exclusively
/// (the write fails rather than follow a link someone planted there), and a
/// destination that is not an ordinary file is left alone.
pub fn write(dir: &Path, file: &str, text: &str) -> io::Result<()> {
    check_name(file)?;
    write_file(dir, file, text)
}

/// [`write`] for any file name, including the hidden ones Reckon keeps in
/// the folder itself.
fn write_file(dir: &Path, file: &str, text: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let dest = dir.join(file);
    if let Ok(meta) = fs::symlink_metadata(&dest) {
        if !meta.file_type().is_file() {
            return Err(invalid("the sheet is not a regular file"));
        }
    }
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let tmp = dir.join(format!(
        ".{file}.{}-{nanos}-{}{TMP_SUFFIX}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .and_then(|()| fs::rename(&tmp, &dest));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

const TMP_SUFFIX: &str = ".tmp";

/// Removes temporary files that a crash left behind (older than an hour, so
/// another device's write in progress is not disturbed).
fn remove_stale_temps(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let ours = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            n.starts_with('.') && n.ends_with(TMP_SUFFIX) && n.contains(&format!(".{EXTENSION}."))
        });
        let old = fs::symlink_metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(3600));
        if ours && old {
            let _ = fs::remove_file(&path);
        }
    }
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
    check_name(file)?;
    let text = read_head(&dir.join(file))?;
    let wanted = free_name(dir, &title_of(&text), Some(file));
    if wanted != file {
        fs::rename(dir.join(file), dir.join(&wanted))?;
    }
    Ok(wanted)
}

/// Moves a sheet into the folder's `.trash`. Returns its path there, for
/// [`restore`].
pub fn trash(dir: &Path, file: &str) -> io::Result<PathBuf> {
    check_name(file)?;
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
    let text = read_head(trashed)?;
    let name = free_name(dir, &title_of(&text), None);
    fs::rename(trashed, dir.join(&name))?;
    Ok(name)
}

/// Sheets that come with the app besides the welcome sheet, as (id, text).
/// Each one shows a feature, and is added to a sheets folder once: on a new
/// installation, or on the first start after the update that brings it.
pub const SAMPLES: &[(&str, &str)] = &[
    ("statistics", include_str!("../samples/statistics.reckon")),
    (
        "method-comparison",
        include_str!("../samples/method-comparison.reckon"),
    ),
];

/// Hidden file in a sheets folder that lists the ids of the samples already
/// added to it, one per line. It lives in the folder, not in the app's
/// settings, so that a sample someone deleted stays deleted and a folder
/// synced between devices gets one copy instead of one from each device.
pub const SAMPLES_SEEN: &str = ".samples";

/// Adds the samples this folder has not had yet. Each is dated just before
/// `open`, the sheet about to be shown: the list is ordered by date, so a
/// new sample sits right under that sheet, where it is seen, and is never
/// the newest sheet, which is the one that opens when none is remembered.
fn add_samples(dir: &Path, open: &str) {
    let mut seen = read_path(&dir.join(SAMPLES_SEEN)).unwrap_or_default();
    let new: Vec<&(&str, &str)> = SAMPLES
        .iter()
        .filter(|(id, _)| !seen.lines().any(|line| line.trim() == *id))
        .collect();
    if new.is_empty() {
        return;
    }
    if !seen.is_empty() && !seen.ends_with('\n') {
        seen.push('\n');
    }
    for (id, _) in &new {
        seen.push_str(id);
        seen.push('\n');
    }
    // Record them first. If the folder cannot be written to, it is better
    // to add nothing than another copy on every start.
    if write_file(dir, SAMPLES_SEEN, &seen).is_err() {
        return;
    }
    let before_open = fs::metadata(dir.join(open))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.checked_sub(Duration::from_secs(1)));
    for (_, text) in new {
        let Ok(name) = create(dir, text) else {
            continue;
        };
        // Opened for writing: Windows will not change a date otherwise.
        let path = dir.join(&name);
        let regular = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file());
        if let (Some(when), true) = (before_open, regular) {
            if let Ok(file) = fs::OpenOptions::new().write(true).open(&path) {
                let _ = file.set_modified(when);
            }
        }
    }
}

/// Sets up a sheets folder: moves an old single-document file into it, or
/// creates a first sheet from `welcome` when the folder has none, and adds
/// any sample sheets that are new to the folder. Returns the sheet to open,
/// which is never one of those samples.
pub fn prepare(dir: &Path, legacy: Option<&Path>, welcome: &str) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    remove_stale_temps(dir);
    if let Some(old) = legacy.filter(|p| p.exists()) {
        let text = read_path(old)?;
        let name = create(dir, &text)?;
        fs::remove_file(old)?;
        add_samples(dir, &name);
        return Ok(name);
    }
    let open = match list(dir).into_iter().next() {
        Some(newest) => newest.file,
        None => create(dir, welcome)?,
    };
    add_samples(dir, &open);
    Ok(open)
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

    #[test]
    fn samples_are_added_once() {
        let titles = |d: &Path| -> Vec<String> { list(d).into_iter().map(|s| s.title).collect() };
        let samples = ["Lists and statistics", "Method comparison"].map(String::from);

        // A new installation gets the samples, and opens the welcome sheet.
        let d = temp("samples-new");
        assert_eq!(prepare(&d, None, "# Welcome").unwrap(), "Welcome.reckon");
        let mut got = titles(&d);
        got.sort();
        assert_eq!(got, [&samples[..], &["Welcome".to_string()]].concat());
        // Starting again adds nothing, and still opens the welcome sheet.
        assert_eq!(prepare(&d, None, "# Welcome").unwrap(), "Welcome.reckon");
        assert_eq!(list(&d).len(), 3);
        // A deleted sample stays deleted.
        trash(&d, "Lists and statistics.reckon").unwrap();
        trash(&d, "Method comparison.reckon").unwrap();
        prepare(&d, None, "# Welcome").unwrap();
        assert_eq!(titles(&d), ["Welcome"]);
        let _ = fs::remove_dir_all(&d);

        // An update: the folder has sheets but has never had the sample.
        // It is added, and the sheet that opens is still the user's own.
        let d = temp("samples-update");
        create(&d, "# Mine\n1 + 1").unwrap();
        assert_eq!(prepare(&d, None, "# Welcome").unwrap(), "Mine.reckon");
        // The samples are listed right under it, and do not take its place
        // on the next start either.
        assert_eq!(titles(&d)[0], "Mine");
        let mut got = titles(&d)[1..].to_vec();
        got.sort();
        assert_eq!(got, samples);
        assert_eq!(prepare(&d, None, "# Welcome").unwrap(), "Mine.reckon");
        // The record survives a later sample being added to it.
        fs::write(d.join(SAMPLES_SEEN), "something-older").unwrap();
        prepare(&d, None, "# Welcome").unwrap();
        assert_eq!(
            fs::read_to_string(d.join(SAMPLES_SEEN)).unwrap(),
            "something-older\nstatistics\nmethod-comparison\n"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn samples_are_not_added_when_the_record_cannot_be_kept() {
        // A link where the record goes: nothing is written through it, and
        // without a record no sample is added (or it would be added again
        // on every start).
        let d = temp("samples-link");
        let target = d.join("elsewhere.txt");
        fs::write(&target, "keep").unwrap();
        let sheets = d.join("sheets");
        fs::create_dir_all(&sheets).unwrap();
        std::os::unix::fs::symlink(&target, sheets.join(SAMPLES_SEEN)).unwrap();
        create(&sheets, "# Mine").unwrap();
        prepare(&sheets, None, "# Welcome").unwrap();
        assert_eq!(list(&sheets).len(), 1);
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn writes_leave_no_temporary_files() {
        let d = temp("tmp");
        write(&d, "A.reckon", "1").unwrap();
        write(&d, "A.reckon", "2").unwrap();
        assert_eq!(read(&d, "A.reckon").unwrap(), "2");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn file_names_cannot_leave_the_folder() {
        let d = temp("names");
        for bad in [
            "../x.reckon",
            "a/b.reckon",
            ".hidden.reckon",
            "",
            "a\\b.reckon",
        ] {
            assert!(write(&d, bad, "x").is_err(), "{bad}");
            assert!(read(&d, bad).is_err(), "{bad}");
            assert!(trash(&d, bad).is_err(), "{bad}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn oversized_sheets_are_not_read() {
        let d = temp("big");
        let f = fs::File::create(d.join("Big.reckon")).unwrap();
        f.set_len(MAX_BYTES + 1).unwrap();
        assert!(read(&d, "Big.reckon").is_err());
        assert!(list(&d).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn titles_come_from_the_head_of_long_sheets() {
        let d = temp("head");
        let long = format!("# Long\nfirst line\n{}", "1 + 1\n".repeat(5000));
        write(&d, "Long.reckon", &long).unwrap();
        let sheets = list(&d);
        assert_eq!(
            (sheets[0].title.as_str(), sheets[0].preview.as_str()),
            ("Long", "first line")
        );
        assert_eq!(read(&d, "Long.reckon").unwrap(), long);
        let _ = fs::remove_dir_all(&d);
    }

    /// A shared folder can contain links planted by someone else. They must
    /// never be read (leaking the target) or written through (replacing it).
    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_never_followed() {
        use std::os::unix::fs::symlink;
        let d = temp("links");
        let outside = temp("links-outside");
        let secret = outside.join("secret.txt");
        fs::write(&secret, "# secret").unwrap();

        // A sheet that is really a link to another file.
        symlink(&secret, d.join("Leak.reckon")).unwrap();
        assert!(list(&d).is_empty());
        assert!(read(&d, "Leak.reckon").is_err());
        assert!(tidy(&d, "Leak.reckon").is_err());
        assert!(write(&d, "Leak.reckon", "overwritten").is_err());
        assert_eq!(fs::read_to_string(&secret).unwrap(), "# secret");

        // A link where the old, predictable temporary file used to be.
        symlink(&secret, d.join(".Note.reckon.tmp")).unwrap();
        write(&d, "Note.reckon", "# Note").unwrap();
        assert_eq!(fs::read_to_string(&secret).unwrap(), "# secret");
        assert_eq!(read(&d, "Note.reckon").unwrap(), "# Note");

        // A link to a device that never ends.
        symlink("/dev/zero", d.join("Zero.reckon")).unwrap();
        assert!(read(&d, "Zero.reckon").is_err());
        assert_eq!(list(&d).len(), 1);
        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&outside);
    }
}
