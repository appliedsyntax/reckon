//! Charts as results: `scatter(x, y)`, `bland_altman(x, y)`, `bars(list)`
//! and `line(list)`. The engine works out what to draw (points, guide
//! lines, names and labels); each app draws it. Where an app cannot draw,
//! the chart's summary is its text result.

use crate::stats::{self, Fit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    Scatter,
    BlandAltman,
    Bars,
    Line,
}

impl ChartKind {
    /// How many lines of the sheet the small chart takes.
    pub fn rows(self) -> usize {
        match self {
            ChartKind::Scatter | ChartKind::BlandAltman => 3,
            ChartKind::Bars => 2,
            ChartKind::Line => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideRole {
    /// The fitted line of a scatter.
    Fit,
    /// Where x and y agree (y = x).
    Identity,
    /// The Bland-Altman bias.
    Bias,
    /// A Bland-Altman limit of agreement.
    Limit,
}

/// A straight line across a chart: y = slope · x + intercept.
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    pub slope: f64,
    pub intercept: f64,
    pub role: GuideRole,
    /// Shown beside the line in the larger view: "bias 10.05".
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub kind: ChartKind,
    /// In the units of the axes. Bars and lines: (1, v₁), (2, v₂), ….
    pub points: Vec<(f64, f64)>,
    pub guides: Vec<Guide>,
    /// Axis titles, with units where there are any.
    pub x_title: String,
    pub y_title: String,
    /// The larger view's heading: "Passing-Bablok: slope 1.026".
    pub title: String,
    /// A short note drawn in the corner of the small chart: "slope 1.026".
    pub corner: String,
    /// The chart as text, where it cannot be drawn: "30 points, slope 1.026".
    pub summary: String,
}

/// Numbers in chart labels: four significant digits, no grouping.
pub fn short(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{v}");
    }
    let mag = v.abs().log10().floor() as i32;
    let places = (3 - mag).clamp(0, 12) as usize;
    let s = format!("{v:.places$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn titled(name: &str, unit: &str) -> String {
    if unit.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({unit})")
    }
}

fn plural(n: usize, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {one}s")
    }
}

/// A scatter of y against x. When the two are the same kind of quantity
/// (`same`), as in a method comparison, it shows the line where they agree
/// and fits Passing-Bablok, which outliers barely move; otherwise, or when
/// Passing-Bablok cannot fit the data, least squares.
pub fn scatter(
    x: &[f64],
    y: &[f64],
    names: (&str, &str),
    units: (&str, &str),
    same: bool,
) -> Chart {
    let n = x.len();
    let mut guides = Vec::new();
    let pb = if same && n <= stats::PB_MAX_POINTS {
        stats::passing_bablok(x, y).ok()
    } else {
        None
    };
    let fit = match pb {
        Some(l) => Some(("Passing-Bablok", l.slope, l.intercept)),
        None => {
            let f = Fit::new(x, y);
            f.slope()
                .zip(f.intercept())
                .map(|(b, a)| ("Least squares", b, a))
        }
    };
    if same {
        guides.push(Guide {
            slope: 1.0,
            intercept: 0.0,
            role: GuideRole::Identity,
            label: "y = x".into(),
        });
    }
    let (title, corner, summary) = match fit {
        Some((method, b, a)) => {
            guides.push(Guide {
                slope: b,
                intercept: a,
                role: GuideRole::Fit,
                label: method.into(),
            });
            (
                format!("{method}: slope {}, intercept {}", short(b), short(a)),
                format!("slope {}", short(b)),
                format!("{}, slope {}", plural(n, "point"), short(b)),
            )
        }
        None => (
            format!("{} against {}", names.1, names.0),
            String::new(),
            plural(n, "point"),
        ),
    };
    Chart {
        kind: ChartKind::Scatter,
        points: x.iter().copied().zip(y.iter().copied()).collect(),
        guides,
        x_title: titled(names.0, units.0),
        y_title: titled(names.1, units.1),
        title,
        corner,
        summary,
    }
}

