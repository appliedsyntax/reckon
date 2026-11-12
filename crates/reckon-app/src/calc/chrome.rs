//! Window chrome: the toolbar, the settings panel, and the text-size and
//! window actions behind the standard menus and shortcuts.

use gpui::{
    div, prelude::*, px, svg, App, ClickEvent, Context, FontWeight, Keystroke, MouseButton,
    MouseDownEvent, ScrollHandle, SharedString, Subscription, Window, WindowBounds,
};
use reckon_core::sheets;

use super::{
    swatch_el, Calc, CloseSettings, CloseWindow, Minimize, OpenAbout, OpenHelp, OpenSettings,
    ReportIssue, ToggleFullScreen, ZoomIn, ZoomOut, ZoomReset, ZoomWindow, DEFAULT_FONT_SIZE,
};
use crate::hotkey;
use crate::theme::{Theme, THEMES};

pub(super) const TOOLBAR_H: f32 = 38.;
/// Room for the macOS close/minimise/zoom buttons at the toolbar's left.
const WINDOW_BUTTONS_W: f32 = 78.;

const HOME_URL: &str = "https://github.com/appliedsyntax/reckon";
const GUIDE_URL: &str = "https://github.com/appliedsyntax/reckon/blob/main/docs/user-guide.md";
const ISSUES_URL: &str = "https://github.com/appliedsyntax/reckon/issues";
const LICENSES_URL: &str =
    "https://github.com/appliedsyntax/reckon/blob/main/THIRD_PARTY_LICENSES.txt";

pub(super) struct SettingsPanel {
    /// Set while waiting for the user to press a new quick-calc shortcut:
    /// it intercepts every keystroke before key bindings see it.
    recorder: Option<Subscription>,
    /// Outcome of the last shortcut change.
    note: Option<SharedString>,
    scroll: ScrollHandle,
}

