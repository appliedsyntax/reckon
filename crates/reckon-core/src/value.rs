//! Runtime values and how they are displayed.

use crate::time::{civil_from_days, split, MONTHS, WEEKDAYS};
use crate::tz::Zone;
use crate::units::{Conv, Unit, UnitCtx};

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

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(Quantity),
    Percent(f64),
    Time(DateTime),
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
    }
}

fn format_quantity(q: &Quantity, ucx: &UnitCtx, s: &Settings) -> String {
    let money = q
        .unit
        .as_simple()
        .is_some_and(|id| matches!(ucx.reg.def(id).conv, Conv::Currency(_)));
    let num = match q.fmt {
        NumFmt::Hex | NumFmt::Bin | NumFmt::Oct => format_radix(q.v, q.fmt),
        NumFmt::Sci => format_sci(q.v, s.precision),
        NumFmt::Auto if money => {
            let rounded = (q.v * 10f64.powi(s.currency_precision as i32)).round();
            if rounded % 10f64.powi(s.currency_precision as i32) == 0.0 && q.v.abs() < 1e15 {
                format_number(q.v, 0, s)
            } else {
                // Keep trailing zeros for money: 27.60, not 27.6
                let n = format_number(q.v, s.currency_precision, s);
                pad_decimals(&n, s.currency_precision, s.decimal_sep)
            }
        }
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
