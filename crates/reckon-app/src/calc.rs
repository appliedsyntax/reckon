//! The calculator view: a multi-line editor on the left and live results
//! on the right. The editor is a custom GPUI element (no web views, no
//! widget library) with IME support through `EntityInputHandler`.

use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use gpui::{
    actions, canvas, div, fill, point, prelude::*, px, relative, size, AnyElement, App, Bounds,
    ClickEvent, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, Font, FontWeight, GlobalElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathPromptOptions,
    Pixels, Point, ScrollHandle, ShapedLine, SharedString, Style, Subscription, TextRun,
    UTF16Selection, Window,
};
use reckon_core::{paths, Engine, Hl, LineKind, LineOutput, Rates};

use crate::theme::{Pattern, Theme, ThemeChoice, THEMES};

mod chrome;
mod sheets_ui;
use chrome::SettingsPanel;
use sheets_ui::SheetsState;

actions!(
    reckon,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteToLineStart,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocStart,
        DocEnd,
        SelectDocStart,
        SelectDocEnd,
        SelectAll,
        Newline,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        CopyResult,
        CopyAll,
        DeleteAll,
        WrapParens,
        Open,
        Export,
        Quit,
        ShowCharacterPalette,
        NewSheet,
        ToggleSidebar,
        FindSheet,
        ChooseSheetsFolder,
        RevealSheetsFolder,
        DismissQuick,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        OpenSettings,
        OpenAbout,
        CloseSettings,
        Minimize,
        ZoomWindow,
        ToggleFullScreen,
        CloseWindow,
        OpenHelp,
        ReportIssue,
        Hide,
        HideOthers,
        ShowAll,
        ToggleThemePicker,
        MatchSystemTheme,
        PickerUp,
        PickerDown,
        PickerConfirm,
        PickerCancel,
    ]
);

/// Switches to the theme at `index` in [`THEMES`] (used by the menu).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = reckon, no_json)]
pub struct SelectTheme {
    pub index: usize,
}

/// The open theme picker. Row 0 is "Match System"; row `i + 1` is
/// `THEMES[i]`. Moving the selection previews the theme; Escape restores
/// `original`.
struct Picker {
    selected: usize,
    original: ThemeChoice,
}

const PICKER_ROW_H: Pixels = px(32.);

/// Index among the picker list's children of picker row `row`: the list
/// is row 0, a "Light" heading, the light themes, a "Dark" heading, then
/// the dark themes.
fn picker_child(row: usize) -> usize {
    let first_dark = THEMES.iter().position(|t| t.dark).unwrap_or(THEMES.len());
    match row {
        0 => 0,
        r if r <= first_dark => r + 1,
        r => r + 2,
    }
}

/// A small preview of a theme: its background with a number-coloured and
/// a result-coloured mark. "Match System" shows its light and dark halves.
fn swatch_el(halves: [[gpui::Hsla; 3]; 2], theme: &Theme) -> impl IntoElement {
    let half = |[bg, number, result]: [gpui::Hsla; 3]| {
        div()
            .flex_1()
            .h_full()
            .bg(bg)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(3.))
            .child(div().size(px(5.)).rounded_full().bg(number))
            .child(div().w(px(9.)).h(px(4.)).rounded(px(2.)).bg(result))
    };
    let split = halves[0] != halves[1];
    div()
        .flex_none()
        .w(px(38.))
        .h(px(22.))
        .rounded(px(5.))
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .flex()
        .child(half(halves[0]))
        .when(split, |d| d.child(half(halves[1])))
}

/// Editor text size in pixels; the line height and results column scale
/// with it.
pub const DEFAULT_FONT_SIZE: f32 = 14.5;
const MIN_FONT_SIZE: f32 = 10.5;
const MAX_FONT_SIZE: f32 = 30.5;
const PAD_X: Pixels = px(18.);
const PAD_Y: Pixels = px(14.);
/// Scrolling can be clamped to the previous frame's content size, so keep
/// correcting for a few frames until the cursor really is visible.
const AUTOSCROLL_FRAMES: u8 = 3;
const MAX_UNDO_STEPS: usize = 500;
const MAX_UNDO_BYTES: usize = 8 * 1024 * 1024;
/// Rows beyond the visible ones that are still laid out and drawn.
const SHAPE_MARGIN: isize = 30;

const WELCOME: &str = "\
# Welcome to Reckon
// Type math in plain words. Results appear on the right.
// Click a result (or press ⇧⌘C) to copy it.

Price: $20 + 56 EUR
Discounted: prev - 5% discount
20% of $75
$50 as a % of $200

# Units
1 meter 20 cm
100 °F in celsius
3 GB in MB
60 mph in km/h

# Time
today + 2 weeks
New York time
2:30 pm HKT in Berlin

# Monthly budget
Rent: $1,200
Food: $450
Fun: $150
Total: sum
";

struct Snapshot {
    text: String,
    sel: Range<usize>,
}

pub struct Calc {
    focus: FocusHandle,
    text: String,
    sel: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    preferred_x: Option<Pixels>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    typing: bool,
    engine: Engine,
    outputs: Vec<LineOutput>,
    layouts: Vec<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    scroll: ScrollHandle,
    /// Frames left in which to bring the cursor into view (0 = done).
    autoscroll: u8,
    selecting: bool,
    theme: Theme,
    choice: ThemeChoice,
    picker: Option<Picker>,
    picker_scroll: ScrollHandle,
    status: Option<(SharedString, Instant)>,
    /// The sheets folder and sidebar (None in the quick-calc box).
    sheets: Option<SheetsState>,
    /// Editor text size in pixels (see `DEFAULT_FONT_SIZE`).
    font_size: f32,
    /// Whether the window is full screen (for the View menu's label).
    fullscreen: bool,
    /// The settings panel, when open.
    settings: Option<SettingsPanel>,
    /// Window size and position waiting to be written to app.conf.
    pending_bounds: Option<Bounds<Pixels>>,
    /// The title last given to the system window.
    window_title: SharedString,
    /// The quick-calc box: one line, nothing saved, Enter copies the result.
    quick: bool,
    /// Quick box only: the window has been active, so losing focus closes it.
    was_active: bool,
    /// Editor font family (see `editor_font` in main.rs).
    font: SharedString,
    _subs: Vec<Subscription>,
}

