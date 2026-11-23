//! Evaluates parsed expressions into values.

use std::collections::HashMap;
use std::sync::Arc;

use crate::parser::{BinOp, Expr, Pct, Target};
use crate::stats::{self, Fit, Line, PbError, PB_MAX_POINTS};
use crate::time::{add_months, civil_from_days, days_from_civil, days_in_month, split};
use crate::tz::Zone;
use crate::units::{dim, Conv, Unit, UnitCtx, ANGLE, TIME};
use crate::value::{DateTime, List, Quantity, Show, Value};
use crate::{Engine, UserFn};

/// State carried from line to line within one document.
#[derive(Clone, Debug)]
pub struct DocState {
    pub vars: HashMap<String, Value>,
    /// Pixels per inch for CSS units.
    pub ppi: f64,
    /// Size of 1em in pixels.
    pub em_px: f64,
    /// Result of the previous line (`prev`).
    pub prev: Option<Value>,
    /// Results since the last blank line or header (`sum`, `avg`).
    pub block: Vec<Value>,
    /// Numbers put into lists so far in this document (see
    /// [`MAX_DOC_LIST_VALUES`]).
    pub list_values: usize,
    /// The last Passing-Bablok fit and the two lists it was made from, so
    /// that the slope, intercept and intervals of the same data, on four
    /// lines, cost one fit.
    pub pb_cache: Option<PbCache>,
}

/// A Passing-Bablok fit and the lists (x, y) it was made from.
pub type PbCache = (Arc<List>, Arc<List>, Result<Line, PbError>);

/// Most numbers one list holds.
const MAX_LIST_VALUES: usize = 10_000;
/// Most numbers all the lists written in one document hold. Every result is
/// kept for as long as the sheet is open, so without a limit a small sheet
/// that keeps doubling a list could use all the memory there is.
const MAX_DOC_LIST_VALUES: usize = 100_000;
/// Numbers a statistics function works through for the cost of one function
/// call (see [`crate::MAX_CALLS`]).
const VALUES_PER_CALL: usize = 64;

impl Default for DocState {
    fn default() -> Self {
        DocState {
            vars: HashMap::new(),
            ppi: 96.0,
            em_px: 16.0,
            prev: None,
            block: Vec::new(),
            list_values: 0,
            pb_cache: None,
        }
    }
}

pub struct Eval<'a> {
    pub eng: &'a Engine,
    pub st: &'a mut DocState,
    pub now: f64,
    pub depth: usize,
    /// Set when the expression used `sum`/`avg`; such lines are not
    /// themselves added to the running block.
    pub aggregate: bool,
}

type R = Result<Value, String>;

fn is_affine(ucx: &UnitCtx, u: &Unit) -> bool {
    u.parts
        .iter()
        .any(|(id, _)| matches!(ucx.reg.def(*id).conv, Conv::Affine { .. }))
}

fn map_num(v: Value, f: impl Fn(f64) -> f64) -> R {
    match v {
        Value::Num(q) => Ok(Value::Num(Quantity { v: f(q.v), ..q })),
        Value::Percent(p) => Ok(Value::Percent(f(p))),
        Value::Time(_) | Value::List(_) | Value::Chart(_) => Err("expected a number".into()),
    }
}

