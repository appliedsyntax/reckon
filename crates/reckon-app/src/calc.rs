//! The calculator view: a multi-line editor on the left and live results
//! on the right. The editor is a custom GPUI element (no web views, no
//! widget library) with IME support through `EntityInputHandler`.

use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use gpui::{
    actions, div, fill, point, prelude::*, px, relative, size, App, Bounds, ClickEvent,
    ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, Font, FontWeight, GlobalElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathPromptOptions,
    Pixels, Point, ScrollHandle, ShapedLine, SharedString, Style, Subscription, TextRun,
    UTF16Selection, Window,
};
use reckon_core::{paths, Engine, Hl, LineKind, LineOutput, Rates};

use crate::theme::Theme;

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
    ]
);

pub const LINE_H: Pixels = px(28.);
const FONT_SIZE: Pixels = px(15.);
const PAD_X: Pixels = px(18.);
const PAD_Y: Pixels = px(14.);
const RESULTS_W: Pixels = px(260.);
/// Scrolling can be clamped to the previous frame's content size, so keep
/// correcting for a few frames until the cursor really is visible.
const AUTOSCROLL_FRAMES: u8 = 3;

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
    status: Option<(SharedString, Instant)>,
    doc_path: Option<PathBuf>,
    _subs: Vec<Subscription>,
}

impl Calc {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let doc_path = paths::data_dir().map(|d| d.join("document.reckon"));
        let text = doc_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| WELCOME.to_string());
        let engine = Engine::with_user_data();
        let outputs = engine.evaluate(&text);
        let sub = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        let mut calc = Calc {
            focus: cx.focus_handle(),
            sel: text.len()..text.len(),
            text,
            reversed: false,
            marked: None,
            preferred_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
            engine,
            outputs,
            layouts: Vec::new(),
            bounds: None,
            scroll: ScrollHandle::new(),
            autoscroll: AUTOSCROLL_FRAMES,
            selecting: false,
            theme: Theme::for_appearance(window.appearance()),
            status: None,
            doc_path,
            _subs: vec![sub],
        };
        if !calc.engine.plugin_errors.is_empty() {
            let msg = format!("Plugin error: {}", calc.engine.plugin_errors[0]);
            calc.set_status(msg);
        }
        if calc.engine.rates_stale() {
            calc.refresh_rates(cx);
        }
        calc
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
            ((rel_y / LINE_H) as usize).min(rows - 1)
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
        if self.undo.len() > 500 {
            self.undo.remove(0);
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
        if let Some(p) = &self.doc_path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, &self.text);
        }
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
    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.replace(self.sel.clone(), "\n", cx);
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
        let top = bounds.top() + PAD_Y + LINE_H * row as f32;
        let x0 = bounds.left() + PAD_X + layout.x_for_index(r.start - start);
        let x1 =
            bounds.left() + PAD_X + layout.x_for_index(r.end.min(self.line_range(row).end) - start);
        Some(Bounds::from_corners(
            point(x0, top),
            point(x1, top + LINE_H),
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
        let rows = self.calc.read(cx).text.split('\n').count();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (PAD_Y * 2. + LINE_H * rows as f32).into();
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
        let font = window.text_style().font();
        let starts = calc.line_starts();
        let mut lines = Vec::with_capacity(starts.len());
        for (row, line) in calc.text.split('\n').enumerate() {
            let spans = calc
                .outputs
                .get(row)
                .map(|o| o.spans.as_slice())
                .unwrap_or(&[]);
            let runs = runs_for(line, spans, &theme, &font);
            lines.push(window.text_system().shape_line(
                SharedString::from(line.to_string()),
                FONT_SIZE,
                &runs,
                None,
            ));
        }
        let origin_x = bounds.left() + PAD_X;
        let row_top = |row: usize| bounds.top() + PAD_Y + LINE_H * row as f32;
        let mut quads = Vec::new();
        let cursor = calc.cursor();
        let cur_row = calc.row_of(cursor);
        if calc.sel.is_empty() {
            quads.push(fill(
                Bounds::from_corners(
                    point(bounds.left(), row_top(cur_row)),
                    point(bounds.right(), row_top(cur_row) + LINE_H),
                ),
                theme.active_line,
            ));
        } else {
            let (sr, er) = (calc.row_of(calc.sel.start), calc.row_of(calc.sel.end));
            for row in sr..=er {
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
                        point(origin_x + x1, row_top(row) + LINE_H),
                    ),
                    theme.selection,
                ));
            }
        }
        let cx_x = lines[cur_row].x_for_index(cursor - starts[cur_row]);
        let cursor_quad = fill(
            Bounds::new(
                point(origin_x + cx_x, row_top(cur_row) + px(5.)),
                size(px(2.), LINE_H - px(10.)),
            ),
            theme.cursor,
        );
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
        let focus = self.calc.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.calc.clone()),
            cx,
        );
        for q in pre.quads.drain(..) {
            window.paint_quad(q);
        }
        for (row, line) in pre.lines.iter().enumerate() {
            let origin = point(
                bounds.left() + PAD_X,
                bounds.top() + PAD_Y + LINE_H * row as f32,
            );
            line.paint(origin, LINE_H, window, cx).ok();
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
            let top = bounds.top() + PAD_Y + LINE_H * row as f32;
            let bottom = top + LINE_H + PAD_Y;
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
        let theme = Theme::for_appearance(window.appearance());
        self.theme = theme;
        if self
            .status
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs() > 6)
        {
            self.status = None;
        }
        let rows = self.text.split('\n').count();
        let cur_row = self.row_of(self.cursor());
        let results = (0..rows).map(|i| {
            let text = self.outputs.get(i).and_then(|o| o.text.clone());
            let active = i == cur_row && self.sel.is_empty();
            div()
                .id(("result", i))
                .h(LINE_H)
                .w_full()
                .flex()
                .items_center()
                .justify_end()
                .overflow_hidden()
                .when(active, |d| d.bg(theme.active_line))
                .text_color(theme.result)
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
            .key_context("Calc")
            .track_focus(&self.focus)
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
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .text_size(FONT_SIZE)
            .line_height(LINE_H)
            .child(
                div()
                    .id("scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .w_full()
                            .min_h_full()
                            .cursor(CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                            .on_mouse_move(cx.listener(Self::on_mouse_move))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(EditorElement { calc: cx.entity() }),
                            )
                            .child(div().w(px(1.)).bg(theme.divider))
                            .child(
                                div()
                                    .w(RESULTS_W)
                                    .flex_none()
                                    .pt(PAD_Y)
                                    .pr(PAD_X)
                                    .pl(px(12.))
                                    .cursor(CursorStyle::Arrow)
                                    .children(results),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(26.))
                    .flex_none()
                    .px(PAD_X)
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme.divider)
                    .text_size(px(11.))
                    .text_color(theme.muted)
                    .child(status)
                    .child(hint),
            )
    }
}