impl Calc {
    /// The main window (`quick == false`) or the quick-calc box.
    pub fn new(
        window: &mut Window,
        font: SharedString,
        quick: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::trace("view: start");
        let engine = Engine::with_user_data();
        crate::trace("view: engine ready");
        let mut subs = vec![cx.observe_window_appearance(window, |_, _, cx| cx.notify())];
        if quick {
            subs.push(
                cx.observe_window_activation(window, |this: &mut Calc, window, _| {
                    if window.is_window_active() {
                        this.was_active = true;
                    } else if this.was_active {
                        window.remove_window();
                    }
                }),
            );
        }
        let choice = ThemeChoice::load();
        let mut calc = Calc {
            focus: cx.focus_handle(),
            sel: 0..0,
            text: String::new(),
            reversed: false,
            marked: None,
            preferred_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
            engine,
            outputs: Vec::new(),
            layouts: Vec::new(),
            bounds: None,
            scroll: ScrollHandle::new(),
            autoscroll: AUTOSCROLL_FRAMES,
            selecting: false,
            theme: Theme::get(choice.resolve(window.appearance())),
            choice,
            picker: None,
            picker_scroll: ScrollHandle::new(),
            status: None,
            sheets: None,
            font_size: Self::saved_font_size(),
            settings: None,
            fullscreen: false,
            pending_bounds: None,
            window_title: SharedString::default(),
            quick,
            was_active: false,
            font,
            _subs: subs,
        };
        if !quick {
            let bounds_sub = cx.observe_window_bounds(window, |this: &mut Calc, window, cx| {
                this.note_window_bounds(window, cx)
            });
            let quit_sub = cx.on_app_quit(|this: &mut Calc, _| {
                this.save_window_bounds();
                async {}
            });
            calc._subs.extend([bounds_sub, quit_sub]);
            let text = calc
                .init_sheets(window, cx)
                .unwrap_or_else(|| WELCOME.to_string());
            crate::trace("view: sheets loaded");
            calc.sel = text.len()..text.len();
            calc.text = text;
            if !calc.engine.plugin_errors.is_empty() {
                let msg = format!("Plugin error: {}", calc.engine.plugin_errors[0]);
                calc.set_status(msg);
            }
            if calc.engine.rates_stale() {
                calc.refresh_rates(cx);
            }
        }
        calc.outputs = calc.engine.evaluate(&calc.text);
        crate::trace("view: evaluated");
        calc
    }

    // ------------------------------------------------------------ text size

    /// The text size saved in app.conf.
    pub fn saved_font_size() -> f32 {
        crate::conf::get("font_size")
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(DEFAULT_FONT_SIZE, |v| v.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
    }

    /// Height of one line at a text size: about 1.9 times the size (28 at
    /// the default).
    pub fn line_height_for(size: f32) -> f32 {
        (size * 1.93).round()
    }

    fn font_px(&self) -> Pixels {
        px(self.font_size)
    }

    pub fn line_h(&self) -> Pixels {
        px(Self::line_height_for(self.font_size))
    }

    /// Width of the results column, which grows with the text size.
    fn results_w(&self) -> Pixels {
        px((260. * self.font_size / DEFAULT_FONT_SIZE)
            .clamp(210., 440.)
            .round())
    }

    fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        // The quick box is sized when it opens; it follows the main window.
        if self.quick {
            return;
        }
        let size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        if (size - self.font_size).abs() > f32::EPSILON {
            self.font_size = size;
            crate::conf::set(&[("font_size", format!("{size}"))]);
            self.autoscroll = AUTOSCROLL_FRAMES;
        }
        self.set_status(format!("Text size {size}"));
        cx.notify();
    }

    // ------------------------------------------------------------ text model

    fn line_starts(&self) -> Vec<usize> {
        let mut v = vec![0];
        v.extend(self.text.match_indices('\n').map(|(i, _)| i + 1));
        v
    }

    fn row_of(&self, offset: usize) -> usize {
        self.line_starts()
            .partition_point(|s| *s <= offset)
            .saturating_sub(1)
    }

    fn line_range(&self, row: usize) -> Range<usize> {
        let starts = self.line_starts();
        let start = starts[row.min(starts.len() - 1)];
        let end = self.text[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(self.text.len());
        start..end
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.sel.start
        } else {
            self.sel.end
        }
    }

    fn prev_char(&self, o: usize) -> usize {
        self.text[..o]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_char(&self, o: usize) -> usize {
        self.text[o..]
            .chars()
            .next()
            .map(|c| o + c.len_utf8())
            .unwrap_or(self.text.len())
    }

    fn is_word(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '.'
    }

    fn word_left(&self, o: usize) -> usize {
        let mut i = o;
        let chars: Vec<(usize, char)> = self.text[..o].char_indices().collect();
        let mut k = chars.len();
        while k > 0 && !Self::is_word(chars[k - 1].1) {
            k -= 1;
            i = chars[k].0;
        }
        while k > 0 && Self::is_word(chars[k - 1].1) {
            k -= 1;
            i = chars[k].0;
        }
        i
    }

    fn word_right(&self, o: usize) -> usize {
        let mut it = self.text[o..].char_indices().peekable();
        let mut end = self.text.len();
        while let Some(&(_, c)) = it.peek() {
            if Self::is_word(c) {
                break;
            }
            it.next();
        }
        for (i, c) in it {
            if !Self::is_word(c) {
                end = o + i;
                break;
            }
        }
        end
    }

    fn word_range_at(&self, o: usize) -> Range<usize> {
        let start = self.text[..o]
            .char_indices()
            .rev()
            .take_while(|(_, c)| Self::is_word(*c))
            .last()
            .map(|(i, _)| i)
            .unwrap_or(o);
        let end = self.text[o..]
            .char_indices()
            .find(|(_, c)| !Self::is_word(*c))
            .map(|(i, _)| o + i)
            .unwrap_or(self.text.len());
        start..end
    }

    fn move_to(&mut self, o: usize, cx: &mut Context<Self>) {
        self.sel = o..o;
        self.reversed = false;
        self.preferred_x = None;
        self.typing = false;
        self.autoscroll = AUTOSCROLL_FRAMES;
        cx.notify();
    }

    fn select_to(&mut self, o: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.sel.start = o;
        } else {
            self.sel.end = o;
        }
        if self.sel.end < self.sel.start {
            self.reversed = !self.reversed;
            self.sel = self.sel.end..self.sel.start;
        }
        self.typing = false;
        self.autoscroll = AUTOSCROLL_FRAMES;
        cx.notify();
    }

