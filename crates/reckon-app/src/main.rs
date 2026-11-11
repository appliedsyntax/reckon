//! Reckon desktop app (macOS, Windows, Linux) built on GPUI.

// No console window behind the app in Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod calc;
mod conf;
mod hotkey;
mod input;
mod theme;

use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver};

use gpui::{
    point, px, size, App, AppContext, Application, Bounds, Focusable, KeyBinding, Menu, MenuItem,
    SharedString, TitlebarOptions, WindowBounds, WindowHandle, WindowKind, WindowOptions,
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
        KeyBinding::new(&format!("{m}-n"), NewSheet, ctx),
        KeyBinding::new(&format!("{m}-p"), FindSheet, ctx),
        KeyBinding::new(&format!("{m}-\\"), ToggleSidebar, ctx),
        KeyBinding::new("escape", DismissQuick, ctx),
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
    cx.bind_keys(input::bindings());
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
                MenuItem::action("New Sheet", NewSheet),
                MenuItem::action("Find Sheet…", FindSheet),
                MenuItem::separator(),
                MenuItem::action("Open…", Open),
                MenuItem::action("Export…", Export),
                MenuItem::separator(),
                MenuItem::action("Choose Sheets Folder…", ChooseSheetsFolder),
                MenuItem::action("Show Sheets Folder", RevealSheetsFolder),
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
            items: vec![
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Theme".into(),
                    items: theme_items(choice),
                }),
            ],
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

thread_local! {
    static MAIN: RefCell<Option<WindowHandle<Calc>>> = const { RefCell::new(None) };
    static QUICK: RefCell<Option<WindowHandle<Calc>>> = const { RefCell::new(None) };
    static FONT: RefCell<SharedString> = RefCell::new(SharedString::default());
}

fn open_main(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(940.), px(600.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Reckon".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let font = FONT.with(|f| f.borrow().clone());
    let window = cx
        .open_window(options, |window, cx| {
            cx.new(|cx| Calc::new(window, font, false, cx))
        })
        .expect("open window");
    window
        .update(cx, |view, window, cx| {
            window.focus(&view.focus_handle(cx));
            cx.activate(true);
        })
        .ok();
    MAIN.with(|m| *m.borrow_mut() = Some(window));
}

/// Shows the quick-calc box, or closes it if it is already open.
fn toggle_quick(cx: &mut App) {
    if let Some(h) = QUICK.with(|q| q.borrow_mut().take()) {
        if h.update(cx, |_, window, _| window.remove_window()).is_ok() {
            return;
        }
    }
    let (w, h) = (px(680.), px(56.));
    let origin = cx
        .primary_display()
        .map(|d| {
            let b = d.bounds();
            point(
                b.origin.x + (b.size.width - w) / 2.,
                b.origin.y + b.size.height * 0.28,
            )
        })
        .unwrap_or_else(|| point(px(200.), px(200.)));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size(w, h)))),
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(px(-100.), px(-100.))),
        }),
        kind: WindowKind::PopUp,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        ..Default::default()
    };
    let font = FONT.with(|f| f.borrow().clone());
    let Ok(window) = cx.open_window(options, |window, cx| {
        cx.new(|cx| Calc::new(window, font, true, cx))
    }) else {
        return;
    };
    window
        .update(cx, |view, window, cx| {
            window.focus(&view.focus_handle(cx));
            window.activate_window();
            // macOS shows the box as a non-activating panel, so Reckon's main
            // window does not jump to the front; elsewhere the app must be
            // active for the box to get the keyboard.
            if !cfg!(target_os = "macos") {
                cx.activate(true);
            }
        })
        .ok();
    QUICK.with(|q| *q.borrow_mut() = Some(window));
}

/// Opens the quick box each time `rx` fires. The wait happens on a
/// background thread, so nothing polls.
fn listen_quick(rx: Receiver<()>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut rx = rx;
        loop {
            let (got, back) = cx
                .background_executor()
                .spawn(async move { (rx.recv(), rx) })
                .await;
            rx = back;
            if got.is_err() || cx.update(toggle_quick).is_err() {
                break;
            }
        }
    })
    .detach();
}

fn main() {
    let quick_only = std::env::args().any(|a| a == "--quick");
    // `reckon-app --quick`: hand the request to a running app if there is one.
    #[cfg(unix)]
    if quick_only && hotkey::socket::ask_running_app() {
        return;
    }
    Application::new().run(move |cx: &mut App| {
        bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(app_menus(&theme::ThemeChoice::load()));
        let font = editor_font(cx);
        if std::env::var_os("RECKON_DEBUG").is_some() {
            eprintln!("reckon-app: editor font {font}");
        }
        FONT.with(|f| *f.borrow_mut() = font);

        if quick_only {
            // No app running: show just the box and quit when it closes.
            toggle_quick(cx);
            cx.on_window_closed(|cx| cx.quit()).detach();
            return;
        }

        open_main(cx);
        let (tx, rx) = channel();
        #[cfg(unix)]
        hotkey::socket::listen(tx.clone());
        let shortcut = conf::get("quick_shortcut").unwrap_or_else(|| hotkey::DEFAULT.into());
        match hotkey::parse(&shortcut) {
            Some(sc) => {
                if let Err(e) = hotkey::register(sc, tx) {
                    if std::env::var_os("RECKON_DEBUG").is_some() {
                        eprintln!("reckon-app: quick shortcut {}: {e}", hotkey::label(&sc));
                    }
                }
            }
            None => eprintln!("reckon-app: quick_shortcut \"{shortcut}\" is not a valid shortcut"),
        }
        listen_quick(rx, cx);
        if std::env::var_os("RECKON_QUICK_ON_START").is_some() {
            toggle_quick(cx);
        }

        // Closing the main window quits; closing the quick box does not.
        cx.on_window_closed(|cx| {
            let main_open = MAIN.with(|m| m.borrow().map(|h| h.update(cx, |_, _, _| ()).is_ok()));
            if main_open != Some(true) {
                cx.quit();
            }
        })
        .detach();
    });
}
