//! Reckon desktop app (macOS, Windows, Linux) built on GPUI.

// No console window behind the app in Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
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
    OsAction, Pixels, SharedString, SystemMenuType, TitlebarOptions, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
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
        // Standard app and window shortcuts work wherever the focus is.
        KeyBinding::new(&format!("{m}-,"), OpenSettings, None),
        KeyBinding::new(&format!("{m}-="), ZoomIn, None),
        KeyBinding::new(&format!("{m}-+"), ZoomIn, None),
        KeyBinding::new(&format!("{m}--"), ZoomOut, None),
        KeyBinding::new(&format!("{m}-0"), ZoomReset, None),
        KeyBinding::new("escape", CloseSettings, Some("Settings")),
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
            // Show/Hide Sidebar and full screen use the system-wide macOS
            // shortcuts.
            KeyBinding::new("ctrl-cmd-s", ToggleSidebar, None),
            KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
            KeyBinding::new("cmd-m", Minimize, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("alt-cmd-h", HideOthers, None),
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
            KeyBinding::new("ctrl-b", ToggleSidebar, None),
            KeyBinding::new("f11", ToggleFullScreen, None),
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
    let ts = cx.text_system();
    if cfg!(target_os = "macos") {
        if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNSMono.ttf") {
            let _ = ts.add_fonts(vec![Cow::Owned(bytes)]);
        }
        // Ask for each font by name. Listing every installed font to see
        // which exist took about 150 ms, half of the whole start-up.
        let found = |name: &'static str| {
            let wanted = gpui::font(name);
            // A font that cannot be loaded resolves to a fallback instead.
            let got = ts.get_font_for_id(ts.resolve_font(&wanted));
            got.is_some_and(|f| f.family == wanted.family)
        };
        return [".SF NS Mono", "SF Mono", "Menlo"]
            .into_iter()
            .find(|name| found(name))
            .unwrap_or("Menlo")
            .into();
    }
    // Windows and Linux keep their fonts in memory, so the list is cheap.
    let available = ts.all_font_names();
    let candidates = [
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
pub fn app_menus(choice: &theme::ThemeChoice, sidebar_open: bool, fullscreen: bool) -> Vec<Menu> {
    vec![
        // The application menu, in the order macOS apps use.
        Menu {
            name: "Reckon".into(),
            items: vec![
                MenuItem::action("About Reckon", OpenAbout),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Reckon", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Reckon", Quit),
            ],
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
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                // Tagged as the system's edit commands, so they also work in
                // native text fields such as the Open and Save dialogs.
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
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
                MenuItem::action(
                    if sidebar_open {
                        "Hide Sidebar"
                    } else {
                        "Show Sidebar"
                    },
                    ToggleSidebar,
                ),
                MenuItem::separator(),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
                MenuItem::action("Actual Size", ZoomReset),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Theme".into(),
                    items: theme_items(choice),
                }),
                MenuItem::separator(),
                MenuItem::action(
                    if fullscreen {
                        "Exit Full Screen"
                    } else {
                        "Enter Full Screen"
                    },
                    ToggleFullScreen,
                ),
            ],
        },
        // macOS adds its own items to a menu named "Window": the tiling
        // commands (Fill, Center, Move & Resize, Full Screen Tile) and the
        // list of open windows.
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", ZoomWindow),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![
                MenuItem::action("Reckon User Guide", OpenHelp),
                MenuItem::action("Report an Issue…", ReportIssue),
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

