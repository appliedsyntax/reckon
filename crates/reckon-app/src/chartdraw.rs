//! Draws a chart result (see `reckon_core::chart`): small in the results
//! column, or large, with axes and values, in the chart panel. Both are
//! drawn by the same code from the same chart, so they always agree.

use std::sync::Arc;

use gpui::{
    fill, point, px, size, App, Bounds, Font, FontWeight, Hsla, PathBuilder, Pixels, Point,
    SharedString, TextRun, Window,
};
use reckon_core::chart::{short, Chart, ChartKind, GuideRole};

use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq)]
pub enum Size {
    /// In the results column: no axes; a note in the corner.
    Small,
    /// In the panel: axes, ticks, titles and the value under the pointer.
    Large,
}

/// Where data values land inside a plot area.
struct Frame {
    plot: Bounds<Pixels>,
    x: (f64, f64),
    y: (f64, f64),
}

impl Frame {
    fn new(chart: &Chart, plot: Bounds<Pixels>) -> Frame {
        let fold = |it: &mut dyn Iterator<Item = f64>| {
            it.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                if v.is_finite() {
                    (l.min(v), h.max(v))
                } else {
                    (l, h)
                }
            })
        };
        let mut x = fold(&mut chart.points.iter().map(|p| p.0));
        let mut y = fold(&mut chart.points.iter().map(|p| p.1));
        match chart.kind {
            // Bars stand on zero and sit between half-steps.
            ChartKind::Bars => {
                x = (0.5, chart.points.len() as f64 + 0.5);
                y = (y.0.min(0.0), y.1.max(0.0));
            }
            // The bias and limits must be in view.
            ChartKind::BlandAltman => {
                for g in &chart.guides {
                    y = (y.0.min(g.intercept), y.1.max(g.intercept));
                }
            }
            _ => {}
        }
        let pad = |(l, h): (f64, f64), share: f64| {
            if !l.is_finite() {
                return (0.0, 1.0);
            }
            let span = if h > l { h - l } else { l.abs().max(1.0) };
            (l - span * share, h + span * share)
        };
        let x = if chart.kind == ChartKind::Bars {
            x
        } else {
            pad(x, 0.04)
        };
        Frame {
            plot,
            x,
            y: pad(y, 0.08),
        }
    }

    fn at(&self, x: f64, y: f64) -> Point<Pixels> {
        let fx = ((x - self.x.0) / (self.x.1 - self.x.0)) as f32;
        let fy = ((y - self.y.0) / (self.y.1 - self.y.0)) as f32;
        point(
            self.plot.left() + self.plot.size.width * fx,
            self.plot.bottom() - self.plot.size.height * fy,
        )
    }
}

