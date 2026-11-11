//! Reckon desktop app (macOS, Windows, Linux) built on GPUI.

// No console window behind the app in Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod calc;
mod theme;

use std::borrow::Cow;

use gpui::{
    px, size, App, AppContext, Application, Bounds, Focusable, KeyBinding, Menu, MenuItem,
    SharedString, TitlebarOptions, WindowBounds, WindowOptions,
};

use calc::*;
use theme::THEMES;

fn bind_keys(cx: &mut App) {
    let mac = cfg!(target_os = "macos");
    let m = if mac { "cmd" } else { "ctrl" };
    let word = if mac { "alt" } else { "ctrl" };
    let ctx = Some("Calc");
    let k = |s: String| s;
    let mut b = vec![
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new(&k(format!("{word}-backspace")), DeleteWordLeft, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new(&format!("{word}-left"), WordLeft, ctx),
        KeyBinding::new(&format!("{word}-right"), WordRight, ctx),
        KeyBinding::new(&format!("{word}-shift-left"), SelectWordLeft, ctx),
        KeyBinding::new(&format!("{word}-shift-right"), SelectWordRight, ctx),
        KeyBinding::new("home", LineStart, ctx),
        KeyBinding::new("end", LineEnd, ctx),
        KeyBinding::new("shift-home", SelectLineStart, ctx),
        KeyBinding::new("shift-end", SelectLineEnd, ctx),
        KeyBinding::new("enter", Newline, ctx),
        KeyBinding::new(&format!("{m}-a"), SelectAll, ctx),
        KeyBinding::new(&format!("{m}-c"), Copy, ctx),
        KeyBinding::new(&format!("{m}-x"), Cut, ctx),
        KeyBinding::new(&format!("{m}-v"), Paste, ctx),
        KeyBinding::new(&format!("{m}-z"), Undo, ctx),
        KeyBinding::new(&format!("{m}-shift-z"), Redo, ctx),
        KeyBinding::new(&format!("{m}-shift-c"), CopyResult, ctx),
        KeyBinding::new(&format!("{m}-alt-c"), CopyAll, ctx),
        KeyBinding::new(&format!("{m}-alt-backspace"), DeleteAll, ctx),
        KeyBinding::new(&format!("{m}-shift-0"), WrapParens, ctx),
        KeyBinding::new(&format!("{m}-)"), WrapParens, ctx),
        KeyBinding::new(&format!("{m}-o"), Open, ctx),
        KeyBinding::new(&format!("{m}-s"), Export, ctx),
        KeyBinding::new(&format!("{m}-q"), Quit, None),
        // Same chord as Zed's theme selector.
        KeyBinding::new(&format!("{m}-k {m}-t"), ToggleThemePicker, None),
        KeyBinding::new("up", PickerUp, Some("ThemePicker")),
        KeyBinding::new("down", PickerDown, Some("ThemePicker")),
        KeyBinding::new("enter", PickerConfirm, Some("ThemePicker")),
        KeyBinding::new("escape", PickerCancel, Some("ThemePicker")),
    ];
    if mac {
        b.extend([
            KeyBinding::new("cmd-backspace", DeleteToLineStart, ctx),
            KeyBinding::new("cmd-left", LineStart, ctx),
            KeyBinding::new("cmd-right", LineEnd, ctx),
            KeyBinding::new("cmd-shift-left", SelectLineStart, ctx),
            KeyBinding::new("cmd-shift-right", SelectLineEnd, ctx),
            KeyBinding::new("cmd-up", DocStart, ctx),
            KeyBinding::new("cmd-down", DocEnd, ctx),
            KeyBinding::new("cmd-shift-up", SelectDocStart, ctx),
            KeyBinding::new("cmd-shift-down", SelectDocEnd, ctx),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
        ]);
    } else {
        b.extend([
            KeyBinding::new("ctrl-home", DocStart, ctx),
            KeyBinding::new("ctrl-end", DocEnd, ctx),
            KeyBinding::new("ctrl-shift-home", SelectDocStart, ctx),
            KeyBinding::new("ctrl-shift-end", SelectDocEnd, ctx),
            KeyBinding::new("ctrl-y", Redo, ctx),
        ]);
    }
    cx.bind_keys(b);
}

/// Picks the editor font: a monospaced face so digits and operators line up.
///
/// macOS keeps SF Mono as a hidden system font (family `.SF NS Mono`) that
/// cannot be looked up by name, so it is loaded from the system font folder
/// at startup. Nothing is bundled. Other platforms use their usual monospace
/// fonts, and the system UI font is the last resort.
fn editor_font(cx: &App) -> SharedString {
    if cfg!(target_os = "macos") {
        if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNSMono.ttf") {
            let _ = cx.text_system().add_fonts(vec![Cow::Owned(bytes)]);
        }
    }
    let available = cx.text_system().all_font_names();
    let candidates = [
        ".SF NS Mono",
        "SF Mono",
        "Menlo",
        "Cascadia Mono",
        "Consolas",
        "DejaVu Sans Mono",
        "Liberation Mono",
        "Noto Sans Mono",
    ];
    candidates
        .iter()
        .find(|c| available.iter().any(|a| a == *c))
        .map(|c| SharedString::from(*c))
        .unwrap_or_else(|| ".SystemUIFont".into())
}

/// The menu bar. Rebuilt when the theme changes, because GPUI menus have no
/// check state: the current theme gets a ✓ in its name instead.
pub fn app_menus(choice: &theme::ThemeChoice) -> Vec<Menu> {
    vec![
        Menu {
            name: "Reckon".into(),
            items: vec![MenuItem::action("Quit Reckon", Quit)],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open…", Open),
                MenuItem::action("Export…", Export),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", Cut),
                MenuItem::action("Copy", Copy),
                MenuItem::action("Paste", Paste),
                MenuItem::action("Select All", SelectAll),
                MenuItem::separator(),
                MenuItem::action("Copy Result", CopyResult),
                MenuItem::action("Copy All", CopyAll),
                MenuItem::action("Wrap in Parentheses", WrapParens),
                MenuItem::action("Delete All", DeleteAll),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![MenuItem::submenu(Menu {
                name: "Theme".into(),
                items: theme_items(choice),
            })],
        },
    ]
}

fn theme_items(choice: &theme::ThemeChoice) -> Vec<MenuItem> {
    let tick = |on: bool, name: &str| -> String {
        if on {
            format!("✓  {name}")
        } else {
            format!("     {name}")
        }
    };
    let current = choice.current_row();
    let mut items = vec![
        MenuItem::action("Select Theme…", ToggleThemePicker),
        MenuItem::separator(),
        MenuItem::action(tick(current == 0, "Match System"), MatchSystemTheme),
        MenuItem::separator(),
    ];
    let mut was_dark = false;
    for (index, t) in THEMES.iter().enumerate() {
        if t.dark && !was_dark {
            items.push(MenuItem::separator());
            was_dark = true;
        }
        items.push(MenuItem::action(
            tick(current == index + 1, t.name),
            SelectTheme { index },
        ));
    }
    items
}

fn main() {
    Application::new().run(|cx: &mut App| {
        bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(app_menus(&theme::ThemeChoice::load()));
        let bounds = Bounds::centered(None, size(px(780.), px(580.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Reckon".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let font = editor_font(cx);
        if std::env::var_os("RECKON_DEBUG").is_some() {
            eprintln!("reckon-app: editor font {font}");
        }
        let window = cx
            .open_window(options, |window, cx| {
                cx.new(|cx| Calc::new(window, font, cx))
            })
            .expect("open window");
        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .ok();
        cx.on_window_closed(|cx| cx.quit()).detach();
    });
}
