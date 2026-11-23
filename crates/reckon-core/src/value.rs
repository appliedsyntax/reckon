//! Runtime values and how they are displayed.

use std::sync::Arc;

use crate::time::{civil_from_days, split, MONTHS, WEEKDAYS};
use crate::tz::Zone;
use crate::units::{Conv, Unit, UnitCtx, UnitId};

/// Output number format chosen with `in hex`, `in sci`, ...
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NumFmt {
    #[default]
    Auto,
    Hex,
    Bin,
    Oct,
    Sci,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quantity {
    pub v: f64,
    pub unit: Unit,
    pub fmt: NumFmt,
}

/// Which parts of a date-time to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Date,
    Time,
    DateTime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DateTime {
    /// Seconds since the Unix epoch (UTC).
    pub utc: f64,
    pub zone: Zone,
    pub show: Show,
    /// True when `zone` is the user's local zone (no zone label shown).
    pub local: bool,
    /// Derived from "now" (affects how differences are computed).
    pub live: bool,
}

/// Several numbers that share one unit: `[3 kg, 500 g]` is held as
/// 3 and 0.5 kilograms. Statistics functions take these.
#[derive(Clone, Debug, PartialEq)]
pub struct List {
    pub v: Vec<f64>,
    pub unit: Unit,
    /// The numbers are percentages (and `unit` is empty).
    pub percent: bool,
}

impl List {
    /// The number `x` as a value in this list's unit.
    pub fn value(&self, x: f64) -> Value {
        if self.percent {
            Value::Percent(x)
        } else {
            Value::qty(x, self.unit.clone())
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(Quantity),
    Percent(f64),
    Time(DateTime),
    /// Shared, so that copying a list into a variable or a result costs
    /// nothing however long it is.
    List(Arc<List>),
}

impl Value {
    pub fn num(v: f64) -> Value {
        Value::Num(Quantity {
            v,
            unit: Unit::none(),
            fmt: NumFmt::Auto,
        })
    }
    pub fn qty(v: f64, unit: Unit) -> Value {
        Value::Num(Quantity {
            v,
            unit,
            fmt: NumFmt::Auto,
        })
    }
    /// Plain numeric value, if this is a quantity.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(q) => Some(q.v),
            Value::Percent(p) => Some(*p),
            Value::Time(t) => Some(t.utc),
            Value::List(_) => None,
        }
    }
}

/// Display preferences.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Maximum decimal places for plain numbers and units.
    pub precision: usize,
    /// Decimal places for money.
    pub currency_precision: usize,
    /// Thousands separator, `None` to disable grouping.
    pub group_sep: Option<char>,
    pub decimal_sep: char,
    /// 24-hour clock instead of am/pm.
    pub hour24: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            precision: 4,
            currency_precision: 2,
            group_sep: Some(','),
            decimal_sep: '.',
            hour24: false,
        }
    }
}

pub fn format_number(v: f64, decimals: usize, s: &Settings) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "∞".into() } else { "-∞".into() };
    }
    let a = v.abs();
    let smallest = 0.5 * 10f64.powi(-(decimals as i32));
    if a >= 1e15 || (a != 0.0 && a < smallest) {
        return format_sci(v, decimals.max(4));
    }
    let raw = format!("{:.*}", decimals, v);
    let raw = if raw.contains('.') {
        raw.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        raw
    };
    let (neg, body) = match raw.strip_prefix('-') {
        Some(b) => (true, b.to_string()),
        None => (false, raw),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (body, None),
    };
    let int = match s.group_sep {
        Some(sep) if int.len() > 3 => {
            let mut out = String::new();
            for (i, c) in int.chars().enumerate() {
                if i > 0 && (int.len() - i) % 3 == 0 {
                    out.push(sep);
                }
                out.push(c);
            }
            out
        }
        _ => int,
    };
    let mut out = String::new();
    if neg && (int != "0" || frac.is_some()) {
        out.push('-');
    }
    out.push_str(&int);
    if let Some(f) = frac {
        out.push(s.decimal_sep);
        out.push_str(&f);
    }
    out
}

