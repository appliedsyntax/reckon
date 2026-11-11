//! A small single-line text field (GPUI has none built in), used for the
//! sheet search box. It supports IME composition through
//! `EntityInputHandler`, like the main editor.

use std::ops::Range;

use gpui::{
    actions, canvas, div, fill, point, prelude::*, px, size, App, Bounds, ClipboardItem, Context,
    ElementInputHandler, EventEmitter, FocusHandle, Focusable, Hsla, Pixels, Point, ShapedLine,
    SharedString, TextRun, UTF16Selection, Window,
};

actions!(
    text_input,
    [
        Backspace, Delete, Left, Right, Home, End, SelectAll, Copy, Cut, Paste, Submit, Cancel, Up,
        Down
    ]
);

/// What the owner of a [`TextInput`] hears about.
pub enum InputEvent {
    Changed,
    Submit,
    Cancel,
    Up,
    Down,
}

pub struct TextInput {
    focus: FocusHandle,
    text: String,
    sel: Range<usize>,
    marked: Option<Range<usize>>,
    placeholder: SharedString,
    pub colors: InputColors,
    layout: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Copy)]
pub struct InputColors {
    pub text: Hsla,
    pub placeholder: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
}

impl EventEmitter<InputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Key bindings for every text field (context "TextInput").
pub fn bindings() -> Vec<gpui::KeyBinding> {
    use gpui::KeyBinding as K;
    let m = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let ctx = Some("TextInput");
    vec![
        K::new("backspace", Backspace, ctx),
        K::new("delete", Delete, ctx),
        K::new("left", Left, ctx),
        K::new("right", Right, ctx),
        K::new("home", Home, ctx),
        K::new("end", End, ctx),
        K::new("up", Up, ctx),
        K::new("down", Down, ctx),
        K::new("enter", Submit, ctx),
        K::new("escape", Cancel, ctx),
        K::new(&format!("{m}-a"), SelectAll, ctx),
        K::new(&format!("{m}-c"), Copy, ctx),
        K::new(&format!("{m}-x"), Cut, ctx),
        K::new(&format!("{m}-v"), Paste, ctx),
    ]
}

impl TextInput {
    pub fn new(placeholder: &str, colors: InputColors, cx: &mut Context<Self>) -> Self {
        TextInput {
            focus: cx.focus_handle(),
            text: String::new(),
            sel: 0..0,
            marked: None,
            placeholder: placeholder.to_string().into(),
            colors,
            layout: None,
            bounds: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.text = text.to_string();
        self.sel = self.text.len()..self.text.len();
        self.marked = None;
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn replace(&mut self, range: Range<usize>, new: &str, cx: &mut Context<Self>) {
        // One line only: pasted newlines become spaces.
        let new = new.replace(['\n', '\r'], " ");
        self.text.replace_range(range.clone(), &new);
        let c = range.start + new.len();
        self.sel = c..c;
        self.marked = None;
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn prev(&self, o: usize) -> usize {
        self.text[..o]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self, o: usize) -> usize {
        self.text[o..]
            .chars()
            .next()
            .map_or(self.text.len(), |c| o + c.len_utf8())
    }

    fn move_to(&mut self, o: usize, cx: &mut Context<Self>) {
        self.sel = o..o;
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let r = if self.sel.is_empty() {
            self.prev(self.sel.start)..self.sel.end
        } else {
            self.sel.clone()
        };
        self.replace(r, "", cx);
    }
    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let r = if self.sel.is_empty() {
            self.sel.start..self.next(self.sel.end)
        } else {
            self.sel.clone()
        };
        self.replace(r, "", cx);
    }
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let o = if self.sel.is_empty() {
            self.prev(self.sel.start)
        } else {
            self.sel.start
        };
        self.move_to(o, cx);
    }
    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let o = if self.sel.is_empty() {
            self.next(self.sel.end)
        } else {
            self.sel.end
        };
        self.move_to(o, cx);
    }
    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }
    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.text.len(), cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.sel = 0..self.text.len();
        cx.notify();
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.sel.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.sel.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, w: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, w, cx);
        self.replace(self.sel.clone(), "", cx);
    }
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.replace(self.sel.clone(), &t, cx);
        }
    }
    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Submit);
    }
    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Cancel);
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Up);
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Down);
    }

    // UTF-16 conversions for the platform input system.
    fn offset_to_utf16(&self, o: usize) -> usize {
        self.text[..o].chars().map(char::len_utf16).sum()
    }
    fn utf16_to_offset(&self, u: usize) -> usize {
        let mut n = 0;
        for (i, c) in self.text.char_indices() {
            if n >= u {
                return i;
            }
            n += c.len_utf16();
        }
        self.text.len()
    }
    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }
    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.utf16_to_offset(r.start)..self.utf16_to_offset(r.end)
    }
}