/// A rounded step for axis ticks: 1, 2 or 5 times a power of ten.
fn tick_step(span: f64, count: f64) -> f64 {
    let raw = span / count;
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    // Halfway between the round numbers, on a log scale.
    let nice = if f < std::f64::consts::SQRT_2 {
        1.0
    } else if f < 10f64.sqrt() {
        2.0
    } else if f < 50f64.sqrt() {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn ticks((lo, hi): (f64, f64), count: f64) -> Vec<f64> {
    let step = tick_step(hi - lo, count);
    if step.is_nan() || step <= 0.0 || step.is_infinite() {
        return Vec::new();
    }
    let mut t = (lo / step).ceil() * step;
    let mut out = Vec::new();
    while t <= hi && out.len() < 20 {
        out.push(if t.abs() < step * 1e-9 { 0.0 } else { t });
        t += step;
    }
    out
}

fn shaped(window: &Window, s: &str, font: &Font, size: Pixels, color: Hsla) -> gpui::ShapedLine {
    let run = TextRun {
        len: s.len(),
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(SharedString::from(s.to_string()), size, &[run], None)
}

fn text(
    window: &mut Window,
    cx: &mut App,
    s: &str,
    at: Point<Pixels>,
    (font, size, color): (&Font, Pixels, Hsla),
    right_aligned: bool,
) -> Pixels {
    let line = shaped(window, s, font, size, color);
    let x = if right_aligned {
        at.x - line.width
    } else {
        at.x
    };
    line.paint(point(x, at.y), size * 1.3, window, cx).ok();
    line.width
}

fn stroke(
    window: &mut Window,
    from: Point<Pixels>,
    to: Point<Pixels>,
    w: f32,
    dash: bool,
    c: Hsla,
) {
    let mut b = PathBuilder::stroke(px(w));
    if dash {
        b = b.dash_array(&[px(4.), px(3.)]);
    }
    b.move_to(from);
    b.line_to(to);
    if let Ok(path) = b.build() {
        window.paint_path(path, c);
    }
}

/// The index of the point (or bar) nearest the pointer, if it is close.
fn nearest(chart: &Chart, f: &Frame, pos: Point<Pixels>) -> Option<usize> {
    if chart.kind == ChartKind::Bars {
        if !f.plot.contains(&pos) {
            return None;
        }
        let i = ((pos.x - f.plot.left()) / f.plot.size.width * chart.points.len() as f32) as usize;
        return (i < chart.points.len()).then_some(i);
    }
    let mut best: Option<(usize, f32)> = None;
    for (i, p) in chart.points.iter().enumerate() {
        let q = f.at(p.0, p.1);
        let d = f32::from(q.x - pos.x).hypot(f32::from(q.y - pos.y));
        if d < 14.0 && best.is_none_or(|b| d < b.1) {
            best = Some((i, d));
        }
    }
    best.map(|b| b.0)
}

/// What the pointer is over, in words.
fn describe(chart: &Chart, i: usize) -> String {
    let (x, y) = chart.points[i];
    match chart.kind {
        ChartKind::Scatter => format!("sample {}: {} → {}", i + 1, short(x), short(y)),
        ChartKind::BlandAltman => format!(
            "sample {}: average {}, difference {}",
            i + 1,
            short(x),
            short(y)
        ),
        _ => format!("{}: {}", i + 1, short(y)),
    }
}

/// Draws `chart` in `bounds`. `hover` is the pointer, when it is over the
/// chart.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    chart: &Arc<Chart>,
    bounds: Bounds<Pixels>,
    theme: &Theme,
    font: &Font,
    font_size: Pixels,
    sz: Size,
    hover: Option<Point<Pixels>>,
    window: &mut Window,
    cx: &mut App,
) {
    let small_text = font_size * 0.78;
    let label_font = {
        let mut f = font.clone();
        f.weight = FontWeight::NORMAL;
        f
    };
    let plot = match sz {
        Size::Small => Bounds::from_corners(
            point(bounds.left() + px(2.), bounds.top() + px(4.)),
            point(bounds.right() - px(2.), bounds.bottom() - px(4.)),
        ),
        Size::Large => Bounds::from_corners(
            point(bounds.left() + px(64.), bounds.top() + small_text * 2.2),
            point(bounds.right() - px(96.), bounds.bottom() - small_text * 3.4),
        ),
    };
    if plot.size.width <= px(4.) || plot.size.height <= px(4.) {
        return;
    }
    let f = Frame::new(chart, plot);
    let data = theme.result;
    let quiet = theme.muted;

    if sz == Size::Large {
        // Axes, ticks and titles.
        let lab = (&label_font, small_text, quiet);
        stroke(
            window,
            plot.bottom_left(),
            plot.bottom_right(),
            1.,
            false,
            theme.border,
        );
        stroke(
            window,
            plot.bottom_left(),
            plot.origin,
            1.,
            false,
            theme.border,
        );
        for t in ticks(f.y, 5.0) {
            let p = f.at(f.x.0, t);
            stroke(
                window,
                point(plot.left() - px(4.), p.y),
                point(plot.left(), p.y),
                1.,
                false,
                theme.border,
            );
            text(
                window,
                cx,
                &short(t),
                point(plot.left() - px(8.), p.y - small_text * 0.65),
                lab,
                true,
            );
        }
        let xt = if matches!(chart.kind, ChartKind::Bars | ChartKind::Line) {
            ticks(f.x, 8.0)
                .into_iter()
                .filter(|t| t.fract() == 0.0 && *t >= 1.0)
                .collect()
        } else {
            ticks(f.x, 6.0)
        };
        for t in xt {
            let p = f.at(t, f.y.0);
            stroke(
                window,
                point(p.x, plot.bottom()),
                point(p.x, plot.bottom() + px(4.)),
                1.,
                false,
                theme.border,
            );
            let w = shaped(window, &short(t), &label_font, small_text, quiet).width;
            text(
                window,
                cx,
                &short(t),
                point(p.x - w / 2., plot.bottom() + px(6.)),
                lab,
                false,
            );
        }
        if !chart.x_title.is_empty() {
            text(
                window,
                cx,
                &chart.x_title,
                point(plot.right(), plot.bottom() + small_text * 1.9),
                lab,
                true,
            );
        }
        if !chart.y_title.is_empty() {
            text(
                window,
                cx,
                &chart.y_title,
                point(plot.left() - px(56.), bounds.top()),
                lab,
                false,
            );
        }
    }

    window.with_content_mask(Some(gpui::ContentMask { bounds: plot }), |window| {
        // Guide lines across the whole plot.
        for g in &chart.guides {
            let (a, b) = (
                f.at(f.x.0, g.intercept + g.slope * f.x.0),
                f.at(f.x.1, g.intercept + g.slope * f.x.1),
            );
            let (color, dash, w) = match g.role {
                GuideRole::Identity => (quiet, true, 1.),
                GuideRole::Limit => (quiet, true, 1.),
                GuideRole::Fit | GuideRole::Bias => (data.opacity(0.75), false, 1.5),
            };
            stroke(window, a, b, w, dash, color);
        }
        let r = if sz == Size::Small { px(1.6) } else { px(3.) };
        match chart.kind {
            ChartKind::Scatter | ChartKind::BlandAltman => {
                for p in &chart.points {
                    let c = f.at(p.0, p.1);
                    let dot = Bounds::new(point(c.x - r, c.y - r), size(r * 2., r * 2.));
                    window.paint_quad(fill(dot, data).corner_radii(r));
                }
            }
            ChartKind::Bars => {
                let n = chart.points.len().max(1) as f32;
                let slot = plot.size.width / n;
                let w = (slot * 0.7).max(px(1.));
                let base = f.at(0.0, 0.0).y;
                for p in &chart.points {
                    let c = f.at(p.0, p.1);
                    let (top, bottom) = if c.y < base { (c.y, base) } else { (base, c.y) };
                    let bar = Bounds::from_corners(
                        point(c.x - w / 2., top),
                        point(c.x + w / 2., bottom.max(top + px(1.))),
                    );
                    window.paint_quad(fill(bar, data.opacity(0.85)));
                }
            }
            ChartKind::Line => {
                let mut b = PathBuilder::stroke(px(if sz == Size::Small { 1.5 } else { 2. }));
                for (i, p) in chart.points.iter().enumerate() {
                    let c = f.at(p.0, p.1);
                    if i == 0 {
                        b.move_to(c);
                    } else {
                        b.line_to(c);
                    }
                }
                if chart.points.len() == 1 {
                    let c = f.at(chart.points[0].0, chart.points[0].1);
                    b.line_to(point(c.x + px(1.), c.y));
                }
                if let Ok(path) = b.build() {
                    window.paint_path(path, data);
                }
            }
        }
    });

    let note = (&label_font, small_text, quiet);
    match sz {
        Size::Small => {
            // The corner note, or on hover the smallest and largest value.
            let corner = if hover.is_some() {
                let ys = chart.points.iter().map(|p| p.1);
                let lo = ys.clone().fold(f64::INFINITY, f64::min);
                let hi = ys.fold(f64::NEG_INFINITY, f64::max);
                format!("{} to {}", short(lo), short(hi))
            } else {
                chart.corner.clone()
            };
            if !corner.is_empty() {
                text(
                    window,
                    cx,
                    &corner,
                    point(bounds.left() + px(4.), bounds.top()),
                    note,
                    false,
                );
            }
        }
        Size::Large => {
            // Labels at the right end of the Bland-Altman lines.
            if chart.kind == ChartKind::BlandAltman {
                for g in &chart.guides {
                    let y = f.at(f.x.1, g.intercept).y;
                    text(
                        window,
                        cx,
                        &g.label,
                        point(plot.right() + px(6.), y - small_text * 0.65),
                        note,
                        false,
                    );
                }
            }
            if let Some(i) = hover.and_then(|h| nearest(chart, &f, h)) {
                let p = chart.points[i];
                let c = f.at(p.0, p.1);
                if chart.kind != ChartKind::Bars {
                    let ring =
                        Bounds::new(point(c.x - px(6.), c.y - px(6.)), size(px(12.), px(12.)));
                    window.paint_quad(gpui::quad(
                        ring,
                        px(6.),
                        gpui::transparent_black(),
                        px(1.5),
                        theme.cursor,
                        gpui::BorderStyle::Solid,
                    ));
                }
                let label = describe(chart, i);
                let tip = (&label_font, small_text, theme.text);
                let w = shaped(window, &label, &label_font, small_text, theme.text).width;
                let h = small_text * 1.3;
                let mut at = point(c.x + px(10.), c.y - h - px(10.));
                if at.x + w + px(12.) > bounds.right() {
                    at.x = c.x - w - px(22.);
                }
                if at.y < bounds.top() {
                    at.y = c.y + px(10.);
                }
                let boxed = Bounds::new(
                    point(at.x - px(6.), at.y - px(3.)),
                    size(w + px(12.), h + px(6.)),
                );
                window.paint_quad(gpui::quad(
                    boxed,
                    px(5.),
                    theme.surface,
                    px(1.),
                    theme.border,
                    gpui::BorderStyle::Solid,
                ));
                text(window, cx, &label, at, tip, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_steps_are_round() {
        assert_eq!(tick_step(100.0, 5.0), 20.0);
        assert_eq!(tick_step(37.0, 5.0), 10.0);
        assert_eq!(tick_step(30.0, 5.0), 5.0);
        assert_eq!(tick_step(0.9, 5.0), 0.2);
        assert_eq!(ticks((53.0, 421.0), 5.0), vec![100.0, 200.0, 300.0, 400.0]);
        assert_eq!(
            ticks((-12.2, 32.3), 5.0),
            vec![-10.0, 0.0, 10.0, 20.0, 30.0]
        );
    }
}
