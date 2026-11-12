//! The app's own icons, embedded in the binary and served to GPUI's `svg()`
//! element. They are drawn for Reckon (16-unit grid, 1.3 stroke) and are
//! tinted with the theme's text colours when painted.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

const ICONS: &[(&str, &str)] = &[
    (
        "icons/sidebar.svg",
        include_str!("../assets/icons/sidebar.svg"),
    ),
    ("icons/plus.svg", include_str!("../assets/icons/plus.svg")),
    ("icons/gear.svg", include_str!("../assets/icons/gear.svg")),
    ("icons/close.svg", include_str!("../assets/icons/close.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, data)| Cow::Borrowed(data.as_bytes())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| (*p).into())
            .collect())
    }
}