impl<'a> Eval<'a> {
    pub fn ucx(&self) -> UnitCtx<'_> {
        UnitCtx {
            reg: &self.eng.reg,
            rates: &self.eng.rates,
            ppi: self.st.ppi,
            em_px: self.st.em_px,
        }
    }

    fn unit(&self, key: &str) -> Unit {
        Unit::simple(self.eng.reg.by_key(key).expect("built-in unit"))
    }

    pub fn eval(&mut self, e: &Expr) -> R {
        match e {
            Expr::Num(v) => Ok(Value::num(*v)),
            Expr::WithUnit(inner, u) => {
                let v = self.eval(inner)?;
                self.apply_unit(v, u)
            }
            Expr::Percent(inner) => match self.eval(inner)? {
                Value::Num(q) if q.unit.is_none() => Ok(Value::Percent(q.v)),
                Value::Percent(p) => Ok(Value::Percent(p)),
                _ => Err("only plain numbers can be percentages".into()),
            },
            Expr::Var(name) => self.lookup_var(name),
            Expr::Assign(name, e) => {
                let v = self.eval(e)?;
                self.assign(name, v.clone())?;
                Ok(v)
            }
            Expr::Neg(e) => match self.eval(e)? {
                Value::Time(_) => Err("cannot negate a date".into()),
                v => map_num(v, |x| -x),
            },
            Expr::Bin(op, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                self.binop(*op, a, b)
            }
            Expr::Implicit(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                self.implicit(a, b)
            }
            Expr::PctOp(op, p, y) | Expr::PctInv(op, p, y) => {
                let f = self.percent_factor(p)?;
                let y = self.eval(y)?;
                let k = match op {
                    Pct::Of => f,
                    Pct::On => 1.0 + f,
                    Pct::Off => 1.0 - f,
                };
                self.mul(y, Value::num(k), matches!(e, Expr::PctInv(..)))
            }
            Expr::AsPct(op, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                let r = match self.mul(a, b, true)? {
                    Value::Num(q) if q.unit.is_none() => q.v,
                    _ => return Err("values must have compatible units".into()),
                };
                Ok(Value::Percent(match op {
                    Pct::Of => r * 100.0,
                    Pct::On => (r - 1.0) * 100.0,
                    Pct::Off => (1.0 - r) * 100.0,
                }))
            }
            Expr::Convert(e, t) => {
                let v = self.eval(e)?;
                self.convert(v, t)
            }
            Expr::Call(name, args) => self.call(name, args),
            Expr::List(items) => {
                let vals: Vec<Value> = items
                    .iter()
                    .map(|e| self.eval(e))
                    .collect::<Result<_, _>>()?;
                let list = self.gather(&vals, true)?;
                self.st.list_values += list.v.len();
                if self.st.list_values > MAX_DOC_LIST_VALUES {
                    return Err("this sheet has too many list values".into());
                }
                Ok(Value::List(Arc::new(list)))
            }
            Expr::Fact(e) => {
                let v = self.eval(e)?;
                self.factorial(v)
            }
            Expr::Prev => self
                .st
                .prev
                .clone()
                .ok_or_else(|| "no previous result".into()),
            Expr::Sum => self.aggregate(false),
            Expr::Avg => self.aggregate(true),
            Expr::Now(z) => Ok(self.mk_time(self.now, z.as_ref(), Show::Time, true)),
            Expr::Day(n) => {
                let days = split(self.local_now()).0 + n;
                let utc = self.eng.zones.local.to_utc(days * 86400) as f64;
                Ok(self.mk_time(utc, None, Show::Date, false))
            }
            Expr::TimeLit(h, m, s) => {
                if *h > 23 || *m > 59 || *s > 59 {
                    return Err("invalid time".into());
                }
                let days = split(self.local_now()).0;
                let local = days * 86400 + (*h as i64) * 3600 + (*m as i64) * 60 + *s as i64;
                let utc = self.eng.zones.local.to_utc(local) as f64;
                Ok(self.mk_time(utc, None, Show::Time, false))
            }
            Expr::DateLit(y, m, d) => {
                let y = if *y == 0 {
                    civil_from_days(split(self.local_now()).0).0
                } else {
                    *y as i64
                };
                if !(1..=12).contains(m) || *d == 0 || *d > days_in_month(y, *m) {
                    return Err("invalid date".into());
                }
                let utc = self
                    .eng
                    .zones
                    .local
                    .to_utc(days_from_civil(y, *m, *d) * 86400) as f64;
                Ok(self.mk_time(utc, None, Show::Date, false))
            }
            Expr::InZone(e, z) => match self.eval(e)? {
                Value::Time(t) => {
                    let (off, _) = t.zone.offset_at(t.utc.floor() as i64);
                    let wall = t.utc + off as f64;
                    let utc = z.to_utc(wall.floor() as i64) as f64 + wall.fract();
                    Ok(Value::Time(DateTime {
                        utc,
                        zone: z.clone(),
                        show: t.show,
                        local: false,
                        live: t.live,
                    }))
                }
                _ => Err("a time zone needs a time".into()),
            },
            Expr::Ago(d) | Expr::FromNow(d) => {
                let sign = if matches!(e, Expr::Ago(_)) { -1.0 } else { 1.0 };
                let Value::Num(q) = self.eval(d)? else {
                    return Err("expected a duration".into());
                };
                let base = DateTime {
                    utc: self.now,
                    zone: self.eng.zones.local.clone(),
                    show: Show::DateTime,
                    local: true,
                    live: false,
                };
                let (t, whole_days) = self.shift_raw(base, &q, sign)?;
                Ok(Value::Time(DateTime {
                    show: if whole_days {
                        Show::Date
                    } else {
                        Show::DateTime
                    },
                    ..t
                }))
            }
        }
    }

    // ------------------------------------------------------------ helpers

    fn local_now(&self) -> f64 {
        let (off, _) = self.eng.zones.local.offset_at(self.now.floor() as i64);
        self.now + off as f64
    }

    fn mk_time(&self, utc: f64, zone: Option<&Zone>, show: Show, live: bool) -> Value {
        let (zone, local) = match zone {
            Some(z) => (z.clone(), false),
            None => (self.eng.zones.local.clone(), true),
        };
        Value::Time(DateTime {
            utc,
            zone,
            show,
            local,
            live,
        })
    }

    fn lookup_var(&self, name: &str) -> R {
        if let Some(v) = self
            .st
            .vars
            .get(name)
            .or_else(|| self.eng.globals.get(name))
        {
            return Ok(v.clone());
        }
        if name == "ppi" {
            return Ok(Value::num(self.st.ppi));
        }
        Err(format!("unknown variable {name}"))
    }

    fn assign(&mut self, name: &str, v: Value) -> Result<(), String> {
        match name {
            "ppi" => match v {
                Value::Num(q) if q.unit.is_none() && q.v > 0.0 => self.st.ppi = q.v,
                _ => return Err("ppi must be a positive number".into()),
            },
            "em" => match v {
                Value::Num(q) if q.v > 0.0 => {
                    self.st.em_px = if q.unit.is_none() {
                        q.v
                    } else {
                        self.ucx().convert(q.v, &q.unit, &self.unit("px"))?
                    };
                }
                _ => return Err("em must be a positive size, e.g. em = 20px".into()),
            },
            _ => {
                self.st.vars.insert(name.to_string(), v);
            }
        }
        Ok(())
    }

    fn apply_unit(&self, v: Value, u: &Unit) -> R {
        match v {
            Value::Num(q) if q.unit.is_none() => Ok(Value::Num(Quantity {
                unit: u.clone(),
                ..q
            })),
            Value::Num(q) => {
                let v = self.ucx().convert(q.v, &q.unit, u)?;
                Ok(Value::Num(Quantity {
                    v,
                    unit: u.clone(),
                    fmt: q.fmt,
                }))
            }
            _ => Err("units only apply to numbers".into()),
        }
    }

    fn percent_factor(&mut self, p: &Expr) -> Result<f64, String> {
        match self.eval(p)? {
            Value::Percent(p) => Ok(p / 100.0),
            _ => Err("expected a percentage".into()),
        }
    }

    fn binop(&mut self, op: BinOp, a: Value, b: Value) -> R {
        match op {
            BinOp::Add => self.add(a, b, false),
            BinOp::Sub => self.add(a, b, true),
            BinOp::Mul => self.mul(a, b, false),
            BinOp::Div => self.mul(a, b, true),
            BinOp::Pow => self.pow(a, b),
            BinOp::Mod => {
                let (Value::Num(x), Value::Num(y)) = (a, b) else {
                    return Err("mod needs numbers".into());
                };
                let yv = if x.unit.is_none() || y.unit.is_none() {
                    y.v
                } else {
                    self.ucx().convert(y.v, &y.unit, &x.unit)?
                };
                if yv == 0.0 {
                    return Err("division by zero".into());
                }
                let unit = if x.unit.is_none() { y.unit } else { x.unit };
                Ok(Value::Num(Quantity {
                    v: x.v % yv,
                    unit,
                    fmt: x.fmt,
                }))
            }
            BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr => {
                let (Value::Num(x), Value::Num(y)) = (a, b) else {
                    return Err("bitwise operations need whole numbers".into());
                };
                let (xi, yi) = (x.v.trunc() as i64, y.v.trunc() as i64);
                let r = match op {
                    BinOp::And => xi & yi,
                    BinOp::Or => xi | yi,
                    BinOp::Xor => xi ^ yi,
                    BinOp::Shl => xi.checked_shl(yi as u32).ok_or("shift too large")?,
                    _ => xi.checked_shr(yi as u32).ok_or("shift too large")?,
                };
                Ok(Value::Num(Quantity { v: r as f64, ..x }))
            }
        }
    }

    fn add(&mut self, a: Value, b: Value, sub: bool) -> R {
        let sign = if sub { -1.0 } else { 1.0 };
        match (a, b) {
            (Value::Num(x), Value::Num(y)) => {
                if x.unit.is_none() {
                    return Ok(Value::Num(Quantity {
                        v: x.v + sign * y.v,
                        unit: y.unit,
                        fmt: x.fmt,
                    }));
                }
                if y.unit.is_none() {
                    return Ok(Value::Num(Quantity {
                        v: x.v + sign * y.v,
                        ..x
                    }));
                }
                let yv = self.ucx().convert(y.v, &y.unit, &x.unit)?;
                Ok(Value::Num(Quantity {
                    v: x.v + sign * yv,
                    ..x
                }))
            }
            (Value::Num(x), Value::Percent(p)) => Ok(Value::Num(Quantity {
                v: x.v * (1.0 + sign * p / 100.0),
                ..x
            })),
            (Value::Percent(p), Value::Percent(q)) => Ok(Value::Percent(p + sign * q)),
            (Value::Percent(p), Value::Num(y)) if y.unit.is_none() => {
                Ok(Value::Percent(p + sign * y.v))
            }
            (Value::Time(t), Value::Num(q)) => self.shift(t, &q, sign),
            (Value::Num(q), Value::Time(t)) if !sub => self.shift(t, &q, 1.0),
            (Value::Time(a), Value::Time(b)) if sub => self.diff(a, b),
            _ => Err("cannot combine these values".into()),
        }
    }

    fn mul(&mut self, a: Value, b: Value, div: bool) -> R {
        match (a, b) {
            (Value::Num(x), Value::Num(y)) => Ok(Value::Num(self.mul_q(x, y, div)?)),
            (Value::Num(x), Value::Percent(p)) => {
                let f = p / 100.0;
                Ok(Value::Num(Quantity {
                    v: if div { x.v / f } else { x.v * f },
                    ..x
                }))
            }
            (Value::Percent(p), Value::Num(y)) if y.unit.is_none() => {
                Ok(Value::Percent(if div { p / y.v } else { p * y.v }))
            }
            (Value::Percent(p), Value::Num(y)) if !div => Ok(Value::Num(Quantity {
                v: y.v * p / 100.0,
                ..y
            })),
            (Value::Percent(p), Value::Percent(q)) => Ok(if div {
                Value::num(p / q)
            } else {
                Value::Percent(p * q / 100.0)
            }),
            _ => Err("cannot multiply these values".into()),
        }
    }

    fn mul_q(&self, x: Quantity, y: Quantity, div: bool) -> Result<Quantity, String> {
        let ucx = self.ucx();
        let (mut yv, mut yu) = (y.v, y.unit.clone());
        // Express y in a unit x already uses (m * cm → m * m).
        if let Some(u2) = yu.as_simple() {
            for (u1, _) in &x.unit.parts {
                if *u1 != u2
                    && ucx.same_kind(*u1, u2)
                    && !is_affine(&ucx, &yu)
                    && !is_affine(&ucx, &Unit::simple(*u1))
                {
                    yv = ucx.convert(yv, &yu, &Unit::simple(*u1))?;
                    yu = Unit::simple(*u1);
                    break;
                }
            }
        }
        let mut unit = if div {
            x.unit.mul(&yu.inv())
        } else {
            x.unit.mul(&yu)
        };
        if div && yv == 0.0 {
            return Err("division by zero".into());
        }
        let mut v = if div { x.v / yv } else { x.v * yv };
        let both = !x.unit.is_none() && !y.unit.is_none();
        if both && !unit.is_none() {
            (v, unit) = ucx.tidy(v, unit)?;
        }
        Ok(Quantity {
            v,
            unit,
            fmt: x.fmt,
        })
    }

    fn pow(&self, a: Value, b: Value) -> R {
        let (Value::Num(x), Value::Num(y)) = (a, b) else {
            return Err("powers need numbers".into());
        };
        if !y.unit.is_none() {
            return Err("exponent must be a plain number".into());
        }
        let unit = if x.unit.is_none() {
            Unit::none()
        } else if y.v.fract() == 0.0 && y.v.abs() < 16.0 {
            x.unit.powi(y.v as i8)
        } else {
            return Err("fractional power of a unit".into());
        };
        Ok(Value::Num(Quantity {
            v: x.v.powf(y.v),
            unit,
            fmt: x.fmt,
        }))
    }

    /// Two values written next to each other: `1 meter 20 cm` adds,
    /// `6 (3)` multiplies, `March 5 2:30 pm` combines date and time.
    fn implicit(&mut self, a: Value, b: Value) -> R {
        match (&a, &b) {
            (Value::Num(x), Value::Num(y)) if !x.unit.is_none() && !y.unit.is_none() => {
                let ucx = self.ucx();
                if ucx.compatible(&x.unit, &y.unit) && !is_affine(&ucx, &x.unit) {
                    let (fx, fy) = (ucx.factor(&x.unit)?, ucx.factor(&y.unit)?);
                    let target = if fy < fx {
                        y.unit.clone()
                    } else {
                        x.unit.clone()
                    };
                    let v =
                        ucx.convert(x.v, &x.unit, &target)? + ucx.convert(y.v, &y.unit, &target)?;
                    return Ok(Value::Num(Quantity {
                        v,
                        unit: target,
                        fmt: x.fmt,
                    }));
                }
                self.mul(a, b, false)
            }
            (Value::Time(d), Value::Time(t)) | (Value::Time(t), Value::Time(d))
                if d.show == Show::Date && t.show == Show::Time =>
            {
                let (doff, _) = d.zone.offset_at(d.utc as i64);
                let (toff, _) = t.zone.offset_at(t.utc as i64);
                let day = split(d.utc + doff as f64).0;
                let sod = split(t.utc + toff as f64).1;
                let utc = t.zone.to_utc(day * 86400 + sod as i64) as f64;
                Ok(Value::Time(DateTime {
                    utc,
                    show: Show::DateTime,
                    ..t.clone()
                }))
            }
            _ => self.mul(a, b, false),
        }
    }

    fn shift(&self, t: DateTime, q: &Quantity, sign: f64) -> R {
        let show = t.show;
        let (nt, whole_days) = self.shift_raw(t, q, sign)?;
        let show = match (show, whole_days) {
            (Show::Date, true) => Show::Date,
            (Show::Date, false) | (Show::Time, true) => Show::DateTime,
            (s, _) => s,
        };
        Ok(Value::Time(DateTime { show, ..nt }))
    }

    /// Moves a date by a duration. Whole days, weeks, months and years move
    /// the wall clock (so they are DST- and month-length-aware).
    fn shift_raw(&self, t: DateTime, q: &Quantity, sign: f64) -> Result<(DateTime, bool), String> {
        let ucx = self.ucx();
        if q.unit.is_none() {
            return Err("add a unit of time, e.g. 2 days".into());
        }
        if ucx.dims(&q.unit) != dim(TIME, 1) {
            return Err("only durations can be added to dates".into());
        }
        let (off, _) = t.zone.offset_at(t.utc.floor() as i64);
        let local = t.utc + off as f64;
        let key = q.unit.as_simple().map(|id| ucx.reg.def(id).key.as_str());
        let n = sign * q.v;
        let new_local = match key {
            Some("month") if q.v.fract() == 0.0 => Some(add_months(local, n as i64)),
            Some("year") if q.v.fract() == 0.0 => Some(add_months(local, n as i64 * 12)),
            Some("day") if q.v.fract() == 0.0 => Some(local + n * 86400.0),
            Some("week") if q.v.fract() == 0.0 => Some(local + n * 604800.0),
            _ => None,
        };
        let whole_days = new_local.is_some();
        let utc = match new_local {
            Some(nl) => t.zone.to_utc(nl.floor() as i64) as f64 + nl.fract(),
            None => t.utc + sign * ucx.convert(q.v, &q.unit, &self.unit("s"))?,
        };
        Ok((
            DateTime {
                utc,
                live: false,
                ..t
            },
            whole_days,
        ))
    }

    fn diff(&self, a: DateTime, b: DateTime) -> R {
        let secs = if a.live && b.live {
            let (oa, _) = a.zone.offset_at(a.utc as i64);
            let (ob, _) = b.zone.offset_at(b.utc as i64);
            (a.utc + oa as f64) - (b.utc + ob as f64)
        } else {
            a.utc - b.utc
        };
        let dates = a.show == Show::Date && b.show == Show::Date;
        let key = if dates || secs.abs() >= 2.0 * 86400.0 {
            "day"
        } else if secs.abs() >= 3600.0 {
            "h"
        } else if secs.abs() >= 60.0 {
            "min"
        } else {
            "s"
        };
        let unit = self.unit(key);
        let mut v = secs / self.ucx().factor(&unit)?;
        if dates {
            v = v.round();
        }
        Ok(Value::qty(v, unit))
    }

    fn convert(&self, v: Value, t: &Target) -> R {
        match (t, v) {
            // A plain number counts single FLOP or tokens: 8.4e22 in EFLOP.
            (Target::Unit(u), Value::Num(q)) if q.unit.is_none() && self.ucx().is_count(u) => {
                Ok(Value::Num(Quantity {
                    v: q.v / self.ucx().factor(u)?,
                    unit: u.clone(),
                    fmt: q.fmt,
                }))
            }
            (Target::Unit(u), v) => self.apply_unit(v, u),
            (Target::Fmt(f), Value::Num(q)) => Ok(Value::Num(Quantity { fmt: *f, ..q })),
            (Target::Fmt(f), Value::Percent(p)) => Ok(Value::Num(Quantity {
                v: p,
                unit: Unit::none(),
                fmt: *f,
            })),
            (Target::Zone(z), Value::Time(t)) => Ok(Value::Time(DateTime {
                zone: z.clone(),
                local: false,
                ..t
            })),
            (Target::Percent, Value::Num(q)) if q.unit.is_none() => Ok(Value::Percent(q.v * 100.0)),
            (Target::Percent, Value::Percent(p)) => Ok(Value::Percent(p)),
            _ => Err("cannot convert this value".into()),
        }
    }

    fn aggregate(&mut self, avg: bool) -> R {
        self.aggregate = true;
        let mut acc: Option<Value> = None;
        let mut count = 0usize;
        for v in self.st.block.clone() {
            acc = match acc {
                None if matches!(v, Value::Num(_) | Value::Percent(_)) => {
                    count = 1;
                    Some(v)
                }
                None => None,
                Some(a) => {
                    let same_kind = matches!(
                        (&a, &v),
                        (Value::Num(_), Value::Num(_)) | (Value::Percent(_), Value::Percent(_))
                    );
                    match same_kind.then(|| self.add(a.clone(), v, false)) {
                        Some(Ok(s)) => {
                            count += 1;
                            Some(s)
                        }
                        _ => Some(a),
                    }
                }
            };
        }
        let total = acc.unwrap_or_else(|| Value::num(0.0));
        if avg && count > 0 {
            return map_num(total, |x| x / count as f64);
        }
        Ok(total)
    }

    fn factorial(&self, v: Value) -> R {
        match v {
            Value::Num(q)
                if q.unit.is_none() && q.v >= 0.0 && q.v.fract() == 0.0 && q.v <= 170.0 =>
            {
                Ok(Value::num((1..=q.v as u64).fold(1.0, |a, b| a * b as f64)))
            }
            _ => Err("factorial needs a whole number between 0 and 170".into()),
        }
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> R {
        if let Some(f) = self.eng.functions.get(name) {
            return self.call_user(f, args);
        }
        let vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval(a))
            .collect::<Result<_, _>>()?;
        match name {
            "sum" | "mean" | "median" | "stdev" | "stddev" | "pstdev" | "variance"
            | "pvariance" | "count" => return self.describe(name, vals),
            "correlation" | "correl" | "corr" | "slope" | "intercept" | "rsquared" | "r2"
            | "predict" | "slope_ci" | "intercept_ci" => return self.regress(name, &vals),
            "pb_slope"
            | "pb_intercept"
            | "pb_slope_ci"
            | "pb_intercept_ci"
            | "deming_slope"
            | "deming_intercept"
            | "deming_slope_ci"
            | "deming_intercept_ci"
            | "ba_bias"
            | "ba_bias_ci"
            | "ba_limits"
            | "ba_lower_ci"
            | "ba_upper_ci" => return self.compare(name, &vals),
            "scatter" | "bland_altman" | "bars" | "line" => return self.chart(name, args, &vals),
            "softmax" | "logsumexp" | "accuracy_ci" | "mcnemar" | "paired_ci" | "items_needed"
            | "holm" => return self.evaluation(name, &vals),
            _ => {}
        }
        let arg = |i: usize| -> Result<Value, String> {
            vals.get(i)
                .cloned()
                .ok_or_else(|| format!("{name} needs more arguments"))
        };
        let plain = |i: usize| -> Result<f64, String> {
            match arg(i)? {
                Value::Num(q) if q.unit.is_none() => Ok(q.v),
                Value::Percent(p) => Ok(p / 100.0),
                _ => Err(format!("{name} expects a plain number")),
            }
        };
        let radians = |i: usize| -> Result<f64, String> {
            match arg(i)? {
                Value::Num(q) if q.unit.is_none() => Ok(q.v),
                Value::Num(q) if self.ucx().dims(&q.unit) == dim(ANGLE, 1) => {
                    self.ucx().convert(q.v, &q.unit, &self.unit("rad"))
                }
                _ => Err(format!("{name} expects an angle")),
            }
        };
        let f1 = |f: fn(f64) -> f64| -> R { Ok(Value::num(f(plain(0)?))) };
        match name {
            "sqrt" | "cbrt" => {
                let n: i8 = if name == "sqrt" { 2 } else { 3 };
                match arg(0)? {
                    Value::Num(q) => {
                        if q.unit.parts.iter().any(|(_, p)| p % n != 0) {
                            return Err(format!("{name} of this unit is not defined"));
                        }
                        let unit = Unit {
                            parts: q.unit.parts.iter().map(|(id, p)| (*id, p / n)).collect(),
                        };
                        let v = if n == 2 { q.v.sqrt() } else { q.v.cbrt() };
                        Ok(Value::Num(Quantity {
                            v,
                            unit,
                            fmt: q.fmt,
                        }))
                    }
                    _ => Err(format!("{name} expects a number")),
                }
            }
            "root" => Ok(Value::num(plain(1)?.powf(1.0 / plain(0)?))),
            "abs" => map_num(arg(0)?, f64::abs),
            "log" if vals.len() >= 2 => Ok(Value::num(plain(1)?.ln() / plain(0)?.ln())),
            "log" => f1(f64::log10),
            "ln" => f1(f64::ln),
            "exp" => f1(f64::exp),
            "log2" => f1(f64::log2),
            "sigmoid" => f1(stats::sigmoid),
            "logit" => match plain(0)? {
                p if p > 0.0 && p < 1.0 => Ok(Value::num((p / (1.0 - p)).ln())),
                _ => Err("logit needs a probability between 0 and 1".into()),
            },
            // Perplexity from a loss in nats per token.
            "perplexity" => f1(f64::exp),
            "fact" | "factorial" => self.factorial(arg(0)?),
            "round" | "ceil" | "floor" => {
                let places = if vals.len() > 1 { plain(1)? as i32 } else { 0 };
                let k = 10f64.powi(places);
                let f: fn(f64) -> f64 = match name {
                    "round" => f64::round,
                    "ceil" => f64::ceil,
                    _ => f64::floor,
                };
                map_num(arg(0)?, |x| f(x * k) / k)
            }
            "sin" => Ok(Value::num(radians(0)?.sin())),
            "cos" => Ok(Value::num(radians(0)?.cos())),
            "tan" => Ok(Value::num(radians(0)?.tan())),
            "arcsin" | "asin" => f1(f64::asin),
            "arccos" | "acos" => f1(f64::acos),
            "arctan" | "atan" => f1(f64::atan),
            "sinh" => f1(f64::sinh),
            "cosh" => f1(f64::cosh),
            "tanh" => f1(f64::tanh),
            "fromunix" => Ok(self.mk_time(plain(0)?, None, Show::DateTime, false)),
            _ => Err(format!("unknown function {name}")),
        }
    }

    fn call_user(&mut self, f: &UserFn, args: &[Expr]) -> R {
        if self.depth > 32 {
            return Err("functions nested too deeply".into());
        }
        let calls = self
            .eng
            .calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if calls >= crate::MAX_CALLS {
            return Err("too many function calls".into());
        }
        if args.len() != f.params.len() {
            return Err(format!(
                "expected {} argument(s), got {}",
                f.params.len(),
                args.len()
            ));
        }
        let vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval(a))
            .collect::<Result<_, _>>()?;
        let mut st = DocState {
            block: Vec::new(),
            ..self.st.clone()
        };
        for (p, v) in f.params.iter().zip(vals) {
            st.vars.insert(p.clone(), v);
        }
        let result = self
            .eng
            .eval_expression(&f.body, &mut st, self.now, self.depth + 1);
        self.st.list_values = st.list_values;
        result
    }

    // --------------------------------------------------------- statistics

    /// Puts numbers and lists into one list, in the unit of the first value
    /// that has one. `strict` fails on a value that does not fit; otherwise
    /// such values are left out, the way `sum` leaves them out.
    fn gather(&self, vals: &[Value], strict: bool) -> Result<List, String> {
        let is_percent = |v: &Value| match v {
            Value::Percent(_) => Some(true),
            Value::Num(_) => Some(false),
            Value::List(l) if !l.v.is_empty() => Some(l.percent),
            _ => None,
        };
        let percent = vals.iter().find_map(is_percent).unwrap_or(false);
        let unit = vals
            .iter()
            .filter(|_| !percent)
            .find_map(|v| match v {
                Value::Num(q) if !q.unit.is_none() => Some(q.unit.clone()),
                Value::List(l) if !l.unit.is_none() => Some(l.unit.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let ucx = self.ucx();
        let fit = |x: f64, from: &Unit| -> Result<f64, String> {
            if from.is_none() || *from == unit {
                Ok(x)
            } else {
                ucx.convert(x, from, &unit)
            }
        };
        let mixed = || Err::<(), String>("cannot mix percentages and numbers".into());
        let mut out = Vec::new();
        for v in vals {
            let before = out.len();
            let added = match v {
                Value::Percent(p) if percent => {
                    out.push(*p);
                    Ok(())
                }
                Value::Num(q) if !percent => fit(q.v, &q.unit).map(|x| out.push(x)),
                Value::List(l) if l.v.is_empty() => Ok(()),
                Value::List(l) if l.percent == percent => {
                    l.v.iter()
                        .try_for_each(|x| fit(*x, &l.unit).map(|x| out.push(x)))
                }
                Value::Time(_) => Err("dates cannot go in a list".into()),
                _ => mixed(),
            };
            match added {
                Ok(()) => {}
                Err(e) if strict => return Err(e),
                Err(_) => out.truncate(before),
            }
            if out.len() > MAX_LIST_VALUES {
                return Err("list is too long".into());
            }
        }
        Ok(List {
            v: out,
            unit,
            percent,
        })
    }

    /// Counts work on `n` numbers against the line's function-call budget,
    /// so that a function calling `median` thousands of times cannot hang
    /// the app.
    fn charge(&self, n: usize) -> Result<(), String> {
        let cost = (n / VALUES_PER_CALL + 1) as u32;
        let calls = self
            .eng
            .calls
            .fetch_add(cost, std::sync::atomic::Ordering::Relaxed);
        if calls >= crate::MAX_CALLS {
            return Err("too many function calls".into());
        }
        Ok(())
    }

    /// One number that describes many: `median(3, 1, 2)`, `stdev(prices)`,
    /// or `median` alone for the lines above.
    fn describe(&mut self, name: &str, vals: Vec<Value>) -> R {
        let averaging = matches!(name, "sum" | "mean");
        if averaging && vals.is_empty() {
            return self.aggregate(name == "mean");
        }
        // `sum (2)` has always meant "the sum, times 2".
        if averaging && vals.len() == 1 && !matches!(vals[0], Value::List(_)) {
            let total = self.aggregate(name == "mean")?;
            return self.implicit(total, vals.into_iter().next().unwrap());
        }
        let list = if vals.is_empty() {
            self.aggregate = true;
            self.gather(&self.st.block, false)?
        } else {
            self.gather(&vals, true)?
        };
        let n = list.v.len();
        self.charge(n)?;
        let at_least = |k: usize| -> Result<(), String> {
            if n >= k {
                return Ok(());
            }
            let values = if k == 1 { "one value" } else { "two values" };
            Err(format!("{name} needs at least {values}"))
        };
        // A variance is in squared units.
        let squared = |x: f64| {
            if list.percent || list.unit.is_none() {
                Value::num(x)
            } else {
                Value::qty(x, list.unit.powi(2))
            }
        };
        match name {
            "count" => Ok(Value::num(n as f64)),
            "sum" => Ok(list.value(list.v.iter().sum())),
            "mean" => at_least(1).map(|()| list.value(stats::mean(&list.v))),
            "median" => at_least(1).map(|()| list.value(stats::median(&list.v))),
            "stdev" | "stddev" => {
                at_least(2).map(|()| list.value(stats::variance(&list.v, true).sqrt()))
            }
            "pstdev" => at_least(1).map(|()| list.value(stats::variance(&list.v, false).sqrt())),
            "variance" => at_least(2).map(|()| squared(stats::variance(&list.v, true))),
            _ => at_least(1).map(|()| squared(stats::variance(&list.v, false))),
        }
    }

    /// The straight line through points given as a list of x and a list of
    /// y: `slope(x, y)`, `intercept(x, y)`, `predict(x, y, 12)`.
    fn regress(&mut self, name: &str, vals: &[Value]) -> R {
        let wanted = if name == "predict" { 3 } else { 2 };
        let (Some(Value::List(x)), Some(Value::List(y)), true) =
            (vals.first(), vals.get(1), vals.len() == wanted)
        else {
            return Err(match name {
                "predict" => "predict needs two lists and a value: predict(x, y, 12)".into(),
                _ => format!("{name} needs two lists: {name}(x, y)"),
            });
        };
        if x.v.len() != y.v.len() {
            return Err("the two lists must hold the same number of values".into());
        }
        if x.v.len() < 2 {
            return Err(format!("{name} needs at least two points"));
        }
        self.charge(x.v.len())?;
        let fit = Fit::new(&x.v, &y.v);
        let no_line = || "there is no line when every x is the same".to_string();
        let no_r = || "there is no correlation when a list never changes".to_string();
        match name {
            "slope" => self.slope_value(fit.slope().ok_or_else(no_line)?, x, y),
            "slope_ci" | "intercept_ci" => {
                fit.slope().ok_or_else(no_line)?;
                let ci = fit
                    .intervals()
                    .ok_or_else(|| format!("{name} needs at least three points"))?;
                if name == "slope_ci" {
                    let lo = self.slope_value(ci.slope.0, x, y)?;
                    let hi = self.slope_value(ci.slope.1, x, y)?;
                    self.interval(lo, hi)
                } else {
                    self.interval(y.value(ci.intercept.0), y.value(ci.intercept.1))
                }
            }
            "intercept" => Ok(y.value(fit.intercept().ok_or_else(no_line)?)),
            "predict" => {
                let at = match &vals[2] {
                    Value::Percent(p) if x.percent => *p,
                    Value::Num(q) if !x.percent && q.unit.is_none() => q.v,
                    Value::Num(q) if !x.percent => self.ucx().convert(q.v, &q.unit, &x.unit)?,
                    _ => return Err("the value for x must be like the values in the list".into()),
                };
                let (a, b) = (
                    fit.intercept().ok_or_else(no_line)?,
                    fit.slope().ok_or_else(no_line)?,
                );
                Ok(y.value(a + b * at))
            }
            "rsquared" | "r2" => Ok(Value::num(fit.correlation().ok_or_else(no_r)?.powi(2))),
            _ => Ok(Value::num(fit.correlation().ok_or_else(no_r)?)),
        }
    }

    /// A least-squares slope in units of y per unit of x.
    fn slope_value(&mut self, b: f64, x: &List, y: &List) -> R {
        let rise = y.value(b);
        if x.unit.is_none() || y.percent {
            return Ok(rise);
        }
        self.mul(rise, Value::qty(1.0, x.unit.clone()), true)
    }

    /// A confidence interval: a list of its lower and upper limit.
    fn interval(&self, lo: Value, hi: Value) -> R {
        Ok(Value::List(Arc::new(self.gather(&[lo, hi], true)?)))
    }

    /// Method comparison, where x and y measure the same thing in two ways:
    /// `pb_slope(x, y)` (Passing-Bablok) and `deming_slope(x, y, ratio)`,
    /// with `_intercept` and `_ci` forms, and Bland-Altman agreement:
    /// `ba_bias(x, y)` and `ba_limits(x, y)` with their intervals. The slope
    /// has no unit; y is put in x's unit first, and differences are y − x.
    fn compare(&mut self, name: &str, vals: &[Value]) -> R {
        let deming = name.starts_with("deming");
        let most = if deming { 3 } else { 2 };
        let (Some(Value::List(x)), Some(Value::List(y)), true) =
            (vals.first(), vals.get(1), (2..=most).contains(&vals.len()))
        else {
            return Err(if deming {
                format!("{name} needs two lists, and an error ratio if it is not 1: {name}(x, y)")
            } else {
                format!("{name} needs two lists: {name}(x, y)")
            });
        };
        let (x, y) = (x.clone(), y.clone());
        if x.v.len() != y.v.len() {
            return Err("the two lists must hold the same number of values".into());
        }
        if x.v.len() < 2 {
            return Err(format!("{name} needs at least two points"));
        }
        if x.percent != y.percent {
            return Err("cannot mix percentages and numbers".into());
        }
        let unit = if x.unit.is_none() { &y.unit } else { &x.unit };
        let units = List {
            v: Vec::new(),
            unit: unit.clone(),
            percent: x.percent,
        };
        let convert = |l: &List| -> Result<Vec<f64>, String> {
            if l.unit.is_none() || l.unit == units.unit {
                return Ok(l.v.clone());
            }
            let ucx = self.ucx();
            l.v.iter()
                .map(|v| ucx.convert(*v, &l.unit, &units.unit))
                .collect::<Result<_, _>>()
                .map_err(|_| "both lists need the same kind of unit".to_string())
        };
        let (xv, yv) = (convert(&x)?, convert(&y)?);
        let n = xv.len();
        if name.starts_with("ba_") {
            self.charge(n)?;
            let a = stats::bland_altman(&xv, &yv)
                .ok_or_else(|| format!("{name} needs at least two points"))?;
            let (lo, hi) = match name {
                "ba_bias" => return Ok(units.value(a.bias)),
                "ba_bias_ci" => a.bias_ci,
                "ba_limits" => (a.lower, a.upper),
                "ba_lower_ci" => a.lower_ci,
                _ => a.upper_ci,
            };
            return self.interval(units.value(lo), units.value(hi));
        }
        let line = if deming {
            let ratio = match vals.get(2) {
                None => 1.0,
                Some(Value::Num(q)) if q.unit.is_none() && q.v.is_finite() && q.v > 0.0 => q.v,
                Some(_) => return Err("the error ratio must be a positive number".into()),
            };
            self.charge(n)?;
            stats::deming(&xv, &yv, ratio)
                .ok_or("there is no line when x and y do not vary together")?
        } else {
            if n > PB_MAX_POINTS {
                return Err(format!("{name} works on up to 1,000 points"));
            }
            let cached = match &self.st.pb_cache {
                Some((cx, cy, fit)) if Arc::ptr_eq(cx, &x) && Arc::ptr_eq(cy, &y) => Some(*fit),
                _ => None,
            };
            let fit = match cached {
                Some(fit) => {
                    self.charge(n)?;
                    fit
                }
                None => {
                    self.charge(n * (n - 1) / 2)?;
                    let fit = stats::passing_bablok(&xv, &yv);
                    self.st.pb_cache = Some((x.clone(), y.clone(), fit));
                    fit
                }
            };
            fit.map_err(|e| match e {
                PbError::TooFewPoints => "there is no line when every point is the same",
                PbError::Negative => "Passing-Bablok needs x and y that rise together",
            })?
        };
        let ci = || {
            line.ci.ok_or_else(|| {
                if deming {
                    format!("{name} needs at least three points")
                } else {
                    format!("{name} needs more points for a confidence interval")
                }
            })
        };
        match name.split_once('_').map_or("", |p| p.1) {
            "slope" => Ok(Value::num(line.slope)),
            "intercept" => Ok(units.value(line.intercept)),
            "slope_ci" => {
                let (lo, hi) = ci()?.slope;
                self.interval(Value::num(lo), Value::num(hi))
            }
            _ => {
                let (lo, hi) = ci()?.intercept;
                self.interval(units.value(lo), units.value(hi))
            }
        }
    }

    /// Machine-learning helpers and statistics for comparing models on an
    /// evaluation set.
    fn evaluation(&mut self, name: &str, vals: &[Value]) -> R {
        let count = |v: &Value, what: &str| -> Result<f64, String> {
            match v {
                Value::Num(q) if q.unit.is_none() && q.v >= 0.0 && q.v.fract() == 0.0 => Ok(q.v),
                _ => Err(format!("{what} must be a whole number of items")),
            }
        };
        let rate = |v: &Value| -> Result<f64, String> {
            match v {
                Value::Percent(p) if *p > 0.0 && *p < 100.0 => Ok(p / 100.0),
                Value::Num(q) if q.unit.is_none() && q.v > 0.0 && q.v < 1.0 => Ok(q.v),
                _ => Err(format!("{name} needs percentages between 0% and 100%")),
            }
        };
        let plain_list = |l: &List| -> Result<Vec<f64>, String> {
            if !l.unit.is_none() {
                return Err(format!("{name} works on plain numbers"));
            }
            Ok(if l.percent {
                l.v.iter().map(|p| p / 100.0).collect()
            } else {
                l.v.clone()
            })
        };
        let pair = |lo: f64, hi: f64, l: &List| {
            Value::List(Arc::new(List {
                v: vec![lo, hi],
                unit: l.unit.clone(),
                percent: l.percent,
            }))
        };
        match name {
            "softmax" | "logsumexp" | "holm" => {
                let list = self.gather(vals, true)?;
                if list.v.is_empty() {
                    return Err(format!("{name} needs at least one value"));
                }
                self.charge(list.v.len())?;
                let v = plain_list(&list)?;
                let plain = |v: Vec<f64>| {
                    Value::List(Arc::new(List {
                        v,
                        unit: Unit::none(),
                        percent: false,
                    }))
                };
                match name {
                    "softmax" => Ok(plain(stats::softmax(&v))),
                    "logsumexp" => Ok(Value::num(stats::logsumexp(&v))),
                    _ => {
                        if v.iter().any(|p| !(0.0..=1.0).contains(p)) {
                            return Err("holm needs p-values between 0 and 1".into());
                        }
                        Ok(plain(stats::holm(&v)))
                    }
                }
            }
            "accuracy_ci" => {
                let [k, n] = vals else {
                    return Err("accuracy_ci needs correct and total: accuracy_ci(412, 500)".into());
                };
                let n = count(n, "the total")?;
                let k = match k {
                    Value::Percent(p) if (0.0..=100.0).contains(p) => (p / 100.0 * n).round(),
                    k => count(k, "the number correct")?,
                };
                if n == 0.0 || k > n {
                    return Err("accuracy_ci needs correct ≤ total, and a total above 0".into());
                }
                let (lo, hi) = stats::wilson(k, n);
                Ok(pair(
                    lo * 100.0,
                    hi * 100.0,
                    &List {
                        v: Vec::new(),
                        unit: Unit::none(),
                        percent: true,
                    },
                ))
            }
            "mcnemar" => match vals {
                [Value::List(a), Value::List(b)] => {
                    let (a, b) = (plain_list(a)?, plain_list(b)?);
                    if a.len() != b.len() {
                        return Err("the two lists must hold the same number of values".into());
                    }
                    if a.iter().chain(&b).any(|v| *v != 0.0 && *v != 1.0) {
                        return Err("mcnemar needs lists of 1 (right) and 0 (wrong)".into());
                    }
                    self.charge(a.len())?;
                    let only = |x: &[f64], y: &[f64]| {
                        x.iter()
                            .zip(y)
                            .filter(|(p, q)| **p == 1.0 && **q == 0.0)
                            .count() as u64
                    };
                    Ok(Value::num(stats::mcnemar(only(&a, &b), only(&b, &a))))
                }
                [b, c] => {
                    let b = count(b, "each count")?;
                    let c = count(c, "each count")?;
                    if b + c > 1e15 {
                        return Err("too many items".into());
                    }
                    Ok(Value::num(stats::mcnemar(b as u64, c as u64)))
                }
                _ => {
                    Err("mcnemar needs two counts of disagreements, or two lists of 1 and 0".into())
                }
            },
            "paired_ci" => {
                let [Value::List(a), Value::List(b)] = vals else {
                    return Err("paired_ci needs two lists: paired_ci(a, b)".into());
                };
                if a.v.len() != b.v.len() {
                    return Err("the two lists must hold the same number of values".into());
                }
                if a.percent != b.percent {
                    return Err("cannot mix percentages and numbers".into());
                }
                self.charge(a.v.len())?;
                // b in a's unit (or a in b's, when only b has one).
                let (to, from) = if a.unit.is_none() { (b, a) } else { (a, b) };
                let ucx = self.ucx();
                let moved: Vec<f64> = if from.unit.is_none() || from.unit == to.unit {
                    from.v.clone()
                } else {
                    from.v
                        .iter()
                        .map(|v| ucx.convert(*v, &from.unit, &to.unit))
                        .collect::<Result<_, _>>()?
                };
                let (av, bv) = if std::ptr::eq(to, a) {
                    (&a.v, &moved)
                } else {
                    (&moved, &b.v)
                };
                let (lo, hi) =
                    stats::paired_interval(av, bv).ok_or("paired_ci needs at least two items")?;
                Ok(pair(lo, hi, to))
            }
            _ => {
                let [p, m] = vals else {
                    return Err(
                        "items_needed needs an accuracy and a margin: items_needed(80%, 2%)".into(),
                    );
                };
                Ok(Value::num(stats::items_needed(rate(p)?, rate(m)?)))
            }
        }
    }

    /// Charts: `scatter(x, y)`, `bland_altman(x, y)`, `bars(list)` and
    /// `line(list)`. A list passed by name gives the axis its title.
    fn chart(&mut self, name: &str, args: &[Expr], vals: &[Value]) -> R {
        use crate::chart::{self, ChartKind};
        let named = |i: usize, or: &str| match args.get(i) {
            Some(Expr::Var(n)) => n.clone(),
            _ => or.to_string(),
        };
        let ucx = self.ucx();
        let unit_label = |l: &List| {
            if l.percent {
                "%".to_string()
            } else if l.unit.is_none() {
                String::new()
            } else {
                ucx.label(&l.unit, false)
            }
        };
        let made = match name {
            "bars" | "line" => {
                let list = self.gather(vals, true)?;
                if list.v.is_empty() {
                    return Err(format!("{name} needs values: {name}(list)"));
                }
                self.charge(list.v.len())?;
                let title = if args.len() == 1 {
                    named(0, "")
                } else {
                    String::new()
                };
                let kind = if name == "bars" {
                    ChartKind::Bars
                } else {
                    ChartKind::Line
                };
                chart::series(kind, &list.v, &title, &unit_label(&list))
            }
            _ => {
                let (Some(Value::List(x)), Some(Value::List(y)), 2) =
                    (vals.first(), vals.get(1), vals.len())
                else {
                    return Err(format!("{name} needs two lists: {name}(x, y)"));
                };
                let n = x.v.len();
                if y.v.len() != n {
                    return Err("the two lists must hold the same number of values".into());
                }
                if n < 2 {
                    return Err(format!("{name} needs at least two points"));
                }
                // The same kind of quantity, as in a method comparison: the
                // same dimension, or plain numbers whose ranges overlap.
                let range = |v: &[f64]| {
                    v.iter()
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), a| {
                            (l.min(*a), h.max(*a))
                        })
                };
                let ((x0, x1), (y0, y1)) = (range(&x.v), range(&y.v));
                let overlap = x0.max(y0) <= x1.min(y1);
                let same = x.percent == y.percent
                    && if x.unit.is_none() || y.unit.is_none() {
                        overlap
                    } else {
                        ucx.compatible(&x.unit, &y.unit)
                    };
                // y in x's unit when they measure the same thing.
                let yv: Vec<f64> =
                    if same && !x.unit.is_none() && !y.unit.is_none() && x.unit != y.unit {
                        y.v.iter()
                            .map(|v| ucx.convert(*v, &y.unit, &x.unit))
                            .collect::<Result<_, _>>()?
                    } else {
                        y.v.clone()
                    };
                let (xn, yn) = (named(0, "x"), named(1, "y"));
                let xu = unit_label(x);
                let yu = if same && !x.unit.is_none() {
                    xu.clone()
                } else {
                    unit_label(y)
                };
                if name == "scatter" {
                    self.charge(if same { n * (n - 1) / 2 } else { n })?;
                    chart::scatter(&x.v, &yv, (&xn, &yn), (&xu, &yu), same)
                } else {
                    if !same && !(x.unit.is_none() && y.unit.is_none()) {
                        return Err("both lists need the same kind of unit".into());
                    }
                    self.charge(n)?;
                    let unit = if xu.is_empty() {
                        yu.clone()
                    } else {
                        xu.clone()
                    };
                    chart::bland_altman(&x.v, &yv, (&xn, &yn), &unit)
                        .ok_or_else(|| format!("{name} needs at least two points"))?
                }
            }
        };
        Ok(Value::Chart(Arc::new(made)))
    }
}