pub fn format_sci(v: f64, decimals: usize) -> String {
    let raw = format!("{:.*e}", decimals, v);
    match raw.split_once('e') {
        Some((m, e)) => {
            let m = if m.contains('.') {
                m.trim_end_matches('0').trim_end_matches('.')
            } else {
                m
            };
            format!("{m}e{e}")
        }
        None => raw,
    }
}

fn format_radix(v: f64, fmt: NumFmt) -> String {
    let n = v.round() as i128;
    let (neg, a) = (n < 0, n.unsigned_abs());
    let body = match fmt {
        NumFmt::Hex => format!("0x{a:X}"),
        NumFmt::Bin => format!("0b{a:b}"),
        NumFmt::Oct => format!("0o{a:o}"),
        _ => a.to_string(),
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

pub fn format_value(v: &Value, ucx: &UnitCtx, s: &Settings) -> String {
    match v {
        Value::Percent(p) => format!("{}%", format_number(*p, s.precision, s)),
        Value::Time(t) => format_datetime(t, s),
        Value::Num(q) => format_quantity(q, ucx, s),
        Value::List(l) => format_list(l, ucx, s, LIST_WIDTH),
    }
}

/// `v` as [`format_value`] writes it, except that a list may take up to
/// `width` characters before it is shortened (`usize::MAX`: never).
pub fn format_value_width(v: &Value, ucx: &UnitCtx, s: &Settings, width: usize) -> String {
    match v {
        Value::List(l) => format_list(l, ucx, s, width),
        _ => format_value(v, ucx, s),
    }
}

/// Longest list written out in full, in characters, in a line's result.
/// Results sit in a narrow column (on a phone, most of all), so a longer
/// list shows its first values and a count. The desktop app gives a list
/// the room its line leaves (see [`format_value_width`]).
const LIST_WIDTH: usize = 24;

/// `[1200, 3.5, 8]`, or `[48 km, … 12 values]` when that would be too wide.
/// Digits are not grouped and the separator avoids the decimal mark, so a
/// list shown in full can be pasted back in as the same list.
fn format_list(l: &List, ucx: &UnitCtx, s: &Settings, list_width: usize) -> String {
    let plain = Settings {
        group_sep: None,
        ..s.clone()
    };
    let sep = if s.decimal_sep == ',' { "; " } else { ", " };
    let width = |t: &str| t.chars().count();
    // No more values than this can fit, however short they are.
    let shown: Vec<String> =
        l.v.iter()
            .take((list_width / 2).min(l.v.len()))
            .map(|x| format_value(&l.value(*x), ucx, &plain))
            .collect();
    let full = format!("[{}]", shown.join(sep));
    // A pair, such as a confidence interval, is always shown whole.
    if shown.len() == l.v.len() && (width(&full) <= list_width || l.v.len() <= 2) {
        return full;
    }
    let count = format!("… {} values]", l.v.len());
    let mut text = String::from("[");
    for (i, item) in shown.iter().enumerate() {
        let wide = width(&text) + width(item) + width(sep) + width(&count) > list_width;
        if i > 0 && wide {
            break;
        }
        text.push_str(item);
        text.push_str(sep);
    }
    text + &count
}

fn format_quantity(q: &Quantity, ucx: &UnitCtx, s: &Settings) -> String {
    let (v, unit) = ucx.shown(q.v, &q.unit);
    let q = &Quantity {
        v,
        unit,
        fmt: q.fmt,
    };
    let is_money = |id: UnitId| matches!(ucx.reg.def(id).conv, Conv::Currency(_));
    let money = q.unit.as_simple().is_some_and(is_money);
    // Money per something is written as money: $0.46/Mtok, ₹37.04/h.
    if q.fmt == NumFmt::Auto && !money {
        let mut cur = q.unit.parts.iter().filter(|(id, _)| is_money(*id));
        if let (Some(&(c, 1)), None) = (cur.next(), cur.next()) {
            let rest: Vec<(UnitId, i8)> =
                q.unit.parts.iter().filter(|p| p.0 != c).copied().collect();
            if rest.iter().all(|(_, p)| *p < 0) {
                let per = Unit::normalized(rest.into_iter().map(|(id, p)| (id, -p)).collect());
                let amount = Quantity {
                    v: q.v,
                    unit: Unit::simple(c),
                    fmt: q.fmt,
                };
                return format!(
                    "{}/{}",
                    format_quantity(&amount, ucx, s),
                    ucx.label(&per, false)
                );
            }
        }
    }
    let num = match q.fmt {
        NumFmt::Hex | NumFmt::Bin | NumFmt::Oct => format_radix(q.v, q.fmt),
        NumFmt::Sci => format_sci(q.v, s.precision),
        NumFmt::Auto if money => format_money(q.v, s),
        NumFmt::Auto => format_number(q.v, s.precision, s),
    };
    if q.unit.is_none() {
        return num;
    }
    let plural = (q.v - 1.0).abs() > 1e-12;
    let label = ucx.label(&q.unit, plural);
    if let Some(id) = q.unit.as_simple() {
        let def = ucx.reg.def(id);
        if def.prefix && q.fmt == NumFmt::Auto {
            return match num.strip_prefix('-') {
                Some(n) => format!("-{label}{n}"),
                None => format!("{label}{num}"),
            };
        }
        if def.tight {
            return format!("{num}{label}");
        }
    }
    format!("{num} {label}")
}

/// A money amount: whole when it is whole ($20), otherwise to the cent with
/// trailing zeros kept ($27.60). Amounts under ten cents get three
/// significant digits instead, since a cent can be most of them: a request
/// that costs 0.6¢ shows as $0.006, not $0.01.
fn format_money(v: f64, s: &Settings) -> String {
    let p = s.currency_precision as i32;
    if v != 0.0 && v.abs() < 10f64.powi(1 - p) {
        let places = (2 - v.abs().log10().floor() as i32).max(p);
        if places > 15 {
            return format_sci(v, 2);
        }
        let n = format_number(v, places as usize, s);
        return pad_decimals(&n, p as usize, s.decimal_sep);
    }
    let rounded = (v * 10f64.powi(p)).round();
    if rounded % 10f64.powi(p) == 0.0 && v.abs() < 1e15 {
        format_number(v, 0, s)
    } else {
        // Keep trailing zeros for money: 27.60, not 27.6
        let n = format_number(v, s.currency_precision, s);
        pad_decimals(&n, s.currency_precision, s.decimal_sep)
    }
}

fn pad_decimals(n: &str, places: usize, dec: char) -> String {
    if n.contains('e') {
        return n.to_string();
    }
    match n.split_once(dec) {
        Some((_, f)) if f.len() < places => format!("{n}{}", "0".repeat(places - f.len())),
        Some(_) => n.to_string(),
        None => n.to_string(),
    }
}

pub fn format_clock(sod: i64, s: &Settings) -> String {
    let h = sod / 3600;
    let m = (sod % 3600) / 60;
    if s.hour24 {
        format!("{h:02}:{m:02}")
    } else {
        let suffix = if h < 12 { "am" } else { "pm" };
        let h12 = match h % 12 {
            0 => 12,
            x => x,
        };
        format!("{h12}:{m:02} {suffix}")
    }
}

pub fn format_datetime(t: &DateTime, s: &Settings) -> String {
    let (off, abbr) = t.zone.offset_at(t.utc.floor() as i64);
    let (days, sod) = split(t.utc + off as f64);
    let (y, m, d) = civil_from_days(days);
    let wd = &WEEKDAYS[crate::time::weekday(days)][..3];
    let date = format!("{wd}, {d} {} {y}", &MONTHS[m as usize - 1][..3]);
    let clock = format_clock(sod.floor() as i64, s);
    let zone = if t.local {
        String::new()
    } else {
        format!(" {abbr}")
    };
    match t.show {
        Show::Date => date,
        Show::Time => format!("{clock}{zone}"),
        Show::DateTime => format!("{date}, {clock}{zone}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbers() {
        let s = Settings::default();
        assert_eq!(format_number(1234567.891, 2, &s), "1,234,567.89");
        assert_eq!(format_number(-0.00001, 4, &s), "-1e-5");
        assert_eq!(format_number(2.0, 4, &s), "2");
        assert_eq!(format_number(-0.0, 4, &s), "0");
        assert_eq!(format_number(1.0 / 3.0, 4, &s), "0.3333");
        assert_eq!(format_sci(5300.0, 4), "5.3e3");
        assert_eq!(format_radix(255.0, NumFmt::Hex), "0xFF");
    }
}
