//! Per-user data locations (rates cache, plugins, app documents).

use std::path::PathBuf;
use std::sync::RwLock;

static OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Sets the data folder explicitly (used by the iOS and Android apps, whose
/// sandboxes have their own locations). Takes precedence over everything.
pub fn set_data_dir(dir: PathBuf) {
    if let Ok(mut o) = OVERRIDE.write() {
        *o = Some(dir);
    }
}

/// Root data folder. [`set_data_dir`], then `RECKON_HOME`, override the
/// platform default.
///
/// * macOS: `~/Library/Application Support/Reckon`
/// * Windows: `%APPDATA%\Reckon`
/// * Linux/other: `$XDG_CONFIG_HOME/reckon` or `~/.config/reckon`
pub fn data_dir() -> Option<PathBuf> {
    if let Some(p) = OVERRIDE.read().ok().and_then(|o| o.clone()) {
        return Some(p);
    }
    if let Some(p) = std::env::var_os("RECKON_HOME") {
        return Some(PathBuf::from(p));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/Reckon"));
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|h| PathBuf::from(h).join("Reckon"));
    }
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(x).join("reckon"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/reckon"))
}

pub fn plugins_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("plugins"))
}

pub fn rates_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("rates.txt"))
}
