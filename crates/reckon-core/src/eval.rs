//! Evaluates parsed expressions into values.

use std::collections::HashMap;

use crate::parser::{BinOp, Expr, Pct, Target};
use crate::time::{add_months, civil_from_days, days_from_civil, days_in_month, split};
use crate::tz::Zone;
use crate::units::{dim, Conv, Unit, UnitCtx, ANGLE, DIM_COUNT, TIME};
use crate::value::{DateTime, Quantity, Show, Value};
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
}

impl Default for DocState {
    fn default() -> Self {
        DocState {
            vars: HashMap::new(),
            ppi: 96.0,
            em_px: 16.0,
            prev: None,
            block: Vec::new(),
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
        Value::Time(_) => Err("expected a number".into()),
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
                    && ucx.reg.def(*u1).dims == ucx.reg.def(u2).dims
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
        if both && !unit.is_none() && ucx.dims(&unit) == [0; DIM_COUNT] {
            v *= ucx.factor(&unit)?;
            unit = Unit::none();
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
                None if !matches!(v, Value::Time(_)) => {
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
        self.eng
            .eval_expression(&f.body, &mut st, self.now, self.depth + 1)
    }
}
