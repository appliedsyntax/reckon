//! Sheets in the desktop app: the sidebar, switching, creating and deleting
//! sheets, choosing the folder, and picking up changes that a sync tool
//! (iCloud Drive, Dropbox, OneDrive, Syncthing, …) writes into the folder.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gpui::{
    div, prelude::*, px, ClickEvent, Context, Entity, Focusable, FontWeight, MouseButton,
    MouseDownEvent, PathPromptOptions, ScrollHandle, SharedString, Window,
};
use reckon_core::{paths, sheets, sheets::Sheet};

use super::{Calc, WELCOME};
use crate::input::{InputColors, InputEvent, TextInput};
use crate::theme::Theme;

pub(super) const SIDEBAR_W: f32 = 230.;
const POLL: Duration = Duration::from_millis(1500);

pub(super) struct SheetsState {
    pub dir: PathBuf,
    pub list: Vec<Sheet>,
    /// File name of the open sheet.
    pub current: String,
    /// Modification time of the open sheet after our last write or read, to
    /// tell our own saves apart from a sync tool's.
    pub seen_mtime: Option<SystemTime>,
    dir_mtime: Option<SystemTime>,
    polls: u32,
    pub open: bool,
    pub search: Entity<TextInput>,
    /// Keyboard selection in the filtered list while searching.
    pub selected: usize,
    /// The last deleted sheet, for Undo.
    pub trashed: Option<(PathBuf, String)>,
    pub scroll: ScrollHandle,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// The sheets folder: `sheets_folder` in app.conf, or `sheets` in the data
/// folder.
pub(super) fn sheets_dir() -> Option<PathBuf> {
    crate::conf::get("sheets_folder")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| paths::data_dir().map(|d| d.join("sheets")))
}

fn input_colors(t: &Theme) -> InputColors {
    InputColors {
        text: t.text,
        placeholder: t.muted,
        cursor: t.cursor,
        selection: t.selection,
    }
}

impl Calc {
    /// Opens the sheets folder (moving the old single `document.reckon` into
    /// it on first run) and returns the text of the sheet to show.
    pub(super) fn init_sheets(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let dir = sheets_dir()?;
        let legacy = paths::data_dir().map(|d| d.join("document.reckon"));
        let first = sheets::prepare(&dir, legacy.as_deref(), WELCOME).ok()?;
        let current = crate::conf::get("current_sheet")
            .filter(|f| dir.join(f).exists())
            .unwrap_or(first);
        let text = sheets::read(&dir, &current).ok()?;
        let search = cx.new(|cx| TextInput::new("Search sheets", input_colors(&self.theme), cx));
        let sub = cx.subscribe_in(&search, window, Self::on_search_event);
        self._subs.push(sub);
        self.sheets = Some(SheetsState {
            seen_mtime: mtime(&dir.join(&current)),
            dir_mtime: mtime(&dir),
            list: sheets::list(&dir),
            dir,
            current,
            polls: 0,
            open: crate::conf::get("sidebar").as_deref() != Some("hidden"),
            search,
            selected: 0,
            trashed: None,
            scroll: ScrollHandle::new(),
        });
        self.start_polling(cx);
        Some(text)
    }

    /// Writes the open sheet (called after every edit).
    pub(super) fn save_sheet(&mut self) {
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        if sheets::write(&s.dir, &s.current, &self.text).is_err() {
            return;
        }
        s.seen_mtime = mtime(&s.dir.join(&s.current));
        // Keep the sidebar entry current without re-reading the folder.
        let title = sheets::title_of(&self.text);
        if let Some(i) = s.list.iter().position(|x| x.file == s.current) {
            let mut entry = s.list.remove(i);
            entry.title = title;
            entry.preview = self
                .text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_string();
            s.list.insert(0, entry);
        }
    }

    fn visible_sheets(&self, cx: &Context<Self>) -> Vec<Sheet> {
        let Some(s) = &self.sheets else {
            return Vec::new();
        };
        let q = s.search.read(cx).text().to_string();
        sheets::search(&s.dir, &s.list, &q)
    }

