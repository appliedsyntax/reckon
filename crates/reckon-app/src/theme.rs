//! Light and dark colour palettes.

use gpui::{rgb, Hsla, WindowAppearance};
use reckon_core::Hl;

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub result: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
    pub divider: Hsla,
    pub active_line: Hsla,
    number: Hsla,
    unit: Hsla,
    keyword: Hsla,
    variable: Hsla,
    function: Hsla,
    operator: Hsla,
    label: Hsla,
    comment: Hsla,
    header: Hsla,
}

impl Theme {
    pub fn for_appearance(a: WindowAppearance) -> Theme {
        match a {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
            _ => Theme::light(),
        }
    }

    pub fn dark() -> Theme {
        Theme {
            bg: rgb(0x1c1c1e).into(),
            text: rgb(0xe4e4e6).into(),
            muted: rgb(0x7f848e).into(),
            result: rgb(0x7ee0a1).into(),
            selection: rgb(0x3a4a66).into(),
            cursor: rgb(0x6ea8ff).into(),
            divider: rgb(0x2c2c2f).into(),
            active_line: rgb(0x232326).into(),
            number: rgb(0xf0c674).into(),
            unit: rgb(0x6fc3d1).into(),
            keyword: rgb(0xc792ea).into(),
            variable: rgb(0x82aaff).into(),
            function: rgb(0x9ece6a).into(),
            operator: rgb(0xb0b4bc).into(),
            label: rgb(0x8a8f98).into(),
            comment: rgb(0x5f6570).into(),
            header: rgb(0xff8f6b).into(),
        }
    }

    pub fn light() -> Theme {
        Theme {
            bg: rgb(0xfbfbfa).into(),
            text: rgb(0x2f3337).into(),
            muted: rgb(0x8b9098).into(),
            result: rgb(0x1f8a4c).into(),
            selection: rgb(0xcfe0ff).into(),
            cursor: rgb(0x2f6fed).into(),
            divider: rgb(0xe7e7e4).into(),
            active_line: rgb(0xf2f2ef).into(),
            number: rgb(0x9a6700).into(),
            unit: rgb(0x0b7a99).into(),
            keyword: rgb(0x8f3fb8).into(),
            variable: rgb(0x2f5fd0).into(),
            function: rgb(0x3d8b37).into(),
            operator: rgb(0x55595f).into(),
            label: rgb(0x8b9098).into(),
            comment: rgb(0xa3a7ad).into(),
            header: rgb(0xcf4a2c).into(),
        }
    }

    pub fn color(&self, hl: Hl) -> Hsla {
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
}
