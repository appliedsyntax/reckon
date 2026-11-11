//! `app.conf` in the data folder: `key = value` lines for the desktop app's
//! settings (theme, sheets folder, quick-calc shortcut, …). Writing one key
//! keeps every other line, including unknown ones.

use std::path::PathBuf;

use reckon_core::paths;

pub fn path() -> Option<PathBuf> {
    paths::data_dir().map(|d| d.join("app.conf"))
}

pub fn read() -> String {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

/// The value of `key` in `text`, trimmed.
pub fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim())
    })
}

pub fn get(key: &str) -> Option<String> {
    value(&read(), key).map(str::to_string)
}

/// `text` with each `(key, value)` replaced in place, or appended.
pub fn with(text: &str, pairs: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for (key, val) in pairs {
        let line = format!("{key} = {val}");
        match lines
            .iter()
            .position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == *key))
        {
            Some(i) => lines[i] = line,
            None => lines.push(line),
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn set(pairs: &[(&str, String)]) {
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, with(&read(), pairs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_other_keys() {
        let t = "theme = Paper\n# note\nsheets_folder = /x\n";
        let t = with(t, &[("theme", "Ember".into()), ("new_key", "1".into())]);
        assert_eq!(
            t,
            "theme = Ember\n# note\nsheets_folder = /x\nnew_key = 1\n"
        );
        assert_eq!(value(&t, "sheets_folder"), Some("/x"));
    }
}