/// The Bland-Altman plot: each pair's difference (y − x) against its
/// average, with the bias and the 95% limits of agreement. Needs two pairs.
pub fn bland_altman(x: &[f64], y: &[f64], names: (&str, &str), unit: &str) -> Option<Chart> {
    let a = stats::bland_altman(x, y)?;
    let level = |v: f64, role, label: String| Guide {
        slope: 0.0,
        intercept: v,
        role,
        label,
    };
    let n = x.len();
    Some(Chart {
        kind: ChartKind::BlandAltman,
        points: x
            .iter()
            .zip(y)
            .map(|(a, b)| ((a + b) / 2.0, b - a))
            .collect(),
        guides: vec![
            level(
                a.upper,
                GuideRole::Limit,
                format!("+1.96 SD {}", short(a.upper)),
            ),
            level(a.bias, GuideRole::Bias, format!("bias {}", short(a.bias))),
            level(
                a.lower,
                GuideRole::Limit,
                format!("−1.96 SD {}", short(a.lower)),
            ),
        ],
        x_title: titled(&format!("average of {} and {}", names.0, names.1), unit),
        y_title: titled(&format!("{} − {}", names.1, names.0), unit),
        title: format!(
            "Bias {}, limits of agreement {} to {}",
            short(a.bias),
            short(a.lower),
            short(a.upper)
        ),
        corner: format!("bias {}", short(a.bias)),
        summary: format!("{}, bias {}", plural(n, "point"), short(a.bias)),
    })
}

/// Bars or a line through values in order.
pub fn series(kind: ChartKind, v: &[f64], name: &str, unit: &str) -> Chart {
    let n = v.len();
    let what = if kind == ChartKind::Bars {
        plural(n, "bar")
    } else {
        plural(n, "point")
    };
    let (lo, hi) = v
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), x| {
            (l.min(*x), h.max(*x))
        });
    let range = if n == 0 {
        String::new()
    } else {
        format!("{} to {}", short(lo), short(hi))
    };
    Chart {
        kind,
        points: v
            .iter()
            .enumerate()
            .map(|(i, y)| (i as f64 + 1.0, *y))
            .collect(),
        guides: Vec::new(),
        x_title: String::new(),
        y_title: titled(name, unit),
        title: if name.is_empty() {
            what.clone()
        } else {
            format!("{name}: {what}")
        },
        corner: String::new(),
        summary: if range.is_empty() {
            what
        } else {
            format!("{what}, {range}")
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_numbers() {
        assert_eq!(short(1.02617801), "1.026");
        assert_eq!(short(10.0466667), "10.05");
        assert_eq!(short(-12.2139), "-12.21");
        assert_eq!(short(1234.6), "1235");
        assert_eq!(short(0.000123456), "0.0001235");
        assert_eq!(short(0.0), "0");
    }

    #[test]
    fn charts() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let y = [1.1, 2.0, 3.2, 3.9, 5.1, 6.0, 7.2, 7.9];
        let c = scatter(&x, &y, ("ref", "new"), ("mg/dL", "mg/dL"), true);
        assert_eq!(c.guides[0].role, GuideRole::Identity);
        assert_eq!(c.guides[1].label, "Passing-Bablok");
        assert!(c.title.starts_with("Passing-Bablok: slope "));
        assert_eq!(c.x_title, "ref (mg/dL)");
        assert_eq!(c.points.len(), 8);
        // Different quantities: least squares, no line of agreement.
        let c = scatter(&x, &y, ("hours", "km"), ("h", "km"), false);
        assert_eq!(c.guides.len(), 1);
        assert_eq!(c.guides[0].label, "Least squares");
        // Bland-Altman: mean and difference of each pair.
        let c = bland_altman(&[10.0, 20.0], &[12.0, 21.0], ("a", "b"), "").unwrap();
        assert_eq!(c.points, vec![(11.0, 2.0), (20.5, 1.0)]);
        assert_eq!(c.guides[1].intercept, 1.5);
        assert_eq!(c.summary, "2 points, bias 1.5");
        assert!(bland_altman(&[1.0], &[2.0], ("a", "b"), "").is_none());
        let c = series(ChartKind::Bars, &[3.0, 1.0, 2.0], "sales", "");
        assert_eq!(c.points[2], (3.0, 2.0));
        assert_eq!(c.summary, "3 bars, 1 to 3");
    }
}