/// A small icon button for the toolbar and panels.
fn icon_button(
    id: &'static str,
    icon: &'static str,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (muted, text, hover) = (theme.muted, theme.text, theme.highlight);
    div()
        .id(id)
        .group(id)
        .w(px(30.))
        .h(px(24.))
        .rounded(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(
            svg()
                .path(icon)
                .size(px(16.))
                .text_color(muted)
                .group_hover(id, move |s| s.text_color(text)),
        )
        // Keep a double-click on a button from zooming the window.
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
        .on_click(move |ev, window, cx| {
            cx.stop_propagation();
            on_click(ev, window, cx)
        })
}

impl Calc {
    // ------------------------------------------------------------ actions

    pub(super) fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(self.font_size + 1., cx);
    }
    pub(super) fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(self.font_size - 1., cx);
    }
    pub(super) fn zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.set_font_size(DEFAULT_FONT_SIZE, cx);
    }

    pub(super) fn minimize(&mut self, _: &Minimize, window: &mut Window, _: &mut Context<Self>) {
        window.minimize_window();
    }
    pub(super) fn zoom_window(
        &mut self,
        _: &ZoomWindow,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.zoom_window();
    }
    pub(super) fn toggle_fullscreen(
        &mut self,
        _: &ToggleFullScreen,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.toggle_fullscreen();
    }
    pub(super) fn close_window(
        &mut self,
        _: &CloseWindow,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.save_sheet();
        self.save_window_bounds();
        window.remove_window();
    }

    pub(super) fn open_settings(
        &mut self,
        _: &OpenSettings,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_settings(false, cx);
    }
    pub(super) fn open_about(&mut self, _: &OpenAbout, _: &mut Window, cx: &mut Context<Self>) {
        self.show_settings(true, cx);
    }
    pub(super) fn close_settings(
        &mut self,
        _: &CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.take().is_some() {
            window.focus(&self.focus);
            cx.notify();
        }
    }
    pub(super) fn open_help(&mut self, _: &OpenHelp, _: &mut Window, cx: &mut Context<Self>) {
        cx.open_url(GUIDE_URL);
    }
    pub(super) fn report_issue(&mut self, _: &ReportIssue, _: &mut Window, cx: &mut Context<Self>) {
        cx.open_url(ISSUES_URL);
    }

    fn show_settings(&mut self, about: bool, cx: &mut Context<Self>) {
        if self.quick {
            return;
        }
        self.picker = None;
        let scroll = ScrollHandle::new();
        if about {
            scroll.scroll_to_bottom();
        }
        self.settings = Some(SettingsPanel {
            recorder: None,
            note: None,
            scroll,
        });
        cx.notify();
    }

    /// Rebuilds the menu bar (theme tick, Show/Hide Sidebar).
    pub(super) fn refresh_menus(&self, cx: &mut Context<Self>) {
        let sidebar_open = self.sheets.as_ref().is_some_and(|s| s.open);
        cx.set_menus(crate::app_menus(
            &self.choice,
            sidebar_open,
            self.fullscreen,
        ));
    }

    // ------------------------------------------------------------ window

    /// Remembers the window's size and position for the next launch.
    pub(super) fn note_window_bounds(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let WindowBounds::Windowed(b) = window.window_bounds() {
            self.pending_bounds = Some(b);
        }
        let fullscreen = window.is_fullscreen();
        if fullscreen != self.fullscreen {
            self.fullscreen = fullscreen;
            self.refresh_menus(cx);
        }
        // The toolbar leaves room for the window buttons only when windowed.
        cx.notify();
    }

    pub(super) fn save_window_bounds(&mut self) {
        if let Some(b) = self.pending_bounds.take() {
            let v = |p: gpui::Pixels| f32::from(p).round();
            crate::conf::set(&[(
                "window",
                format!(
                    "{},{},{},{}",
                    v(b.origin.x),
                    v(b.origin.y),
                    v(b.size.width),
                    v(b.size.height)
                ),
            )]);
        }
    }

    /// The open sheet's title, for the toolbar and the window title.
    pub(super) fn sheet_title(&self) -> SharedString {
        match &self.sheets {
            Some(_) => sheets::title_of(&self.text).into(),
            None => "Reckon".into(),
        }
    }

    // ------------------------------------------------------------ shortcut

    fn shortcut_label() -> String {
        let s = crate::conf::get("quick_shortcut").unwrap_or_else(|| hotkey::DEFAULT.into());
        hotkey::parse(&s).map_or(s, |sc| hotkey::label(&sc))
    }

    fn start_recording(&mut self, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let recorder = cx.intercept_keystrokes(move |ev, _, cx| {
            cx.stop_propagation();
            let keystroke = ev.keystroke.clone();
            this.update(cx, |this, cx| this.recorded(keystroke, cx))
                .ok();
        });
        if let Some(s) = self.settings.as_mut() {
            s.recorder = Some(recorder);
            s.note = None;
        }
        cx.notify();
    }

    /// Handles the keystroke pressed while recording a shortcut.
    fn recorded(&mut self, k: Keystroke, cx: &mut Context<Self>) {
        let Some(panel) = self.settings.as_mut() else {
            return;
        };
        let m = k.modifiers;
        if k.key == "escape" && !(m.control || m.alt || m.platform) {
            panel.recorder = None;
            cx.notify();
            return;
        }
        let mut parts: Vec<&str> = Vec::new();
        if m.control {
            parts.push("ctrl");
        }
        if m.alt {
            parts.push("alt");
        }
        if m.shift {
            parts.push("shift");
        }
        if m.platform {
            parts.push("cmd");
        }
        parts.push(&k.key);
        let text = parts.join("-");
        let Some(sc) = hotkey::parse(&text) else {
            panel.note = Some(
                "Use a letter, digit or Space together with Control, Option or Command.".into(),
            );
            cx.notify();
            return;
        };
        panel.recorder = None;
        panel.note = Some(match hotkey::rebind(sc) {
            Ok(live) => {
                crate::conf::set(&[("quick_shortcut", text)]);
                if live {
                    format!("Quick calc is now on {}.", hotkey::label(&sc)).into()
                } else {
                    "Saved. Restart Reckon to use the new shortcut.".into()
                }
            }
            Err(e) => format!("Not changed: {e}.").into(),
        });
        cx.notify();
    }

    // ------------------------------------------------------------ toolbar

    pub(super) fn render_toolbar(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mac = cfg!(target_os = "macos");
        // The macOS window buttons sit over the toolbar's left edge (they
        // are hidden in full screen).
        let left_pad = if mac && !window.is_fullscreen() {
            WINDOW_BUTTONS_W
        } else {
            8.
        };
        let this = cx.weak_entity();
        let toggle = {
            let this = this.clone();
            icon_button(
                "toggle-sidebar",
                "icons/sidebar.svg",
                theme,
                move |_, _, cx| {
                    this.update(cx, |this, cx| this.toggle_sidebar(cx)).ok();
                },
            )
        };
        let new_sheet = {
            let this = this.clone();
            icon_button(
                "toolbar-new",
                "icons/plus.svg",
                theme,
                move |_, window, cx| {
                    this.update(cx, |this, cx| this.new_sheet(window, cx)).ok();
                },
            )
        };
        let settings = icon_button(
            "toolbar-settings",
            "icons/gear.svg",
            theme,
            move |_, _, cx| {
                this.update(cx, |this, cx| this.show_settings(false, cx))
                    .ok();
            },
        );
        div()
            .id("toolbar")
            .h(px(TOOLBAR_H))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(left_pad))
            .pr(px(8.))
            .bg(theme.status_bg)
            .border_b_1()
            .border_color(theme.divider)
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .line_height(px(20.))
            // Double-clicking the bar does what the system setting says
            // (zoom or minimise), as on a native title bar.
            .on_click(|ev: &ClickEvent, window, _| {
                if ev.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(div().flex_1().flex().items_center().child(toggle))
            .child(
                div()
                    .flex_none()
                    .max_w(px(360.))
                    .text_color(theme.muted)
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(self.sheet_title()),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(2.))
                    .child(new_sheet)
                    .child(settings),
            )
    }

    // ------------------------------------------------------------ settings

    pub(super) fn render_settings(
        &self,
        panel: &SettingsPanel,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = *theme;
        let heading = |title: &'static str| {
            div()
                .pt(px(18.))
                .pb(px(8.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.muted)
                .child(title)
        };
        let row = |label: &'static str| {
            div()
                .min_h(px(34.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(div().flex_none().text_color(t.text).child(label))
        };
        let button = |id: &'static str, label: SharedString| {
            div()
                .id(id)
                .flex_none()
                .px(px(10.))
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border)
                .bg(t.bg)
                .text_color(t.text)
                .text_size(px(12.))
                .cursor_pointer()
                .hover(move |s| s.bg(t.highlight))
                .child(label)
        };
        let link = |id: &'static str, label: &'static str, url: &'static str| {
            div()
                .id(id)
                .text_color(t.result)
                .cursor_pointer()
                .hover(|s| s.underline())
                .child(label)
                .on_click(move |_: &ClickEvent, _, cx| cx.open_url(url))
        };

        // Theme tiles.
        let choice = self.choice;
        let tile = |id: usize, name: SharedString, halves, selected: bool| {
            div()
                .id(("theme-tile", id))
                .w(px(92.))
                .py(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(if selected { t.result } else { t.surface })
                .when(selected, |d| d.bg(t.highlight))
                .when(!selected, |d| {
                    d.hover(move |s| s.bg(t.highlight.opacity(0.5)))
                })
                .flex()
                .flex_col()
                .items_center()
                .gap(px(5.))
                .cursor_pointer()
                .child(swatch_el(halves, &t))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.text)
                        .when(selected, |d| d.font_weight(FontWeight::SEMIBOLD))
                        .child(name),
                )
        };
        let system_tile = tile(
            0,
            "Match System".into(),
            [Theme::swatch(choice.light), Theme::swatch(choice.dark)],
            choice.follow_system,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            let mut c = this.choice;
            c.follow_system = true;
            this.apply_choice(c, cx);
        }));
        let theme_tiles = |dark: bool, cx: &mut Context<Self>| {
            THEMES
                .iter()
                .enumerate()
                .filter(|(_, th)| th.dark == dark)
                .map(|(i, th)| {
                    let s = Theme::swatch(i);
                    tile(
                        i + 1,
                        th.name.into(),
                        [s, s],
                        !choice.follow_system && choice.fixed == i,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            let mut c = this.choice;
                            c.pick(i);
                            this.apply_choice(c, cx);
                        },
                    ))
                })
                .collect::<Vec<_>>()
        };
        let tiles = |children: Vec<gpui::Stateful<gpui::Div>>| {
            div().flex().flex_wrap().gap(px(6.)).children(children)
        };
        let mut light = vec![system_tile];
        light.extend(theme_tiles(false, cx));
        let dark = theme_tiles(true, cx);

        // Sheets folder.
        let folder: SharedString = self
            .sheets
            .as_ref()
            .map(|s| s.dir.display().to_string())
            .unwrap_or_default()
            .into();
        let reveal = if cfg!(target_os = "macos") {
            "Show in Finder"
        } else {
            "Show Folder"
        };

        // Quick calc.
        let recording = panel.recorder.is_some();
        let shortcut_control = if cfg!(any(target_os = "macos", windows)) {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_color(if recording { t.muted } else { t.text })
                        .font_weight(FontWeight::MEDIUM)
                        .child(if recording {
                            "Press the new shortcut (Esc cancels)".to_string()
                        } else {
                            Self::shortcut_label()
                        }),
                )
                .when(!recording, |d| {
                    d.child(button("change-shortcut", "Change…".into()).on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.start_recording(cx)),
                    ))
                })
        } else {
            div()
                .text_color(t.muted)
                .text_size(px(12.))
                .child("Bind `reckon-app --quick` in your desktop's keyboard settings")
        };

        let mods = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl "
        };
        let body = div()
            .id("settings-body")
            .flex_1()
            .min_h_0()
            .px(px(22.))
            .pb(px(22.))
            .overflow_y_scroll()
            .track_scroll(&panel.scroll)
            .child(heading("THEME"))
            .child(tiles(light))
            .child(div().h(px(6.)))
            .child(tiles(dark))
            .child(heading("TEXT"))
            .child(
                row("Text size").child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_color(t.muted)
                                .text_size(px(11.))
                                .child(format!("{mods}+  {mods}−  {mods}0")),
                        )
                        .child(button("font-smaller", "−".into()).on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| this.zoom_out(&ZoomOut, window, cx),
                        )))
                        .child(
                            div()
                                .w(px(40.))
                                .flex()
                                .justify_center()
                                .text_color(t.text)
                                .child(format!("{}", self.font_size)),
                        )
                        .child(button("font-larger", "+".into()).on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| this.zoom_in(&ZoomIn, window, cx),
                        )))
                        .child(button("font-reset", "Reset".into()).on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| {
                                this.zoom_reset(&ZoomReset, window, cx)
                            },
                        ))),
                ),
            )
            .child(heading("SHEETS"))
            .child(
                row("Folder").child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .min_w_0()
                                .text_color(t.muted)
                                .text_size(px(12.))
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(folder),
                        )
                        .child(button("choose-folder", "Choose…".into()).on_click(cx.listener(
                            |this, _: &ClickEvent, window, cx| this.choose_folder(window, cx),
                        )))
                        .child(button("reveal-folder", reveal.into()).on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| this.reveal_folder(cx),
                        ))),
                ),
            )
            .child(
                div()
                    .text_color(t.muted)
                    .text_size(px(11.))
                    .child("Pick a folder in iCloud Drive, Dropbox or OneDrive to sync sheets between devices."),
            )
            .child(heading("QUICK CALC"))
            .child(row("Shortcut from any app").child(shortcut_control))
            .when_some(panel.note.clone(), |d, note| {
                d.child(div().text_color(t.muted).text_size(px(11.)).child(note))
            })
            .child(heading("ABOUT"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_color(t.text)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Reckon {}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(
                        div()
                            .text_color(t.muted)
                            .text_size(px(12.))
                            .child("A natural-language calculator. Open source under the MIT License."),
                    )
                    .child(
                        div()
                            .pt(px(4.))
                            .flex()
                            .flex_wrap()
                            .gap_x(px(16.))
                            .gap_y(px(4.))
                            .text_size(px(12.))
                            .child(link("about-home", "Website", HOME_URL))
                            .child(link("about-guide", "User guide", GUIDE_URL))
                            .child(link("about-licenses", "Third-party licenses", LICENSES_URL))
                            .child(link(
                                "about-rates",
                                "Rates By Exchange Rate API",
                                "https://www.exchangerate-api.com",
                            ))
                            .child(link(
                                "about-crypto",
                                "Crypto prices powered by CoinGecko",
                                "https://www.coingecko.com",
                            )),
                    ),
            );

        let close = {
            let this = cx.weak_entity();
            icon_button(
                "settings-close",
                "icons/close.svg",
                theme,
                move |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.close_settings(&CloseSettings, window, cx)
                    })
                    .ok();
                },
            )
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .child(
                // Backdrop: clicking outside the panel closes it.
                div()
                    .id("settings-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(theme.bg.opacity(0.5))
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            this.close_settings(&CloseSettings, window, cx)
                        }),
                    ),
            )
            .child(
                div()
                    .id("settings-panel")
                    .occlude()
                    .w(px(560.))
                    .max_w(gpui::relative(0.94))
                    .max_h(gpui::relative(0.9))
                    .flex()
                    .flex_col()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.))
                    .shadow_lg()
                    .overflow_hidden()
                    .font_family(".SystemUIFont")
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .child(
                        div()
                            .flex_none()
                            .h(px(44.))
                            .pl(px(22.))
                            .pr(px(10.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_color(theme.text)
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            )
                            .child(close),
                    )
                    .child(body),
            )
    }
}