    /// Offset one row up (-1) or down (+1), keeping the horizontal position.
    fn vertical(&mut self, delta: isize) -> usize {
        let cur = self.cursor();
        let row = self.row_of(cur);
        let rows = self.line_starts().len();
        let target = row as isize + delta;
        if target < 0 {
            return 0;
        }
        if target as usize >= rows {
            return self.text.len();
        }
        let start = self.line_range(row).start;
        let x = self.preferred_x.unwrap_or_else(|| {
            self.layouts
                .get(row)
                .map(|l| l.x_for_index(cur - start))
                .unwrap_or_default()
        });
        self.preferred_x = Some(x);
        let tr = self.line_range(target as usize);
        let col = self
            .layouts
            .get(target as usize)
            .map(|l| l.closest_index_for_x(x))
            .unwrap_or(0);
        (tr.start + col).min(tr.end)
    }

    fn index_at(&self, pos: Point<Pixels>) -> usize {
        let Some(b) = self.bounds else { return 0 };
        let rows = self.line_starts().len();
        let rel_y = pos.y - b.top() - PAD_Y;
        let row = if rel_y < px(0.) {
            0
        } else {
            ((rel_y / self.line_h()) as usize).min(rows - 1)
        };
        let r = self.line_range(row);
        let col = self
            .layouts
            .get(row)
            .map(|l| l.closest_index_for_x(pos.x - b.left() - PAD_X))
            .unwrap_or(0);
        (r.start + col).min(r.end)
    }

    fn snapshot(&mut self) {
        self.undo.push(Snapshot {
            text: self.text.clone(),
            sel: self.sel.clone(),
        });
        // Each step keeps a copy of the whole sheet, so bound the history by
        // size as well as by count: a long session on a large sheet must not
        // keep growing.
        let mut bytes: usize = self.undo.iter().map(|s| s.text.len()).sum();
        while self.undo.len() > 1 && (self.undo.len() > MAX_UNDO_STEPS || bytes > MAX_UNDO_BYTES) {
            bytes -= self.undo.remove(0).text.len();
        }
        self.redo.clear();
    }

