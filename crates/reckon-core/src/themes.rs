//! Colour themes shared by every Reckon app.
//!
//! Twelve original themes, six light and six dark. Each one is built around
//! a single idea (graph paper, an accountant's ledger, a chalkboard, …) and
//! carries it through the background texture, the results column, the
//! highlight colours and the weight of the results. The desktop app reads
//! [`THEMES`] directly; the iOS and Android apps get them as JSON through the
//! C interface ([`themes_json`]).
//!
//! Colour rules every theme follows (checked by the tests below):
//! - Plain text has at least 7:1 contrast with the background; results,
//!   numbers and the other syntax colours at least 4.5:1 (WCAG AA).
//!   Comments are meant to recede, but stay at 2.2:1 or more.
//! - The result is the "answer", so it gets the theme's signature colour
//!   and a heavier weight. Labels are quieter than text, and comments are
//!   the quietest.
//! - Five syntax hues at most, chosen to sit together.

use crate::Hl;

/// Texture drawn behind the text and results.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pattern {
    None,
    /// Square grid, like graph paper: a line every half row, stronger on
    /// row boundaries.
    Grid,
    /// One rule under each line, like a ledger or notebook.
    Ruled,
    /// Fine horizontal lines every 3 points, like an old CRT.
    Scanlines,
}

impl Pattern {
    pub fn name(self) -> &'static str {
        match self {
            Pattern::None => "none",
            Pattern::Grid => "grid",
            Pattern::Ruled => "ruled",
            Pattern::Scanlines => "scanlines",
        }
    }
}

/// A palette as 0xRRGGBBAA values.
#[derive(Debug)]
pub struct Colors {
    pub bg: u32,
    /// Background of the results column.
    pub results_bg: u32,
    pub text: u32,
    /// Secondary UI text (status bar).
    pub muted: u32,
    pub result: u32,
    pub selection: u32,
    pub cursor: u32,
    /// Line between the text and the results.
    pub divider: u32,
    pub active_line: u32,
    /// Floating panels (the theme picker).
    pub surface: u32,
    pub border: u32,
    /// Selected row in a list.
    pub highlight: u32,
    pub status_bg: u32,
    /// Colour of the background texture.
    pub rule: u32,
    pub number: u32,
    pub unit: u32,
    pub keyword: u32,
    pub variable: u32,
    pub function: u32,
    pub operator: u32,
    pub label: u32,
    pub comment: u32,
    pub header: u32,
}

impl Colors {
    /// The colour for a highlight class.
    pub fn for_hl(&self, hl: Hl) -> u32 {
        match hl {
            Hl::Number => self.number,
            Hl::Unit => self.unit,
            Hl::Keyword => self.keyword,
            Hl::Variable => self.variable,
            Hl::Function => self.function,
            Hl::Operator => self.operator,
            Hl::Label => self.label,
            Hl::Comment => self.comment,
            Hl::Header => self.header,
            Hl::Error => self.muted,
        }
    }

    fn fields(&self) -> [(&'static str, u32); 23] {
        [
            ("bg", self.bg),
            ("results_bg", self.results_bg),
            ("text", self.text),
            ("muted", self.muted),
            ("result", self.result),
            ("selection", self.selection),
            ("cursor", self.cursor),
            ("divider", self.divider),
            ("active_line", self.active_line),
            ("surface", self.surface),
            ("border", self.border),
            ("highlight", self.highlight),
            ("status_bg", self.status_bg),
            ("rule", self.rule),
            ("number", self.number),
            ("unit", self.unit),
            ("keyword", self.keyword),
            ("variable", self.variable),
            ("function", self.function),
            ("operator", self.operator),
            ("label", self.label),
            ("comment", self.comment),
            ("header", self.header),
        ]
    }
}

#[derive(Debug)]
pub struct ThemeSpec {
    pub name: &'static str,
    /// One line shown in theme pickers.
    pub blurb: &'static str,
    pub dark: bool,
    pub c: Colors,
    pub pattern: Pattern,
    /// Two thin lines between text and results instead of one.
    pub double_divider: bool,
    /// Font weight of results (400 regular … 700 bold).
    pub result_weight: u16,
    /// A translucent block cursor instead of a bar (where the platform
    /// allows it).
    pub block_cursor: bool,
}

/// Index of "Paper" and "Graphite" in [`THEMES`]: the defaults when
/// following the system's light/dark appearance.
pub const DEFAULT_LIGHT: usize = 0;
pub const DEFAULT_DARK: usize = 6;

/// Index of the theme called `name` (case-insensitive).
pub fn find(name: &str) -> Option<usize> {
    THEMES
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name.trim()))
}