/// The window frame saved in app.conf (`window = x,y,w,h`), if there is one;
/// otherwise a centred default.
fn initial_bounds(cx: &App) -> Bounds<Pixels> {
    let default = Bounds::centered(None, size(px(940.), px(600.)), cx);
    let saved = conf::get("window").and_then(|v| parse_frame(&v));
    let (Some(display), Some(saved)) = (cx.primary_display(), saved) else {
        return default;
    };
    let s = display.bounds();
    let screen = [s.origin.x, s.origin.y, s.size.width, s.size.height].map(f32::from);
    let [x, y, w, h] = fit_frame(saved, screen);
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

/// Parses `x,y,w,h`.
fn parse_frame(text: &str) -> Option<[f32; 4]> {
    let v: Vec<f32> = text
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    v.try_into()
        .ok()
        .filter(|f: &[f32; 4]| f.iter().all(|n| n.is_finite()))
}

/// Fits a saved frame `[x, y, w, h]` onto a screen: no smaller than the
/// minimum window, no larger than the screen, and wholly on it (the display
/// it was saved on may be gone).
fn fit_frame([x, y, w, h]: [f32; 4], [sx, sy, sw, sh]: [f32; 4]) -> [f32; 4] {
    let w = w.max(MIN_WINDOW.0).min(sw);
    let h = h.max(MIN_WINDOW.1).min(sh);
    let x = x.min(sx + sw - w).max(sx);
    let y = y.min(sy + sh - h).max(sy);
    [x, y, w, h]
}

/// Smallest useful window: room for the sidebar, a short line and results.
const MIN_WINDOW: (f32, f32) = (520., 320.);

fn open_main(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(initial_bounds(cx))),
        // On macOS the toolbar is drawn in the title bar, next to the window
        // buttons, like a native unified toolbar. Windows and Linux keep
        // their own title bar above it.
        titlebar: Some(TitlebarOptions {
            title: Some("Reckon".into()),
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: Some(point(px(13.), px(12.))),
        }),
        window_min_size: Some(size(px(MIN_WINDOW.0), px(MIN_WINDOW.1))),
        ..Default::default()
    };
    let font = FONT.with(|f| f.borrow().clone());
    let window = cx
        .open_window(options, |window, cx| {
            cx.new(|cx| Calc::new(window, font, false, cx))
        })
        .expect("open window");
    // RECKON_BACKGROUND opens the window without bringing Reckon to the
    // front, so a test run does not take the keyboard from another app.
    let activate = std::env::var_os("RECKON_BACKGROUND").is_none();
    window
        .update(cx, |view, window, cx| {
            window.focus(&view.focus_handle(cx));
            if activate {
                cx.activate(true);
            }
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
    // One line of text at the saved text size, plus the box's padding.
    let line = Calc::line_height_for(Calc::saved_font_size());
    let (w, h) = (px(680.), px(line + 28.));
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

fn set_initial_menus(cx: &mut App) {
    cx.set_menus(app_menus(
        &theme::ThemeChoice::load(),
        conf::get("sidebar").as_deref() != Some("hidden"),
        false,
    ));
}

/// When the process started, for the start-up timeline.
static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// With RECKON_DEBUG set, prints how long after start-up a stage was reached.
pub fn trace(stage: &str) {
    let start = START.get_or_init(std::time::Instant::now);
    if std::env::var_os("RECKON_DEBUG").is_some() {
        eprintln!(
            "reckon-app: {:7.1} ms  {stage}",
            start.elapsed().as_secs_f64() * 1000.
        );
    }
}

fn main() {
    trace("main");
    let quick_only = std::env::args().any(|a| a == "--quick");
    // `reckon-app --quick`: hand the request to a running app if there is one.
    #[cfg(unix)]
    if quick_only && hotkey::socket::ask_running_app() {
        return;
    }
    let app = Application::new().with_assets(assets::Assets);
    trace("platform ready");
    app.run(move |cx: &mut App| {
        trace("app running");
        bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        trace("keys bound");
        let font = editor_font(cx);
        trace("editor font chosen");
        if std::env::var_os("RECKON_DEBUG").is_some() {
            eprintln!("reckon-app: editor font {font}");
        }
        FONT.with(|f| *f.borrow_mut() = font);

        if quick_only {
            // No app running: show just the box and quit when it closes.
            set_initial_menus(cx);
            toggle_quick(cx);
            cx.on_window_closed(|cx| cx.quit()).detach();
            return;
        }

        // The window first: building the menu bar takes about 17 ms,
        // which is better spent after the first frame is drawn.
        open_main(cx);
        trace("main window opened");
        set_initial_menus(cx);
        trace("menus set");
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
            None => {
                eprintln!("reckon-app: quick_shortcut \"{shortcut}\" is not a valid shortcut")
            }
        }
        listen_quick(rx, cx);
        trace("shortcut registered");
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

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 4] = [0., 0., 1512., 982.];

    #[test]
    fn saved_frame_is_parsed() {
        assert_eq!(
            parse_frame("120, 90,700,500"),
            Some([120., 90., 700., 500.])
        );
        assert_eq!(parse_frame("120,90,700"), None);
        assert_eq!(parse_frame("a,b,c,d"), None);
        assert_eq!(parse_frame("1,2,3,NaN"), None);
    }

    #[test]
    fn saved_frame_is_kept_on_screen() {
        // A frame that fits is used as it is.
        assert_eq!(
            fit_frame([120., 90., 700., 500.], SCREEN),
            [120., 90., 700., 500.]
        );
        // One saved on a display that is gone moves back into view.
        assert_eq!(
            fit_frame([5000., 5000., 700., 500.], SCREEN),
            [812., 482., 700., 500.]
        );
        assert_eq!(
            fit_frame([-900., -50., 700., 500.], SCREEN),
            [0., 0., 700., 500.]
        );
        // Too small or too large is brought within the limits.
        assert_eq!(
            fit_frame([10., 10., 50., 50.], SCREEN),
            [10., 10., MIN_WINDOW.0, MIN_WINDOW.1]
        );
        assert_eq!(fit_frame([10., 10., 9000., 9000.], SCREEN), SCREEN);
    }
}