impl gpui::EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.range_from_utf16(&r);
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
            reversed: false,
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
        r: Option<Range<usize>>,
        new: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = r
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.replace(r, new, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        new: &str,
        new_sel: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = r
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.replace(r.clone(), new, cx);
        self.marked = (!new.is_empty()).then(|| r.start..r.start + new.len());
        if let Some(s) = new_sel {
            let s = self.range_from_utf16(&s);
            self.sel = r.start + s.start..r.start + s.end;
        }
    }

    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let r = self.range_from_utf16(&r);
        let l = self.layout.as_ref()?;
        Some(Bounds::from_corners(
            point(bounds.left() + l.x_for_index(r.start), bounds.top()),
            point(bounds.left() + l.x_for_index(r.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let (l, b) = (self.layout.as_ref()?, self.bounds?);
        Some(self.offset_to_utf16(l.closest_index_for_x(p.x - b.left())))
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let focused = self.focus.is_focused(window);
        let empty = self.text.is_empty();
        let shown: SharedString = if empty {
            self.placeholder.clone()
        } else {
            self.text.clone().into()
        };
        let colors = self.colors;
        let sel = self.sel.clone();
        let marked = self.marked.clone();
        let focus = self.focus.clone();
        div()
            .key_context("TextInput")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, ev: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    if let (Some(l), Some(b)) = (&this.layout, this.bounds) {
                        let o = l.closest_index_for_x(ev.position.x - b.left());
                        this.move_to(o, cx);
                    }
                }),
            )
            .cursor(gpui::CursorStyle::IBeam)
            .w_full()
            .h(px(20.))
            .child(
                canvas(
                    move |_, window, _| {
                        let style = window.text_style();
                        let run = TextRun {
                            len: shown.len(),
                            font: style.font(),
                            color: if empty {
                                colors.placeholder
                            } else {
                                colors.text
                            },
                            background_color: None,
                            underline: marked.as_ref().map(|_| gpui::UnderlineStyle {
                                thickness: px(1.),
                                color: Some(colors.text),
                                wavy: false,
                            }),
                            strikethrough: None,
                        };
                        let font_size = style.font_size.to_pixels(window.rem_size());
                        window
                            .text_system()
                            .shape_line(shown.clone(), font_size, &[run], None)
                    },
                    move |bounds, line, window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        let y = bounds.top() + (bounds.size.height - window.line_height()) / 2.;
                        if !empty && !sel.is_empty() {
                            window.paint_quad(fill(
                                Bounds::from_corners(
                                    point(
                                        bounds.left() + line.x_for_index(sel.start),
                                        bounds.top(),
                                    ),
                                    point(
                                        bounds.left() + line.x_for_index(sel.end),
                                        bounds.bottom(),
                                    ),
                                ),
                                colors.selection,
                            ));
                        }
                        line.paint(point(bounds.left(), y), window.line_height(), window, cx)
                            .ok();
                        if focused && sel.is_empty() {
                            let x = if empty {
                                px(0.)
                            } else {
                                line.x_for_index(sel.start)
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(bounds.left() + x, bounds.top() + px(2.)),
                                    size(px(1.5), bounds.size.height - px(4.)),
                                ),
                                colors.cursor,
                            ));
                        }
                        let laid_out = if empty { None } else { Some(line) };
                        entity.update(cx, |this, _| {
                            this.layout = laid_out;
                            this.bounds = Some(bounds);
                        });
                    },
                )
                .size_full(),
            )
    }
}