    /// Replaces `range` with `new` and re-evaluates the document.
    fn replace(&mut self, range: Range<usize>, new: &str, cx: &mut Context<Self>) {
        let is_typing = new.chars().count() == 1 && range.is_empty() && !new.contains('\n');
        if !(is_typing && self.typing) {
            self.snapshot();
        }
        self.typing = is_typing;
        self.text.replace_range(range.clone(), new);
        let c = range.start + new.len();
        self.sel = c..c;
        self.reversed = false;
        self.marked = None;
        self.preferred_x = None;
        self.autoscroll = AUTOSCROLL_FRAMES;
        self.changed(cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.outputs = self.engine.evaluate(&self.text);
        self.save_sheet();
        cx.notify();
    }

    fn set_status(&mut self, msg: impl Into<SharedString>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.set_status(format!("Copied {text}"));
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        cx.notify();
    }

    fn refresh_rates(&mut self, cx: &mut Context<Self>) {
        self.set_status("Updating exchange rates…");
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { Rates::fetch() })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(r) => {
                        if let Some(p) = paths::rates_path() {
                            let _ = r.save(&p);
                        }
                        this.engine.set_rates(r);
                        this.outputs = this.engine.evaluate(&this.text);
                        this.set_status("Exchange rates updated");
                    }
                    Err(e) => this.set_status(format!("Exchange rates not updated: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn rates_label(&self) -> String {
        let r = self.engine.rates();
        match r.updated {
            None => format!("Rates: {}", r.source),
            Some(u) => {
                let mins = ((self.engine.now() - u as f64) / 60.0).max(0.0) as i64;
                let age = match mins {
                    0..=1 => "just now".to_string(),
                    2..=119 => format!("{mins} min ago"),
                    _ if mins < 48 * 60 => format!("{} h ago", mins / 60),
                    _ => format!("{} days ago", mins / 1440),
                };
                format!("Rates: {} · {age}", r.source)
            }
        }
    }

    // ------------------------------------------------------------ themes

    fn toggle_theme_picker(
        &mut self,
        _: &ToggleThemePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.is_some() {
            self.picker_cancel(&PickerCancel, window, cx);
            return;
        }
        let selected = if self.choice.follow_system {
            0
        } else {
            self.choice.fixed + 1
        };
        self.picker = Some(Picker {
            selected,
            original: self.choice,
        });
        self.picker_scroll.scroll_to_item(picker_child(selected));
        cx.notify();
    }

    /// Previews picker row `row` without saving it.
    fn preview_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.selected == row && self.choice == picker.original.previewed(row) {
            return;
        }
        picker.selected = row;
        self.choice = picker.original.previewed(row);
        cx.notify();
    }

    fn picker_up(&mut self, _: &PickerUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.picker {
            let row = p.selected.checked_sub(1).unwrap_or(THEMES.len());
            self.preview_row(row, cx);
            self.picker_scroll.scroll_to_item(picker_child(row));
        }
    }

    fn picker_down(&mut self, _: &PickerDown, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.picker {
            let row = (p.selected + 1) % (THEMES.len() + 1);
            self.preview_row(row, cx);
            self.picker_scroll.scroll_to_item(picker_child(row));
        }
    }

    fn picker_confirm(&mut self, _: &PickerConfirm, _: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_some() {
            self.apply_choice(self.choice, cx);
        }
    }

    fn picker_cancel(&mut self, _: &PickerCancel, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.picker.take() {
            self.choice = p.original;
            cx.notify();
        }
    }

    fn select_theme(&mut self, a: &SelectTheme, _: &mut Window, cx: &mut Context<Self>) {
        let mut choice = self.choice;
        choice.pick(a.index.min(THEMES.len() - 1));
        self.picker = None;
        self.apply_choice(choice, cx);
    }

    fn match_system_theme(&mut self, _: &MatchSystemTheme, _: &mut Window, cx: &mut Context<Self>) {
        let mut choice = self.choice;
        choice.follow_system = true;
        self.picker = None;
        self.apply_choice(choice, cx);
    }

    fn apply_choice(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        self.choice = choice;
        choice.save();
        // Native menus have no check state in GPUI, so rebuild them with a
        // tick next to the current theme.
        self.refresh_menus(cx);
        let msg = if choice.follow_system {
            format!(
                "Theme: match system ({} by day, {} by night)",
                THEMES[choice.light].name, THEMES[choice.dark].name
            )
        } else {
            format!("Theme: {}", THEMES[choice.fixed].name)
        };
        self.set_status(msg);
        cx.notify();
    }

    fn theme_label(&self, window: &Window) -> SharedString {
        THEMES[self.choice.resolve(window.appearance())].name.into()
    }

    fn render_picker(
        &self,
        picker: &Picker,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let current = picker.original.current_row();
        let first_dark = THEMES.iter().position(|t| t.dark).unwrap_or(THEMES.len());
        let group = |title: &'static str| {
            div()
                .px(px(14.))
                .pt(px(10.))
                .pb(px(4.))
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted)
                .child(title)
        };
        let row_el = |row: usize, cx: &mut Context<Self>| {
            let selected = row == picker.selected;
            let (name, blurb, swatch): (SharedString, SharedString, _) = if row == 0 {
                let (l, d) = (picker.original.light, picker.original.dark);
                (
                    "Match System".into(),
                    format!("{} by day, {} by night", THEMES[l].name, THEMES[d].name).into(),
                    [Theme::swatch(l), Theme::swatch(d)],
                )
            } else {
                let t = &THEMES[row - 1];
                let s = Theme::swatch(row - 1);
                (t.name.into(), t.blurb.into(), [s, s])
            };
            div()
                .id(("theme-row", row))
                .h(PICKER_ROW_H)
                .mx(px(6.))
                .px(px(8.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(10.))
                .cursor_pointer()
                .when(selected, |d| d.bg(theme.highlight))
                .child(swatch_el(swatch, theme))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_baseline()
                        .gap(px(8.))
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_none()
                                .text_color(theme.text)
                                .font_weight(FontWeight::MEDIUM)
                                .child(name),
                        )
                        .child(
                            div()
                                .text_color(theme.muted)
                                .text_size(px(11.))
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(blurb),
                        ),
                )
                .child(
                    div()
                        .w(px(14.))
                        .flex_none()
                        .text_color(theme.result)
                        .font_weight(FontWeight::BOLD)
                        .when(row == current, |d| d.child("✓")),
                )
                // Hovering previews; pressing keeps it (no wait for mouse-up).
                .on_mouse_move(
                    cx.listener(move |this, _: &MouseMoveEvent, _, cx| this.preview_row(row, cx)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        this.preview_row(row, cx);
                        this.picker_confirm(&PickerConfirm, window, cx);
                    }),
                )
        };
        // Children of the list, in order: row 0, "Light", light rows,
        // "Dark", dark rows (see `picker_child`).
        let mut list: Vec<AnyElement> = vec![row_el(0, cx).into_any_element()];
        list.push(group("LIGHT").into_any_element());
        for i in 0..first_dark {
            list.push(row_el(i + 1, cx).into_any_element());
        }
        list.push(group("DARK").into_any_element());
        for i in first_dark..THEMES.len() {
            list.push(row_el(i + 1, cx).into_any_element());
        }
        let hint = if cfg!(target_os = "macos") {
            "↑↓ preview · ↩ keep · esc cancel"
        } else {
            "↑↓ preview · Enter keep · Esc cancel"
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .child(
                // Backdrop: clicking outside the list cancels.
                div()
                    .id("theme-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(theme.bg.opacity(0.35))
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            this.picker_cancel(&PickerCancel, window, cx)
                        }),
                    ),
            )
            .child(
                div()
                    .id("theme-picker")
                    .occlude()
                    .mt(px(12.))
                    .w(px(440.))
                    .max_h(relative(0.86))
                    .flex()
                    .flex_col()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(10.))
                    .shadow_lg()
                    .overflow_hidden()
                    .font_family(".SystemUIFont")
                    .text_size(px(13.))
                    .line_height(px(28.))
                    .child(
                        div()
                            .flex_none()
                            .px(px(14.))
                            .py(px(9.))
                            .flex()
                            .justify_between()
                            .items_center()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_color(theme.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Theme"),
                            )
                            .child(div().text_color(theme.muted).text_size(px(11.)).child(hint)),
                    )
                    .child(
                        div()
                            .id("theme-list")
                            .flex_1()
                            .min_h_0()
                            .pt(px(6.))
                            .pb(px(8.))
                            .overflow_y_scroll()
                            .track_scroll(&self.picker_scroll)
                            .children(list),
                    ),
            )
    }

    // ------------------------------------------------------------ actions

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let p = self.prev_char(self.cursor());
            self.select_to(p, cx);
        }
        self.replace(self.sel.clone(), "", cx);
    }
    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let n = self.next_char(self.cursor());
            self.select_to(n, cx);
        }
        self.replace(self.sel.clone(), "", cx);
    }
    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let p = self.word_left(self.cursor());
            self.select_to(p, cx);
        }
        self.replace(self.sel.clone(), "", cx);
    }
    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sel.is_empty() {
            let s = self.line_range(self.row_of(self.cursor())).start;
            self.select_to(s, cx);
        }
        self.replace(self.sel.clone(), "", cx);
    }
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let o = if self.sel.is_empty() {
            self.prev_char(self.cursor())
        } else {
            self.sel.start
        };
        self.move_to(o, cx);
    }
    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let o = if self.sel.is_empty() {
            self.next_char(self.cursor())
        } else {
            self.sel.end
        };
        self.move_to(o, cx);
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let x = self.preferred_x;
        let o = self.vertical(-1);
        let keep = self.preferred_x.or(x);
        self.move_to(o, cx);
        self.preferred_x = keep;
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let x = self.preferred_x;
        let o = self.vertical(1);
        let keep = self.preferred_x.or(x);
        self.move_to(o, cx);
        self.preferred_x = keep;
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let p = self.prev_char(self.cursor());
        self.select_to(p, cx);
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.next_char(self.cursor());
        self.select_to(n, cx);
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.vertical(-1);
        self.select_to(o, cx);
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.vertical(1);
        self.select_to(o, cx);
    }
    fn word_left_a(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.word_left(self.cursor());
        self.move_to(o, cx);
    }
    fn word_right_a(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.word_right(self.cursor());
        self.move_to(o, cx);
    }
    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.word_left(self.cursor());
        self.select_to(o, cx);
    }
    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.word_right(self.cursor());
        self.select_to(o, cx);
    }
    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.line_range(self.row_of(self.cursor())).start;
        self.move_to(o, cx);
    }
    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.line_range(self.row_of(self.cursor())).end;
        self.move_to(o, cx);
    }
    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.line_range(self.row_of(self.cursor())).start;
        self.select_to(o, cx);
    }
    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        let o = self.line_range(self.row_of(self.cursor())).end;
        self.select_to(o, cx);
    }
    fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }
    fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.text.len();
        self.move_to(n, cx);
    }
    fn select_doc_start(&mut self, _: &SelectDocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }
    fn select_doc_end(&mut self, _: &SelectDocEnd, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.text.len();
        self.select_to(n, cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.sel = 0..self.text.len();
        self.reversed = false;
        cx.notify();
    }
    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick {
            // Enter copies the answer and closes the box.
            let result = self.outputs.iter().rev().find_map(|o| o.text.clone());
            if let Some(r) = result {
                cx.write_to_clipboard(ClipboardItem::new_string(r));
                window.remove_window();
            }
            return;
        }
        self.replace(self.sel.clone(), "\n", cx);
    }

    fn dismiss_quick(&mut self, _: &DismissQuick, window: &mut Window, _: &mut Context<Self>) {
        if self.quick {
            window.remove_window();
        }
    }

    // Sheets actions (the work is in sheets_ui.rs).
    fn new_sheet_a(&mut self, _: &NewSheet, window: &mut Window, cx: &mut Context<Self>) {
        self.new_sheet(window, cx);
    }
    fn toggle_sidebar_a(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_sidebar(cx);
    }
    fn find_sheet_a(&mut self, _: &FindSheet, window: &mut Window, cx: &mut Context<Self>) {
        self.find_sheet(window, cx);
    }
    fn choose_folder_a(
        &mut self,
        _: &ChooseSheetsFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_folder(window, cx);
    }
    fn reveal_folder_a(&mut self, _: &RevealSheetsFolder, _: &mut Window, cx: &mut Context<Self>) {
        self.reveal_folder(cx);
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.sel.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.sel.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.sel.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.sel.clone()].to_string(),
            ));
            self.replace(self.sel.clone(), "", cx);
        }
    }
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = cx.read_from_clipboard().and_then(|i| i.text()) {
            self.typing = false;
            self.replace(self.sel.clone(), &t.replace("\r\n", "\n"), cx);
        }
    }
    fn undo_a(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snapshot {
                text: std::mem::replace(&mut self.text, s.text),
                sel: self.sel.clone(),
            });
            self.sel = s.sel;
            self.typing = false;
            self.autoscroll = AUTOSCROLL_FRAMES;
            self.changed(cx);
        }
    }
    fn redo_a(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snapshot {
                text: std::mem::replace(&mut self.text, s.text),
                sel: self.sel.clone(),
            });
            self.sel = s.sel;
            self.typing = false;
            self.autoscroll = AUTOSCROLL_FRAMES;
            self.changed(cx);
        }
    }
    fn copy_result(&mut self, _: &CopyResult, _: &mut Window, cx: &mut Context<Self>) {
        let row = self.row_of(self.cursor());
        match self.outputs.get(row).and_then(|o| o.text.clone()) {
            Some(t) => self.copy_text(t, cx),
            None => {
                self.set_status("No result on this line");
                cx.notify();
            }
        }
    }
    fn copy_all(&mut self, _: &CopyAll, _: &mut Window, cx: &mut Context<Self>) {
        let mut out = String::new();
        for (line, o) in self.text.split('\n').zip(&self.outputs) {
            out.push_str(line);
            if let (LineKind::Expr, Some(t)) = (o.kind, &o.text) {
                out.push_str(" = ");
                out.push_str(t);
            }
            out.push('\n');
        }
        self.set_status("Copied all lines with results");
        cx.write_to_clipboard(ClipboardItem::new_string(out));
        cx.notify();
    }
    fn delete_all(&mut self, _: &DeleteAll, _: &mut Window, cx: &mut Context<Self>) {
        self.replace(0..self.text.len(), "", cx);
    }
    fn wrap_parens(&mut self, _: &WrapParens, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.sel.is_empty() {
            let r = self.line_range(self.row_of(self.cursor()));
            let line = &self.text[r.clone()];
            // Keep a leading "Label:" outside the parentheses.
            let skip = line
                .char_indices()
                .find(|(i, c)| {
                    *c == ':'
                        && !(line[..*i].ends_with(|d: char| d.is_ascii_digit())
                            && line[i + 1..].starts_with(|d: char| d.is_ascii_digit()))
                })
                .map(|(i, _)| i + 1)
                .unwrap_or(0);
            let body = &line[skip..];
            let lead = body.len() - body.trim_start().len();
            (r.start + skip + lead)..r.end
        } else {
            self.sel.clone()
        };
        let inner = self.text[range.clone()].to_string();
        self.typing = false;
        self.replace(range, &format!("({inner})"), cx);
    }
    fn open(&mut self, _: &Open, _: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = std::fs::read_to_string(&path);
            this.update(cx, |this, cx| match text {
                Ok(t) => {
                    this.typing = false;
                    this.replace(0..this.text.len(), &t, cx);
                    this.move_to(0, cx);
                    this.set_status(format!("Opened {}", path.display()));
                }
                Err(e) => {
                    this.set_status(format!("Could not open file: {e}"));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
    fn export(&mut self, _: &Export, _: &mut Window, cx: &mut Context<Self>) {
        let dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let rx = cx.prompt_for_new_path(&dir, Some("Untitled.reckon"));
        let text = self.text.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let res = std::fs::write(&path, text);
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => this.set_status(format!("Exported to {}", path.display())),
                    Err(e) => this.set_status(format!("Export failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ------------------------------------------------------------ mouse

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let ix = self.index_at(ev.position);
        match ev.click_count {
            2 => {
                let r = self.word_range_at(ix);
                self.sel = r;
                self.reversed = false;
                cx.notify();
            }
            n if n >= 3 => {
                let r = self.line_range(self.row_of(ix));
                self.sel = r;
                self.reversed = false;
                cx.notify();
            }
            _ if ev.modifiers.shift => self.select_to(ix, cx),
            _ => self.move_to(ix, cx),
        }
        self.selecting = true;
    }
    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }
    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let ix = self.index_at(ev.position);
            self.select_to(ix, cx);
        }
    }

    // ------------------------------------------------------------ utf16

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }
    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        let mut utf8 = 0;
        for ch in self.text.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }
    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }
    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }
}