/// All themes as a JSON array, for apps that cannot read [`THEMES`]
/// directly. Colours are "#rrggbbaa" strings; field names match [`Colors`].
pub fn themes_json() -> String {
    let mut out = String::from("[");
    for (i, t) in THEMES.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"blurb\":\"{}\",\"dark\":{},\"pattern\":\"{}\",\"double_divider\":{},\"result_weight\":{},\"block_cursor\":{},\"colors\":{{",
            t.name,
            t.blurb,
            t.dark,
            t.pattern.name(),
            t.double_divider,
            t.result_weight,
            t.block_cursor
        ));
        for (j, (k, v)) in t.c.fields().iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{k}\":\"#{v:08x}\""));
        }
        out.push_str("}}");
    }
    out.push(']');
    out
}

pub static THEMES: &[ThemeSpec] = &[
    // ---------------------------------------------------------------- light
    ThemeSpec {
        name: "Paper",
        blurb: "Warm graph paper and fountain-pen ink",
        dark: false,
        c: Colors {
            bg: 0xfaf8f3ff,
            results_bg: 0xf3efe5ff,
            text: 0x2b2a27ff,
            muted: 0x706b61ff,
            result: 0x1e6f43ff,
            selection: 0xd5e1f3ff,
            cursor: 0x1f4fa8ff,
            divider: 0xe2dccdff,
            active_line: 0xefeadfb0,
            surface: 0xfffdf8ff,
            border: 0xddd6c6ff,
            highlight: 0xeee8daff,
            status_bg: 0xf3efe5ff,
            rule: 0xece6d8ff,
            number: 0x1f4fa8ff,
            unit: 0x2c7a72ff,
            keyword: 0x96397fff,
            variable: 0xa94f1cff,
            function: 0x45691fff,
            operator: 0x767166ff,
            label: 0x6f695eff,
            comment: 0xaaa293ff,
            header: 0xbf3a24ff,
        },
        pattern: Pattern::Grid,
        double_divider: false,
        result_weight: 600,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Ledger",
        blurb: "Bookkeeper's green paper with red rules",
        dark: false,
        c: Colors {
            bg: 0xf2f1e1ff,
            results_bg: 0xe9e9d3ff,
            text: 0x22332aff,
            muted: 0x5e6a60ff,
            result: 0x13325bff,
            selection: 0xd2e2cdff,
            cursor: 0xb3261eff,
            divider: 0xd65a4cff,
            active_line: 0xe5e5cdb0,
            surface: 0xfafaf0ff,
            border: 0xd3d1baff,
            highlight: 0xe4e3caff,
            status_bg: 0xe9e9d3ff,
            rule: 0xc9dccdff,
            number: 0x1f3a68ff,
            unit: 0x2e6b4fff,
            keyword: 0x7a2e5cff,
            variable: 0x82520eff,
            function: 0x2e6b4fff,
            operator: 0x5e6f64ff,
            label: 0x55665bff,
            comment: 0x98a290ff,
            header: 0xb3261eff,
        },
        pattern: Pattern::Ruled,
        double_divider: true,
        result_weight: 700,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Dune",
        blurb: "Sand, terracotta and desert sage",
        dark: false,
        c: Colors {
            bg: 0xf6eedfff,
            results_bg: 0xefe4cfff,
            text: 0x3b2f26ff,
            muted: 0x726251ff,
            result: 0x2c6a47ff,
            selection: 0xe8d4b4ff,
            cursor: 0xa8452aff,
            divider: 0xe2d3b9ff,
            active_line: 0xeee2ccb0,
            surface: 0xfbf6ecff,
            border: 0xdfcfb3ff,
            highlight: 0xede0c6ff,
            status_bg: 0xefe4cfff,
            rule: 0x00000000,
            number: 0xa3401fff,
            unit: 0x49735fff,
            keyword: 0x7b4b94ff,
            variable: 0x2f6690ff,
            function: 0x80600fff,
            operator: 0x7b6856ff,
            label: 0x7b6955ff,
            comment: 0xb09c84ff,
            header: 0xa3401fff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 600,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Fjord",
        blurb: "Crisp Nordic air, slate and glacier teal",
        dark: false,
        c: Colors {
            bg: 0xf4f7faff,
            results_bg: 0xeaf0f5ff,
            text: 0x1e2a36ff,
            muted: 0x5f6d7aff,
            result: 0x0b6f62ff,
            selection: 0xcde0f3ff,
            cursor: 0x2b6cb0ff,
            divider: 0xd9e2ebff,
            active_line: 0xe8eff5b0,
            surface: 0xffffffff,
            border: 0xd2dce6ff,
            highlight: 0xe2eaf2ff,
            status_bg: 0xeaf0f5ff,
            rule: 0x00000000,
            number: 0x0a6584ff,
            unit: 0x4466acff,
            keyword: 0x84479fff,
            variable: 0xab4d29ff,
            function: 0x2a7553ff,
            operator: 0x617283ff,
            label: 0x5d6e7eff,
            comment: 0x9aa8b6ff,
            header: 0x24467aff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 600,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Blossom",
        blurb: "Petal pink with plum ink and jade",
        dark: false,
        c: Colors {
            bg: 0xfbf4f5ff,
            results_bg: 0xf5e8ebff,
            text: 0x3d2a33ff,
            muted: 0x7d616cff,
            result: 0x23715aff,
            selection: 0xf1d1dcff,
            cursor: 0xb23a6bff,
            divider: 0xeedae1ff,
            active_line: 0xf4e6eab0,
            surface: 0xfffbfcff,
            border: 0xebd2dbff,
            highlight: 0xf2dfe6ff,
            status_bg: 0xf5e8ebff,
            rule: 0x00000000,
            number: 0xa8325fff,
            unit: 0x3a7873ff,
            keyword: 0x6544a8ff,
            variable: 0xb0562aff,
            function: 0x4a7835ff,
            operator: 0x856874ff,
            label: 0x86656fff,
            comment: 0xbc9ea9ff,
            header: 0xa8325fff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 600,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Delft",
        blurb: "Porcelain white glazed in cobalt",
        dark: false,
        c: Colors {
            bg: 0xf7f8fbff,
            results_bg: 0xedf0f8ff,
            text: 0x1b2340ff,
            muted: 0x626b8aff,
            result: 0x12286eff,
            selection: 0xd2dbf4ff,
            cursor: 0x1d3fa6ff,
            divider: 0xc9d2ebff,
            active_line: 0xeaeef8b0,
            surface: 0xffffffff,
            border: 0xd3d9ebff,
            highlight: 0xe2e7f5ff,
            status_bg: 0xedf0f8ff,
            rule: 0x00000000,
            number: 0x1d3fa6ff,
            unit: 0x2a66adff,
            keyword: 0x643a99ff,
            variable: 0x9b3b28ff,
            function: 0x1b6f80ff,
            operator: 0x5b6690ff,
            label: 0x535e88ff,
            comment: 0x9ea6c0ff,
            header: 0x1d3fa6ff,
        },
        pattern: Pattern::None,
        double_divider: true,
        result_weight: 700,
        block_cursor: false,
    },
    // ----------------------------------------------------------------- dark
    ThemeSpec {
        name: "Graphite",
        blurb: "Soft pencil on charcoal, mint answers",
        dark: true,
        c: Colors {
            bg: 0x18191bff,
            results_bg: 0x1d1f22ff,
            text: 0xe6e4dfff,
            muted: 0x8b8f96ff,
            result: 0x7fe0a6ff,
            selection: 0x2e3b55ff,
            cursor: 0x8fb5ffff,
            divider: 0x2a2c30ff,
            active_line: 0x23252980,
            surface: 0x222428ff,
            border: 0x34373cff,
            highlight: 0x2e3238ff,
            status_bg: 0x161719ff,
            rule: 0x00000000,
            number: 0xf2c46dff,
            unit: 0x7cc4d6ff,
            keyword: 0xc9a2f2ff,
            variable: 0x8fb5ffff,
            function: 0xa6d987ff,
            operator: 0xa9adb4ff,
            label: 0x9da1a8ff,
            comment: 0x5f656dff,
            header: 0xff9b73ff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 600,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Chalkboard",
        blurb: "Slate green board, coloured chalk",
        dark: true,
        c: Colors {
            bg: 0x1e2b25ff,
            results_bg: 0x22312aff,
            text: 0xd6ddd8ff,
            muted: 0x8fa197ff,
            result: 0xf6e27fff,
            selection: 0x34503fff,
            cursor: 0xfffdf5ff,
            divider: 0x34493dff,
            active_line: 0x26372e90,
            surface: 0x24342cff,
            border: 0x3a5145ff,
            highlight: 0x2f4639ff,
            status_bg: 0x1a2620ff,
            rule: 0x00000000,
            number: 0xfffdf5ff,
            unit: 0x9fd3f0ff,
            keyword: 0xf4a6c6ff,
            variable: 0xb9e6a1ff,
            function: 0xf7c08aff,
            operator: 0xc9d2cbff,
            label: 0xa9b8aeff,
            comment: 0x6d8275ff,
            header: 0xf7c08aff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 700,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Blueprint",
        blurb: "Drafting-table blue with a white grid",
        dark: true,
        c: Colors {
            bg: 0x0f2a4aff,
            results_bg: 0x12304fff,
            text: 0xdcebfaff,
            muted: 0x7fa2c8ff,
            result: 0xffe58aff,
            selection: 0x24507fff,
            cursor: 0xffe58aff,
            divider: 0x3a6a9eff,
            active_line: 0x16395f90,
            surface: 0x12345aff,
            border: 0x2d5a8aff,
            highlight: 0x1e4a78ff,
            status_bg: 0x0c2340ff,
            rule: 0x1a3d63ff,
            number: 0xffffffff,
            unit: 0x7fd8ffff,
            keyword: 0xffc27aff,
            variable: 0xa6f0d9ff,
            function: 0xffb38aff,
            operator: 0x9dbbdbff,
            label: 0x9dbbdbff,
            comment: 0x5f84aeff,
            header: 0xffc27aff,
        },
        pattern: Pattern::Grid,
        double_divider: false,
        result_weight: 700,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Nocturne",
        blurb: "Indigo midnight, violet and aqua",
        dark: true,
        c: Colors {
            bg: 0x14121fff,
            results_bg: 0x1a1729ff,
            text: 0xe2def5ff,
            muted: 0x8a84a8ff,
            result: 0x7ff2d0ff,
            selection: 0x33295aff,
            cursor: 0xc3a6ffff,
            divider: 0x2a2540ff,
            active_line: 0x1e1a3290,
            surface: 0x1e1a30ff,
            border: 0x352f52ff,
            highlight: 0x2c2647ff,
            status_bg: 0x110f1aff,
            rule: 0x00000000,
            number: 0xffb3c7ff,
            unit: 0x8fe3e0ff,
            keyword: 0xc3a6ffff,
            variable: 0x9db7ffff,
            function: 0xf5d08bff,
            operator: 0xa39dc4ff,
            label: 0xa39dc4ff,
            comment: 0x5e587aff,
            header: 0xc3a6ffff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 600,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Ember",
        blurb: "Espresso dark with glowing amber",
        dark: true,
        c: Colors {
            bg: 0x1c1714ff,
            results_bg: 0x221b17ff,
            text: 0xeadbc8ff,
            muted: 0x9a8775ff,
            result: 0xff9f5aff,
            selection: 0x4a3426ff,
            cursor: 0xff9f5aff,
            divider: 0x30261fff,
            active_line: 0x261f1a90,
            surface: 0x251e19ff,
            border: 0x3d3128ff,
            highlight: 0x352a22ff,
            status_bg: 0x181310ff,
            rule: 0x00000000,
            number: 0xf2cf7aff,
            unit: 0x8fc7b1ff,
            keyword: 0xe9806eff,
            variable: 0xd9a6c2ff,
            function: 0xa9c98aff,
            operator: 0xb39f8aff,
            label: 0xb39f8aff,
            comment: 0x6e5f52ff,
            header: 0xe9806eff,
        },
        pattern: Pattern::None,
        double_divider: false,
        result_weight: 700,
        block_cursor: false,
    },
    ThemeSpec {
        name: "Phosphor",
        blurb: "Green-screen terminal, scanlines and a block cursor",
        dark: true,
        c: Colors {
            bg: 0x0a0f0bff,
            results_bg: 0x0c140eff,
            text: 0x7cf0a0ff,
            muted: 0x3f8f5aff,
            result: 0xe6fff0ff,
            selection: 0x1c4a2bff,
            cursor: 0x7cf0a08c,
            divider: 0x1b3a25ff,
            active_line: 0x10201590,
            surface: 0x0e1811ff,
            border: 0x1e4029ff,
            highlight: 0x173521ff,
            status_bg: 0x08100aff,
            rule: 0x7cf0a00a,
            number: 0xb8ffcfff,
            unit: 0x7cf0a0ff,
            keyword: 0xffd27aff,
            variable: 0x9bffc0ff,
            function: 0x7cf0a0ff,
            operator: 0x5fc985ff,
            label: 0x5fc985ff,
            comment: 0x2e6b42ff,
            header: 0xffd27aff,
        },
        pattern: Pattern::Scanlines,
        double_divider: false,
        result_weight: 700,
        block_cursor: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance of an opaque 0xRRGGBBAA colour.
    fn luminance(c: u32) -> f64 {
        let ch = |shift: u32| {
            let v = ((c >> shift) & 0xff) as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(24) + 0.7152 * ch(16) + 0.0722 * ch(8)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn contrast_is_readable() {
        let mut failures = Vec::new();
        for t in THEMES {
            let s = &t.c;
            let mut check = |what: &str, fg: u32, bg_name: &str, bg: u32, min: f64| {
                let r = contrast(fg, bg);
                if r < min {
                    failures.push(format!("{} {what} on {bg_name}: {r:.2} < {min}", t.name));
                }
            };
            check("text", s.text, "bg", s.bg, 7.0);
            // Results are drawn on the results column (and its active row).
            check("result", s.result, "results_bg", s.results_bg, 4.5);
            check("result", s.result, "bg", s.bg, 4.5);
            // Status-bar text.
            check("muted", s.muted, "status_bg", s.status_bg, 4.5);
            for (name, c) in [
                ("number", s.number),
                ("unit", s.unit),
                ("keyword", s.keyword),
                ("variable", s.variable),
                ("function", s.function),
                ("operator", s.operator),
                ("label", s.label),
                ("header", s.header),
                ("muted", s.muted),
            ] {
                check(name, c, "bg", s.bg, 4.5);
            }
            check("comment", s.comment, "bg", s.bg, 2.2);
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn defaults_and_order() {
        assert_eq!(THEMES[DEFAULT_LIGHT].name, "Paper");
        assert_eq!(THEMES[DEFAULT_DARK].name, "Graphite");
        // Light themes first, then dark, so pickers can group them.
        let first_dark = THEMES.iter().position(|t| t.dark).unwrap();
        assert!(THEMES[first_dark..].iter().all(|t| t.dark));
        for (i, t) in THEMES.iter().enumerate() {
            assert_eq!(find(t.name), Some(i), "{}", t.name);
            assert!(!t.name.contains('"') && !t.blurb.contains('"'));
        }
    }

    #[test]
    fn json_lists_every_theme() {
        let json = crate::json::parse(&themes_json()).expect("valid JSON");
        let crate::json::Json::Arr(items) = json else {
            panic!("not an array")
        };
        assert_eq!(items.len(), THEMES.len());
    }
}
