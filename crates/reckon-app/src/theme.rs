//! Drawing colours for the desktop app and the user's theme choice.
//!
//! The themes themselves live in `reckon_core::themes`, shared with the
//! iOS and Android apps.

use std::path::PathBuf;

use gpui::{rgba, FontWeight, Hsla, WindowAppearance};
use reckon_core::themes::{find, DEFAULT_DARK, DEFAULT_LIGHT};
pub use reckon_core::themes::{Pattern, THEMES};
use reckon_core::{paths, Hl};

/// Resolved colours for drawing.
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Hsla,
    pub results_bg: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub result: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
    pub divider: Hsla,
    pub active_line: Hsla,
    /// Background of floating panels (the theme picker).
    pub surface: Hsla,
    pub border: Hsla,
    /// Selected row in a list.
    pub highlight: Hsla,
    pub status_bg: Hsla,
    /// Colour of the background texture.
    pub rule: Hsla,
    pub pattern: Pattern,
    pub double_divider: bool,
    pub result_weight: FontWeight,
    pub block_cursor: bool,
    index: usize,
}

fn hsla(v: u32) -> Hsla {
    rgba(v).into()
}

impl Theme {
    pub fn get(index: usize) -> Theme {
        let index = index.min(THEMES.len() - 1);
        let t = &THEMES[index];
        let c = &t.c;
        Theme {
            bg: hsla(c.bg),
            results_bg: hsla(c.results_bg),
            text: hsla(c.text),
            muted: hsla(c.muted),
            result: hsla(c.result),
            selection: hsla(c.selection),
            cursor: hsla(c.cursor),
            divider: hsla(c.divider),
            active_line: hsla(c.active_line),
            surface: hsla(c.surface),
            border: hsla(c.border),
            highlight: hsla(c.highlight),
            status_bg: hsla(c.status_bg),
            rule: hsla(c.rule),
            pattern: t.pattern,
            double_divider: t.double_divider,
            result_weight: FontWeight(t.result_weight as f32),
            block_cursor: t.block_cursor,
            index,
        }
    }

    pub fn color(&self, hl: Hl) -> Hsla {
        hsla(THEMES[self.index].c.for_hl(hl))
    }

    /// Colours for the picker's swatch: background, a number, the result.
    pub fn swatch(index: usize) -> [Hsla; 3] {
        let c = &THEMES[index].c;
        [hsla(c.bg), hsla(c.number), hsla(c.result)]
    }
}

fn is_dark(a: WindowAppearance) -> bool {
    matches!(a, WindowAppearance::Dark | WindowAppearance::VibrantDark)
}

/// Which theme to show. The app can follow
/// the system's light/dark setting with one theme for each, or always use
/// one theme. Saved in `app.conf` in the data folder.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ThemeChoice {
    pub follow_system: bool,
    pub light: usize,
    pub dark: usize,
    pub fixed: usize,
}

impl Default for ThemeChoice {
    fn default() -> Self {
        ThemeChoice {
            follow_system: true,
            light: DEFAULT_LIGHT,
            dark: DEFAULT_DARK,
            fixed: DEFAULT_DARK,
        }
    }
}

impl ThemeChoice {
    fn path() -> Option<PathBuf> {
        paths::data_dir().map(|d| d.join("app.conf"))
    }

    /// Reads `key = value` lines; unknown keys and theme names are ignored.
    pub fn parse(text: &str) -> ThemeChoice {
        let mut c = ThemeChoice::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match (key.trim(), value.trim()) {
                ("theme_mode", v) => c.follow_system = v != "fixed",
                ("theme", v) => c.fixed = find(v).unwrap_or(c.fixed),
                ("light_theme", v) => c.light = find(v).unwrap_or(c.light),
                ("dark_theme", v) => c.dark = find(v).unwrap_or(c.dark),
                _ => {}
            }
        }
        c
    }

    pub fn serialize(self) -> String {
        format!(
            "theme_mode = {}\ntheme = {}\nlight_theme = {}\ndark_theme = {}\n",
            if self.follow_system {
                "system"
            } else {
                "fixed"
            },
            THEMES[self.fixed].name,
            THEMES[self.light].name,
            THEMES[self.dark].name,
        )
    }

    pub fn load() -> ThemeChoice {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, self.serialize());
        }
    }

    /// The choice that picker row `row` previews: row 0 is "Match
    /// System", row `i + 1` is `THEMES[i]`.
    pub fn previewed(self, row: usize) -> ThemeChoice {
        let mut c = self;
        if row == 0 {
            c.follow_system = true;
        } else {
            c.pick(row - 1);
        }
        c
    }

    /// The picker row that represents this choice.
    pub fn current_row(&self) -> usize {
        if self.follow_system {
            0
        } else {
            self.fixed + 1
        }
    }

    /// The theme index to draw with for the window's current appearance.
    pub fn resolve(&self, appearance: WindowAppearance) -> usize {
        match (self.follow_system, is_dark(appearance)) {
            (false, _) => self.fixed,
            (true, true) => self.dark,
            (true, false) => self.light,
        }
    }

    /// Always use theme `index`. It also becomes the light or dark theme
    /// used when following the system again.
    pub fn pick(&mut self, index: usize) {
        self.follow_system = false;
        self.fixed = index;
        if THEMES[index].dark {
            self.dark = index;
        } else {
            self.light = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_round_trips() {
        let mut c = ThemeChoice::default();
        c.pick(find("Blossom").unwrap());
        c.pick(find("Nocturne").unwrap());
        assert_eq!(ThemeChoice::parse(&c.serialize()), c);
        assert_eq!(THEMES[c.light].name, "Blossom");
        assert_eq!(THEMES[c.dark].name, "Nocturne");
        assert_eq!(c.resolve(WindowAppearance::Light), c.fixed);
    }

    #[test]
    fn follows_system() {
        let c =
            ThemeChoice::parse("theme_mode = system\nlight_theme = Fjord\ndark_theme = Ember\n");
        assert_eq!(THEMES[c.resolve(WindowAppearance::Light)].name, "Fjord");
        assert_eq!(THEMES[c.resolve(WindowAppearance::Dark)].name, "Ember");
    }
}