impl Focusable for Calc {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for Calc {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.range_from_utf16(&range_utf16);
        actual.replace(self.range_to_utf16(&r));
        Some(self.text[r].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.sel),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.replace(range, new, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new: &str,
        new_sel_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.replace(range.clone(), new, cx);
        self.marked = (!new.is_empty()).then(|| range.start..range.start + new.len());
        if let Some(s) = new_sel_utf16 {
            let s = self.range_from_utf16(&s);
            self.sel = range.start + s.start..range.start + s.end;
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let r = self.range_from_utf16(&range_utf16);
        let row = self.row_of(r.start);
        let start = self.line_range(row).start;
        let layout = self.layouts.get(row)?;
        let top = bounds.top() + PAD_Y + self.line_h() * row as f32;
        let x0 = bounds.left() + PAD_X + layout.x_for_index(r.start - start);
        let x1 =
            bounds.left() + PAD_X + layout.x_for_index(r.end.min(self.line_range(row).end) - start);
        Some(Bounds::from_corners(
            point(x0, top),
            point(x1, top + self.line_h()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_at(p)))
    }
}

// ---------------------------------------------------------------- element

struct EditorElement {
    calc: Entity<Calc>,
}

/// The line between the text and the results: one hairline, or two for
/// themes like Ledger that draw a double rule.
fn divider(theme: Theme) -> impl IntoElement {
    let line = || div().w(px(1.)).h_full().bg(theme.divider);
    div()
        .flex_none()
        .flex()
        .gap(px(2.))
        .child(line())
        .when(theme.double_divider, |d| d.child(line()))
}

/// Paints the results-column tint and the theme's texture behind the
/// document. Only the visible part is drawn.
fn backdrop(theme: Theme, line_h: Pixels, results_w: Pixels) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let results_left = bounds.right() - results_w;
            window.paint_quad(fill(
                Bounds::from_corners(point(results_left, bounds.top()), bounds.bottom_right()),
                theme.results_bg,
            ));
            let visible = window.content_mask().bounds;
            let (top, bottom) = (
                visible.top().max(bounds.top()),
                visible.bottom().min(bounds.bottom()),
            );
            let hline = |window: &mut Window, y: Pixels, color: gpui::Hsla| {
                window.paint_quad(fill(
                    Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(1.))),
                    color,
                ));
            };
            match theme.pattern {
                Pattern::None => {}
                Pattern::Ruled => {
                    // A rule under each line of text.
                    let first = ((top - bounds.top() - PAD_Y) / line_h).floor().max(0.) as usize;
                    let mut row = first;
                    loop {
                        let y = bounds.top() + PAD_Y + line_h * (row + 1) as f32 - px(1.);
                        if y > bottom {
                            break;
                        }
                        hline(window, y, theme.rule);
                        row += 1;
                    }
                }
                Pattern::Grid => {
                    // Graph paper: a line every half row, stronger on row
                    // boundaries so each line of text sits in its own band.
                    let step = line_h / 2.;
                    let minor = theme.rule.opacity(0.55);
                    let origin = bounds.top() + PAD_Y;
                    let mut i = ((top - origin) / step).floor() as i64;
                    loop {
                        let y = origin + step * i as f32;
                        if y > bottom {
                            break;
                        }
                        if y >= bounds.top() {
                            hline(window, y, if i % 2 == 0 { theme.rule } else { minor });
                        }
                        i += 1;
                    }
                    let mut x = bounds.left() + PAD_X;
                    let mut k = 0;
                    while x < bounds.right() {
                        window.paint_quad(fill(
                            Bounds::from_corners(point(x, top), point(x + px(1.), bottom)),
                            if k % 2 == 0 { theme.rule } else { minor },
                        ));
                        x += step;
                        k += 1;
                    }
                }
                Pattern::Scanlines => {
                    let mut y = top - ((top - bounds.top()) % px(3.));
                    while y < bottom {
                        hline(window, y, theme.rule);
                        y += px(3.);
                    }
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

struct Prepaint {
    lines: Vec<ShapedLine>,
    quads: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn runs_for(line: &str, spans: &[(Range<usize>, Hl)], theme: &Theme, font: &Font) -> Vec<TextRun> {
    let base = TextRun {
        len: 0,
        font: font.clone(),
        color: theme.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut runs = Vec::new();
    let mut pos = 0;
    for (r, hl) in spans {
        let start = r.start.max(pos).min(line.len());
        let end = r.end.min(line.len());
        if end <= start || !line.is_char_boundary(start) || !line.is_char_boundary(end) {
            continue;
        }
        if start > pos {
            runs.push(TextRun {
                len: start - pos,
                ..base.clone()
            });
        }
        let mut f = font.clone();
        if *hl == Hl::Header {
            f.weight = FontWeight::BOLD;
        }
        runs.push(TextRun {
            len: end - start,
            font: f,
            color: theme.color(*hl),
            ..base.clone()
        });
        pos = end;
    }
    if pos < line.len() {
        runs.push(TextRun {
            len: line.len() - pos,
            ..base
        });
    }
    runs
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let calc = self.calc.read(cx);
        let rows = calc.text.split('\n').count();
        let line_h = calc.line_h();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (PAD_Y * 2. + line_h * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let calc = self.calc.read(cx);
        let theme = calc.theme;
        let (line_h, font_size) = (calc.line_h(), calc.font_px());
        let font = window.text_style().font();
        let starts = calc.line_starts();
        let cursor = calc.cursor();
        let cur_row = calc.row_of(cursor);
        let (sel_first, sel_last) = (calc.row_of(calc.sel.start), calc.row_of(calc.sel.end));
        // Only the lines that can be seen are shaped: those in view (plus a
        // margin, so a scroll step never reveals an unshaped line) and the
        // ones the cursor and the selection's ends are on, which caret
        // movement and the input method ask about. Every other row gets an
        // empty placeholder, so a sheet of thousands of lines costs no more
        // to draw than a short one.
        let view = window.content_mask().bounds;
        let row_at = |y: Pixels| ((y - bounds.top() - PAD_Y) / line_h).floor() as isize;
        let (first, last) = (
            row_at(view.top()) - SHAPE_MARGIN,
            row_at(view.bottom()) + SHAPE_MARGIN,
        );
        let wanted = |row: usize| {
            (first..=last).contains(&(row as isize))
                || row.abs_diff(cur_row) <= 1
                || row == sel_first
                || row == sel_last
        };
        let mut lines = Vec::with_capacity(starts.len());
        for (row, line) in calc.text.split('\n').enumerate() {
            if !wanted(row) {
                lines.push(ShapedLine::default());
                continue;
            }
            let spans = calc
                .outputs
                .get(row)
                .map(|o| o.spans.as_slice())
                .unwrap_or(&[]);
            let runs = runs_for(line, spans, &theme, &font);
            lines.push(window.text_system().shape_line(
                SharedString::from(line.to_string()),
                font_size,
                &runs,
                None,
            ));
        }
        let origin_x = bounds.left() + PAD_X;
        let row_top = |row: usize| bounds.top() + PAD_Y + line_h * row as f32;
        let mut quads = Vec::new();
        if calc.sel.is_empty() {
            quads.push(fill(
                Bounds::from_corners(
                    point(bounds.left(), row_top(cur_row)),
                    point(bounds.right(), row_top(cur_row) + line_h),
                ),
                theme.active_line,
            ));
        } else {
            let (sr, er) = (sel_first, sel_last);
            // Rows of the selection that are out of view need no highlight.
            for row in (sr..=er).filter(|row| wanted(*row)) {
                let ls = starts[row];
                let line = &lines[row];
                let line_len = calc.line_range(row).len();
                let x0 = if row == sr {
                    line.x_for_index(calc.sel.start - ls)
                } else {
                    px(0.)
                };
                let x1 = if row == er {
                    line.x_for_index(calc.sel.end - ls)
                } else {
                    line.x_for_index(line_len) + px(6.)
                };
                quads.push(fill(
                    Bounds::from_corners(
                        point(origin_x + x0, row_top(row)),
                        point(origin_x + x1, row_top(row) + line_h),
                    ),
                    theme.selection,
                ));
            }
        }
        let cx_x = lines[cur_row].x_for_index(cursor - starts[cur_row]);
        let cursor_bounds = if theme.block_cursor {
            // As wide as the character under it (or a digit at line end).
            let next = calc.next_char(cursor);
            let w = if next > cursor && next <= starts[cur_row] + calc.line_range(cur_row).len() {
                lines[cur_row].x_for_index(next - starts[cur_row]) - cx_x
            } else {
                font_size * 0.6
            };
            Bounds::new(
                point(origin_x + cx_x, row_top(cur_row) + px(4.)),
                size(w.max(px(2.)), line_h - px(8.)),
            )
        } else {
            Bounds::new(
                point(origin_x + cx_x, row_top(cur_row) + px(5.)),
                size(px(2.), line_h - px(10.)),
            )
        };
        let cursor_quad = fill(cursor_bounds, theme.cursor);
        Prepaint {
            lines,
            quads,
            cursor: Some(cursor_quad),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        pre: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let calc = self.calc.read(cx);
        let focus = calc.focus.clone();
        let line_h = calc.line_h();
        // While a panel is open, typing must not edit the document.
        if calc.picker.is_none() && calc.settings.is_none() {
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.calc.clone()),
                cx,
            );
        }
        for q in pre.quads.drain(..) {
            window.paint_quad(q);
        }
        for (row, line) in pre.lines.iter().enumerate() {
            let origin = point(
                bounds.left() + PAD_X,
                bounds.top() + PAD_Y + line_h * row as f32,
            );
            line.paint(origin, line_h, window, cx).ok();
        }
        let cursor = pre.cursor.take();
        if focus.is_focused(window) {
            if let Some(c) = &cursor {
                window.paint_quad(c.clone());
            }
        }
        let lines = std::mem::take(&mut pre.lines);
        let refresh = self.calc.update(cx, |calc, _| {
            calc.layouts = lines;
            calc.bounds = Some(bounds);
            if calc.autoscroll == 0 {
                return false;
            }
            // Keep the cursor line (plus a little margin) inside the viewport.
            let view = calc.scroll.bounds();
            let row = calc.row_of(calc.cursor());
            let top = bounds.top() + PAD_Y + calc.line_h() * row as f32;
            let bottom = top + calc.line_h() + PAD_Y;
            let mut off = calc.scroll.offset();
            if top < view.top() {
                off.y += view.top() - top;
            } else if bottom > view.bottom() {
                off.y -= bottom - view.bottom();
            } else {
                calc.autoscroll = 0;
                return false;
            }
            calc.autoscroll -= 1;
            calc.scroll.set_offset(off);
            true
        });
        if refresh {
            // `refresh()` is ignored while a frame is being drawn, so ask for
            // the redraw on the next frame instead.
            window.on_next_frame(|window, _| window.refresh());
        }
    }
}

impl Render for Calc {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| {
            crate::trace("first render");
            window.on_next_frame(|_, _| crate::trace("first frame shown"));
        });
        let theme = Theme::get(self.choice.resolve(window.appearance()));
        self.theme = theme;
        if self
            .status
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs() > 6)
        {
            self.status = None;
        }
        let picker = self
            .picker
            .as_ref()
            .map(|p| self.render_picker(p, &theme, cx));
        let theme_label = self.theme_label(window);
        let sidebar = self.render_sidebar(&theme, cx);
        let toolbar = (!self.quick).then(|| self.render_toolbar(&theme, window, cx));
        let settings = self
            .settings
            .as_ref()
            .map(|p| self.render_settings(p, &theme, cx));
        if !self.quick {
            let title: SharedString = format!("{} — Reckon", self.sheet_title()).into();
            if title != self.window_title {
                window.set_window_title(&title);
                self.window_title = title;
            }
        }
        let can_undo_delete = self.sheets.as_ref().is_some_and(|s| s.trashed.is_some())
            && self
                .status
                .as_ref()
                .is_some_and(|(m, _)| m.starts_with("Deleted"));

        let (line_h, results_w) = (self.line_h(), self.results_w());
        let rows = self.text.split('\n').count();
        let cur_row = self.row_of(self.cursor());
        // Only the result rows in view (plus a margin) become elements; the
        // rest are stood in for by a spacer above and one below.
        let scrolled = -self.scroll.offset().y;
        let row_at = |y: Pixels| ((y - PAD_Y) / line_h).floor() as isize;
        let first_row = (row_at(scrolled) - SHAPE_MARGIN).clamp(0, rows as isize) as usize;
        let last_row = (row_at(scrolled + window.viewport_size().height) + SHAPE_MARGIN + 1)
            .clamp(first_row as isize, rows as isize) as usize;
        let results = (first_row..last_row).map(|i| {
            let text = self.outputs.get(i).and_then(|o| o.text.clone());
            let active = i == cur_row && self.sel.is_empty();
            div()
                .id(("result", i))
                .h(line_h)
                .w_full()
                .flex()
                .items_center()
                .justify_end()
                .overflow_hidden()
                .when(active, |d| d.bg(theme.active_line))
                .text_color(theme.result)
                .font_weight(theme.result_weight)
                .when_some(text, |d, t| {
                    let copy = t.clone();
                    d.cursor_pointer()
                        .hover(|s| s.opacity(0.7))
                        .child(t)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.copy_text(copy.clone(), cx)
                        }))
                })
        });
        let status = self
            .status
            .as_ref()
            .map(|s| s.0.clone())
            .unwrap_or_else(|| self.rates_label().into());
        let hint = if cfg!(target_os = "macos") {
            "⇧⌘C copy result"
        } else {
            "Ctrl+Shift+C copy result"
        };

        div()
            .key_context(if self.settings.is_some() {
                "Settings"
            } else if self.picker.is_some() {
                "ThemePicker"
            } else {
                "Calc"
            })
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::zoom_reset))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::open_about))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::minimize))
            .on_action(cx.listener(Self::zoom_window))
            .on_action(cx.listener(Self::toggle_fullscreen))
            .on_action(cx.listener(Self::close_window))
            .on_action(cx.listener(Self::open_help))
            .on_action(cx.listener(Self::report_issue))
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_theme_picker))
            .on_action(cx.listener(Self::picker_up))
            .on_action(cx.listener(Self::picker_down))
            .on_action(cx.listener(Self::picker_confirm))
            .on_action(cx.listener(Self::picker_cancel))
            .on_action(cx.listener(Self::select_theme))
            .on_action(cx.listener(Self::match_system_theme))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::word_left_a))
            .on_action(cx.listener(Self::word_right_a))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::select_doc_start))
            .on_action(cx.listener(Self::select_doc_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo_a))
            .on_action(cx.listener(Self::redo_a))
            .on_action(cx.listener(Self::copy_result))
            .on_action(cx.listener(Self::copy_all))
            .on_action(cx.listener(Self::delete_all))
            .on_action(cx.listener(Self::wrap_parens))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::export))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::new_sheet_a))
            .on_action(cx.listener(Self::toggle_sidebar_a))
            .on_action(cx.listener(Self::find_sheet_a))
            .on_action(cx.listener(Self::choose_folder_a))
            .on_action(cx.listener(Self::reveal_folder_a))
            .on_action(cx.listener(Self::dismiss_quick))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .font_family(self.font.clone())
            .text_size(self.font_px())
            .line_height(line_h)
            .children(toolbar)
            .when(self.quick, |d| {
                d.rounded(px(10.))
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .children(sidebar)
                    .child(
                        div()
                            .id("scroll")
                            .flex_1()
                            // A column, so the document below can grow to
                            // the full height of the view when it is short
                            // (the results column and the paper pattern
                            // then reach the bottom of the window).
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .flex_row()
                                    .w_full()
                                    .flex_grow()
                                    .flex_shrink_0()
                                    .cursor(CursorStyle::IBeam)
                                    .child(backdrop(theme, line_h, results_w))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(Self::on_mouse_down),
                                    )
                                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                                    .on_mouse_up_out(
                                        MouseButton::Left,
                                        cx.listener(Self::on_mouse_up),
                                    )
                                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(EditorElement { calc: cx.entity() }),
                                    )
                                    .child(divider(theme))
                                    .child(
                                        div()
                                            .w(results_w)
                                            .flex_none()
                                            .pt(PAD_Y)
                                            .pr(PAD_X)
                                            .pl(px(12.))
                                            .cursor(CursorStyle::Arrow)
                                            .child(div().h(line_h * first_row as f32))
                                            .children(results)
                                            .child(div().h(line_h * (rows - last_row) as f32)),
                                    ),
                            ),
                    ),
            )
            .when(!self.quick, |d| {
                d.child(
                    div()
                        .h(px(26.))
                        .flex_none()
                        .px(PAD_X)
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_t_1()
                        .border_color(theme.divider)
                        .bg(theme.status_bg)
                        .font_family(".SystemUIFont")
                        .text_size(px(11.))
                        .line_height(px(18.))
                        .text_color(theme.muted)
                        .child(div().flex().items_center().gap(px(10.)).child(status).when(
                            can_undo_delete,
                            |d| {
                                d.child(
                                    div()
                                        .id("undo-delete")
                                        .text_color(theme.text)
                                        .cursor_pointer()
                                        .hover(|s| s.underline())
                                        .child("Undo")
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, window, cx| {
                                                this.undo_trash(window, cx)
                                            },
                                        )),
                                )
                            },
                        ))
                        .child(
                            div().flex().items_center().gap(px(14.)).child(hint).child(
                                div()
                                    .id("theme-button")
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(theme.text))
                                    .child(theme_label)
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.toggle_theme_picker(&ToggleThemePicker, window, cx)
                                    })),
                            ),
                        ),
                )
            })
            .children(picker)
            .children(settings)
    }
}