    /// Switches to `file`, first renaming the sheet being left to match its
    /// title.
    pub(super) fn open_sheet(&mut self, file: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        if s.current == file {
            window.focus(&self.focus);
            return;
        }
        let Ok(text) = sheets::read(&s.dir, file) else {
            self.set_status("That sheet is no longer in the folder");
            self.reload_list(cx);
            return;
        };
        let _ = sheets::tidy(&s.dir, &s.current);
        s.current = file.to_string();
        s.seen_mtime = mtime(&s.dir.join(file));
        s.list = sheets::list(&s.dir);
        crate::conf::set(&[("current_sheet", file.to_string())]);
        self.load_text(text, cx);
        window.focus(&self.focus);
    }

    /// Replaces the document without recording undo history (a different
    /// sheet has its own history).
    fn load_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.text = text;
        let end = self.text.len();
        self.sel = end..end;
        self.reversed = false;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.typing = false;
        self.outputs = self.engine.evaluate(&self.text);
        self.autoscroll = super::AUTOSCROLL_FRAMES;
        cx.notify();
    }

    pub(super) fn new_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.sheets.as_ref().map(|s| s.dir.clone()) else {
            return;
        };
        match sheets::create(&dir, "# Untitled\n") {
            Ok(file) => {
                if let Some(s) = self.sheets.as_mut() {
                    s.open = true;
                    s.list = sheets::list(&dir);
                }
                self.open_sheet(&file, window, cx);
                // Select the title so typing replaces it.
                self.sel = 2..self.text.find('\n').unwrap_or(self.text.len());
                cx.notify();
            }
            Err(e) => self.set_status(format!("Could not create a sheet: {e}")),
        }
    }

    pub(super) fn trash_sheet(&mut self, file: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        let title = s
            .list
            .iter()
            .find(|x| x.file == file)
            .map(|x| x.title.clone())
            .unwrap_or_default();
        let was_current = s.current == file;
        let path = match sheets::trash(&s.dir, file) {
            Ok(p) => p,
            Err(e) => {
                self.set_status(format!("Could not delete: {e}"));
                cx.notify();
                return;
            }
        };
        s.trashed = Some((path, title.clone()));
        s.list = sheets::list(&s.dir);
        let next = s.list.first().map(|x| x.file.clone());
        let dir = s.dir.clone();
        if was_current {
            match next {
                Some(f) => {
                    // The old file is gone, so there is nothing to tidy.
                    s.current = f.clone();
                    s.seen_mtime = mtime(&dir.join(&f));
                    let text = sheets::read(&dir, &f).unwrap_or_default();
                    crate::conf::set(&[("current_sheet", f)]);
                    self.load_text(text, cx);
                }
                None => {
                    s.current = String::new();
                    self.new_sheet(window, cx);
                }
            }
        }
        self.set_status(format!("Deleted “{title}”"));
        cx.notify();
    }

    pub(super) fn undo_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        let Some((path, title)) = s.trashed.take() else {
            return;
        };
        match sheets::restore(&s.dir, &path) {
            Ok(file) => {
                s.list = sheets::list(&s.dir);
                self.set_status(format!("Restored “{title}”"));
                self.open_sheet(&file, window, cx);
            }
            Err(e) => self.set_status(format!("Could not restore: {e}")),
        }
    }

    pub(super) fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Use This Folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.use_folder(dir, window, cx))
                .ok();
        })
        .detach();
    }

    /// Moves to a new sheets folder. If it has no sheets yet, the current
    /// ones are copied into it (the old folder is left untouched).
    pub(super) fn use_folder(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        if dir == s.dir {
            return;
        }
        if sheets::list(&dir).is_empty() {
            for sheet in &s.list {
                let _ = std::fs::copy(s.dir.join(&sheet.file), dir.join(&sheet.file));
            }
        }
        let Ok(first) = sheets::prepare(&dir, None, WELCOME) else {
            self.set_status("Could not use that folder");
            return;
        };
        crate::conf::set(&[
            ("sheets_folder", dir.to_string_lossy().into_owned()),
            ("current_sheet", first.clone()),
        ]);
        s.dir = dir.clone();
        s.current = first.clone();
        s.list = sheets::list(&dir);
        s.seen_mtime = mtime(&dir.join(&first));
        s.dir_mtime = mtime(&dir);
        let text = sheets::read(&dir, &first).unwrap_or_default();
        self.load_text(text, cx);
        self.set_status(format!("Sheets folder: {}", dir.display()));
        window.focus(&self.focus);
    }

    pub(super) fn reveal_folder(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &self.sheets {
            cx.reveal_path(&s.dir.join(&s.current));
        }
    }

    fn reload_list(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheets.as_mut() {
            s.list = sheets::list(&s.dir);
            s.dir_mtime = mtime(&s.dir);
        }
        cx.notify();
    }

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL).await;
            if this.update(cx, |this, cx| this.poll_sheets(cx)).is_err() {
                break;
            }
        })
        .detach();
    }

    /// Picks up changes made by other devices: an edited open sheet is
    /// reloaded, and added, removed or renamed sheets refresh the list.
    fn poll_sheets(&mut self, cx: &mut Context<Self>) {
        self.save_window_bounds();
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        s.polls += 1;
        let path = s.dir.join(&s.current);
        let now = mtime(&path);
        if now.is_none() && !s.current.is_empty() {
            // Renamed or deleted elsewhere: fall back to the newest sheet.
            s.list = sheets::list(&s.dir);
            if let Some(first) = s.list.first().map(|x| x.file.clone()) {
                s.current = first.clone();
                s.seen_mtime = mtime(&s.dir.join(&first));
                let text = sheets::read(&s.dir, &first).unwrap_or_default();
                self.load_text(text, cx);
                self.set_status("The open sheet was moved or deleted on another device");
            }
            return;
        }
        if now != s.seen_mtime {
            s.seen_mtime = now;
            if let Ok(text) = sheets::read(&s.dir, &s.current) {
                if text != self.text {
                    // Keep undo: the other device's edit can be undone here.
                    self.snapshot();
                    self.text = text;
                    let n = self.text.len();
                    self.sel = self.sel.start.min(n)..self.sel.end.min(n);
                    while !self.text.is_char_boundary(self.sel.start) {
                        self.sel.start -= 1;
                    }
                    while !self.text.is_char_boundary(self.sel.end) {
                        self.sel.end -= 1;
                    }
                    self.outputs = self.engine.evaluate(&self.text);
                    self.set_status("Updated from another device");
                    cx.notify();
                }
            }
        }
        let s = self.sheets.as_mut().unwrap();
        let dir_now = mtime(&s.dir);
        // The folder's own time changes when files are added, removed or
        // renamed; edits to other sheets are caught every few polls.
        if dir_now != s.dir_mtime || s.polls.is_multiple_of(8) {
            s.dir_mtime = dir_now;
            let fresh = sheets::list(&s.dir);
            if fresh != s.list {
                s.list = fresh;
                cx.notify();
            }
        }
    }

    fn on_search_event(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let n = self.visible_sheets(cx).len();
        let Some(s) = self.sheets.as_mut() else {
            return;
        };
        match ev {
            InputEvent::Changed => s.selected = 0,
            InputEvent::Up => s.selected = s.selected.saturating_sub(1),
            InputEvent::Down => s.selected = (s.selected + 1).min(n.saturating_sub(1)),
            InputEvent::Submit => {
                let pick = self
                    .visible_sheets(cx)
                    .get(self.sheets.as_ref().unwrap().selected)
                    .map(|x| x.file.clone());
                if let Some(f) = pick {
                    self.open_sheet(&f, window, cx);
                }
            }
            InputEvent::Cancel => {
                let search = s.search.clone();
                search.update(cx, |i, cx| i.set_text("", cx));
                window.focus(&self.focus);
            }
        }
        cx.notify();
    }

    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheets.as_mut() {
            s.open = !s.open;
            crate::conf::set(&[("sidebar", if s.open { "shown" } else { "hidden" }.into())]);
            self.refresh_menus(cx);
            cx.notify();
        }
    }

    pub(super) fn find_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.sheets.as_mut() {
            s.open = true;
            let f = s.search.read(cx).focus_handle(cx);
            window.focus(&f);
            cx.notify();
        }
    }

    pub(super) fn render_sidebar(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let visible = self.visible_sheets(cx);
        let s = self.sheets.as_ref()?;
        if !s.open {
            return None;
        }
        let colors = input_colors(theme);
        s.search.update(cx, |i, _| i.colors = colors);
        let searching = !s.search.read(cx).text().is_empty();
        let folder: SharedString = s
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.dir.display().to_string())
            .into();
        let t = *theme;
        let rows = visible.iter().enumerate().map(|(i, sheet)| {
            let current = sheet.file == s.current;
            let keyed = searching && i == s.selected;
            let file = sheet.file.clone();
            let file2 = sheet.file.clone();
            let group: SharedString = format!("sheet-{i}").into();
            div()
                .id(("sheet", i))
                .group(group.clone())
                .mx(px(6.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .when(current || keyed, |d| d.bg(t.highlight))
                .when(!current && !keyed, |d| {
                    d.hover(|s| s.bg(t.highlight.opacity(0.5)))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_color(t.text)
                                .font_weight(if current {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(sheet.title.clone()),
                        )
                        .when(!sheet.preview.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_color(t.muted)
                                    .text_size(px(11.))
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(sheet.preview.clone()),
                            )
                        }),
                )
                .child(
                    div()
                        .id(("trash", i))
                        .invisible()
                        .group_hover(group, |s| s.visible())
                        .px(px(4.))
                        .rounded(px(4.))
                        .text_color(t.muted)
                        .hover(|s| s.text_color(t.text))
                        .child("✕")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.trash_sheet(&file2, window, cx)
                            }),
                        ),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_sheet(&file, window, cx)
                }))
        });
        let empty = visible.is_empty();
        let count = s.list.len();
        Some(
            div()
                .w(px(SIDEBAR_W))
                .flex_none()
                .h_full()
                .flex()
                .flex_col()
                .bg(t.status_bg)
                .border_r_1()
                .border_color(t.divider)
                .font_family(".SystemUIFont")
                .text_size(px(13.))
                .line_height(px(28.))
                .child(
                    div()
                        .flex_none()
                        .p(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_1()
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(6.))
                                .bg(t.bg)
                                .border_1()
                                .border_color(t.border)
                                .child(s.search.clone()),
                        )
                        .child(
                            div()
                                .id("new-sheet")
                                .flex_none()
                                .w(px(26.))
                                .h(px(26.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(16.))
                                .text_color(t.muted)
                                .cursor_pointer()
                                .hover(|s| s.bg(t.highlight).text_color(t.text))
                                .child("+")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.new_sheet(window, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .id("sheet-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&s.scroll)
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .children(rows)
                        .when(empty, |d| {
                            d.child(div().px(px(16.)).py(px(8.)).text_color(t.muted).child(
                                if searching {
                                    "No matching sheets"
                                } else {
                                    "No sheets"
                                },
                            ))
                        }),
                )
                .child(
                    div()
                        .id("sheets-folder")
                        .flex_none()
                        .h(px(26.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_t_1()
                        .border_color(t.divider)
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .cursor_pointer()
                        .hover(|s| s.text_color(t.text))
                        .child(
                            div()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(format!("▸ {folder}")),
                        )
                        .child(format!("{count}"))
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.choose_folder(window, cx)
                        })),
                ),
        )
    }
}
