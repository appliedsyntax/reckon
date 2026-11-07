//! Word classification and recursive-descent parsing.
//!
//! Classification turns raw tokens into meaningful items (units, keywords,
//! variables, zones, ...) using longest-phrase matching, and silently drops
//! words it does not recognise. That is what lets people write prose such
//! as `prev - 5% discount` and still get a result.

use std::ops::Range;

use crate::currency::symbol_code;
use crate::lexer::{Tok, Token};
use crate::time::month_from_name;
use crate::tz::{max_place_words, Zone, ZoneDb};
use crate::units::{dim, Registry, Unit, MONEY};
use crate::value::NumFmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    In,
    As,
    A,
    Of,
    On,
    Off,
    WhatIs,
    Plus,
    Minus,
    Times,
    Divide,
    Mod,
    Xor,
    Per,
    Prev,
    Sum,
    Avg,
    Now,
    Time,
    Today,
    Tomorrow,
    Yesterday,
    Am,
    Pm,
    Ago,
    FromNow,
    Fmt(NumFmt),
}

#[derive(Debug, Clone)]
pub enum Item {
    Num(f64, u32),
    Time(u32, u32, u32),
    Date(i32, u32, u32),
    Sym(char),
    Shl,
    Shr,
    Kw(Kw),
    Unit(Unit),
    /// A currency sign such as `$` (prefix or postfix).
    CurSym(Unit),
    Scale(f64),
    Func(String),
    Var(String),
    Const(f64),
    Zone(Zone),
    Month(u32),
    /// Unknown word kept only as an assignment target.
    Ident(String),
}

#[derive(Debug, Clone)]
pub struct It {
    pub item: Item,
    pub span: Range<usize>,
}

/// Highlight classes for editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hl {
    Number,
    Unit,
    Keyword,
    Variable,
    Function,
    Operator,
    Label,
    Comment,
    Header,
    Error,
}

pub struct ClassifyCtx<'a> {
    pub reg: &'a Registry,
    pub zones: &'a ZoneDb,
    pub is_var: &'a dyn Fn(&str) -> bool,
    pub is_func: &'a dyn Fn(&str) -> bool,
}

pub const BUILTIN_FUNCS: &[&str] = &[
    "sqrt",
    "cbrt",
    "root",
    "abs",
    "log",
    "ln",
    "exp",
    "fact",
    "factorial",
    "round",
    "ceil",
    "floor",
    "sin",
    "cos",
    "tan",
    "arcsin",
    "asin",
    "arccos",
    "acos",
    "arctan",
    "atan",
    "sinh",
    "cosh",
    "tanh",
    "fromunix",
];

fn keyword(lw: &str) -> Option<Kw> {
    Some(match lw {
        "in" | "into" | "to" => Kw::In,
        "as" => Kw::As,
        "of" => Kw::Of,
        "on" => Kw::On,
        "off" => Kw::Off,
        "plus" | "and" | "with" => Kw::Plus,
        "minus" | "subtract" | "without" => Kw::Minus,
        "times" | "mul" => Kw::Times,
        "divide" | "over" => Kw::Divide,
        "mod" | "modulo" => Kw::Mod,
        "xor" => Kw::Xor,
        "per" => Kw::Per,
        "prev" | "previous" | "ans" => Kw::Prev,
        "sum" | "total" => Kw::Sum,
        "average" | "avg" | "mean" => Kw::Avg,
        "now" => Kw::Now,
        "time" => Kw::Time,
        "today" => Kw::Today,
        "tomorrow" => Kw::Tomorrow,
        "yesterday" => Kw::Yesterday,
        "am" => Kw::Am,
        "pm" => Kw::Pm,
        "ago" => Kw::Ago,
        _ => return None,
    })
}

fn format_word(lw: &str) -> Option<NumFmt> {
    Some(match lw {
        "hex" | "hexadecimal" => NumFmt::Hex,
        "bin" | "binary" => NumFmt::Bin,
        "oct" | "octal" => NumFmt::Oct,
        "dec" | "decimal" => NumFmt::Auto,
        "sci" | "scientific" => NumFmt::Sci,
        _ => return None,
    })
}

fn scale(w: &str) -> Option<f64> {
    Some(match w {
        "k" | "thousand" | "thousands" => 1e3,
        "M" | "million" | "millions" | "mln" => 1e6,
        "billion" | "billions" | "bn" => 1e9,
        "trillion" | "trillions" => 1e12,
        _ => return None,
    })
}

fn constant(w: &str) -> Option<f64> {
    Some(match w {
        "pi" | "PI" | "Pi" | "π" => std::f64::consts::PI,
        "e" | "E" => std::f64::consts::E,
        _ => return None,
    })
}

/// Classifies tokens. Also returns highlight spans for dropped words.
pub fn classify(toks: &[Token], cx: &ClassifyCtx) -> Vec<It> {
    let mut out: Vec<It> = Vec::new();
    let max_words = cx.reg.max_words.max(max_place_words()).max(2);
    let word_at = |i: usize| -> Option<&str> {
        match toks.get(i).map(|t| &t.tok) {
            Some(Tok::Word(w)) => Some(w.as_str()),
            _ => None,
        }
    };
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let span = t.span.clone();
        let prev_item = out.last().map(|x| x.item.clone());
        let push = |out: &mut Vec<It>, item: Item, end: usize| {
            out.push(It {
                item,
                span: span.start..end,
            })
        };
        match &t.tok {
            Tok::Num { value, radix } => push(&mut out, Item::Num(*value, *radix), span.end),
            Tok::Time { h, m, s } => push(&mut out, Item::Time(*h, *m, *s), span.end),
            Tok::Date { y, m, d } => push(&mut out, Item::Date(*y, *m, *d), span.end),
            Tok::Shl => push(&mut out, Item::Shl, span.end),
            Tok::Shr => push(&mut out, Item::Shr, span.end),
            Tok::Sym(c) => {
                if let Some(id) = symbol_code(*c).and_then(|code| cx.reg.by_key(code)) {
                    push(&mut out, Item::CurSym(Unit::simple(id)), span.end);
                } else if *c == '°' {
                    let u = cx.reg.by_key("deg").map(Unit::simple).unwrap_or_default();
                    push(&mut out, Item::Unit(u), span.end);
                } else if *c == ',' && !matches!(prev_item, Some(Item::Num(..))) && !in_call(&out) {
                    // stray comma: ignore
                } else {
                    push(&mut out, Item::Sym(*c), span.end);
                }
            }
            Tok::Word(w) => {
                // Assignment target: `name = ...` at the start of the line.
                if i == 0 && matches!(toks.get(1).map(|t| &t.tok), Some(Tok::Sym('='))) {
                    push(&mut out, Item::Ident(w.clone()), span.end);
                    i += 1;
                    continue;
                }
                // UTC+5 / GMT-3
                if (w == "UTC" || w == "GMT") && i + 2 < toks.len() {
                    if let (Tok::Sym(sign @ ('+' | '-')), Tok::Num { value, .. }) =
                        (&toks[i + 1].tok, &toks[i + 2].tok)
                    {
                        if toks[i + 1].span.start == span.end
                            && toks[i + 2].span.start == toks[i + 1].span.end
                        {
                            let mins = (value * 60.0).round() as i32;
                            let off = if *sign == '-' { -mins } else { mins } * 60;
                            let name = format!("{w}{}", &crate::tz::format_offset(off)[3..]);
                            push(
                                &mut out,
                                Item::Zone(Zone::Fixed { off, name }),
                                toks[i + 2].span.end,
                            );
                            i += 3;
                            continue;
                        }
                    }
                }
                // Longest multi-word phrase first.
                let mut matched = false;
                for n in (2..=max_words).rev() {
                    let words: Option<Vec<&str>> = (i..i + n).map(word_at).collect();
                    let Some(words) = words else { continue };
                    let phrase = words.join(" ");
                    let lower = phrase.to_lowercase();
                    let end = toks[i + n - 1].span.end;
                    let item = match lower.as_str() {
                        "multiplied by" => Some(Item::Kw(Kw::Times)),
                        "divided by" | "divide by" => Some(Item::Kw(Kw::Divide)),
                        "from now" => Some(Item::Kw(Kw::FromNow)),
                        "what is"
                            if matches!(prev_item, Some(Item::Kw(Kw::Of | Kw::On | Kw::Off))) =>
                        {
                            Some(Item::Kw(Kw::WhatIs))
                        }
                        _ => None,
                    }
                    .or_else(|| square_cubic(&words, cx.reg))
                    .or_else(|| cx.reg.lookup(&phrase).map(Item::Unit))
                    .or_else(|| cx.zones.resolve(&phrase).map(Item::Zone));
                    if let Some(item) = item {
                        push(&mut out, item, end);
                        i += n;
                        matched = true;
                        break;
                    }
                }
                if matched {
                    continue;
                }
                let lw = w.to_lowercase();
                let next_is_num = matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::Num { .. }));
                let prev_is_num = matches!(prev_item, Some(Item::Num(..)));
                let item =
                    if matches!(prev_item, Some(Item::Kw(Kw::In))) && format_word(&lw).is_some() {
                        Some(Item::Kw(Kw::Fmt(format_word(&lw).unwrap())))
                    } else if (cx.is_var)(w) {
                        Some(Item::Var(w.clone()))
                    } else if lw == "a" {
                        matches!(prev_item, Some(Item::Kw(Kw::As))).then_some(Item::Kw(Kw::A))
                    } else if lw == "percent" || lw == "pct" {
                        Some(Item::Sym('%'))
                    } else if let Some(k) = keyword(&lw) {
                        Some(Item::Kw(k))
                    } else if (cx.is_func)(&lw) {
                        Some(Item::Func(lw.clone()))
                    } else if prev_is_num && scale(w).is_some() {
                        Some(Item::Scale(scale(w).unwrap()))
                    } else if let Some(c) = constant(w) {
                        Some(Item::Const(c))
                    } else if let Some(u) = cx.reg.lookup(w) {
                        Some(Item::Unit(u))
                    } else if let Some(z) = cx.zones.resolve(w) {
                        Some(Item::Zone(z))
                    } else {
                        month_from_name(w)
                            .filter(|_| next_is_num || prev_is_num)
                            .map(Item::Month)
                    };
                if let Some(item) = item {
                    push(&mut out, item, span.end);
                }
            }
        }
        i += 1;
    }
    out
}

fn in_call(out: &[It]) -> bool {
    let mut depth = 0i32;
    for it in out.iter().rev() {
        match it.item {
            Item::Sym(')') => depth += 1,
            Item::Sym('(') if depth == 0 => return true,
            Item::Sym('(') => depth -= 1,
            _ => {}
        }
    }
    false
}

/// `sq cm`, `square inches`, `cubic feet`, `cu m`.
fn square_cubic(words: &[&str], reg: &Registry) -> Option<Item> {
    let (first, rest) = words.split_first()?;
    let p = match first.to_lowercase().as_str() {
        "sq" | "square" => 2,
        "cu" | "cb" | "cubic" => 3,
        _ => return None,
    };
    let id = reg.length_unit(&rest.join(" "))?;
    Some(Item::Unit(Unit::pow_of(id, p)))
}

pub fn highlight_class(item: &Item) -> Hl {
    match item {
        Item::Num(..) | Item::Time(..) | Item::Date(..) | Item::Const(_) | Item::Scale(_) => {
            Hl::Number
        }
        Item::Unit(_) | Item::CurSym(_) | Item::Zone(_) | Item::Month(_) => Hl::Unit,
        Item::Kw(_) => Hl::Keyword,
        Item::Var(_) | Item::Ident(_) => Hl::Variable,
        Item::Func(_) => Hl::Function,
        Item::Sym(_) | Item::Shl | Item::Shr => Hl::Operator,
    }
}

// ---------------------------------------------------------------- AST

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pct {
    Of,
    On,
    Off,
}

#[derive(Debug, Clone)]
pub enum Target {
    Unit(Unit),
    Fmt(NumFmt),
    Zone(Zone),
    Percent,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    WithUnit(Box<Expr>, Unit),
    Percent(Box<Expr>),
    Var(String),
    Assign(String, Box<Expr>),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Implicit(Box<Expr>, Box<Expr>),
    PctOp(Pct, Box<Expr>, Box<Expr>),
    PctInv(Pct, Box<Expr>, Box<Expr>),
    AsPct(Pct, Box<Expr>, Box<Expr>),
    Convert(Box<Expr>, Target),
    Call(String, Vec<Expr>),
    Fact(Box<Expr>),
    Prev,
    Sum,
    Avg,
    Now(Option<Zone>),
    Day(i64),
    TimeLit(u32, u32, u32),
    /// Year 0 means "the current year".
    DateLit(i32, u32, u32),
    InZone(Box<Expr>, Zone),
    Ago(Box<Expr>),
    FromNow(Box<Expr>),
}

impl Expr {
    fn is_timeish(&self) -> bool {
        match self {
            Expr::Now(_) | Expr::Day(_) | Expr::TimeLit(..) | Expr::DateLit(..) => true,
            Expr::Implicit(a, b) => a.is_timeish() || b.is_timeish(),
            _ => false,
        }
    }
}

pub struct Parser<'a> {
    items: &'a [It],
    pos: usize,
    reg: &'a Registry,
}

type PResult = Result<Expr, String>;

impl<'a> Parser<'a> {
    pub fn new(items: &'a [It], reg: &'a Registry) -> Self {
        Parser { items, pos: 0, reg }
    }

    fn peek(&self) -> Option<&Item> {
        self.items.get(self.pos).map(|i| &i.item)
    }
    fn peek_at(&self, n: usize) -> Option<&Item> {
        self.items.get(self.pos + n).map(|i| &i.item)
    }
    fn bump(&mut self) -> Option<&Item> {
        let it = self.items.get(self.pos).map(|i| &i.item);
        self.pos += 1;
        it
    }
    fn is_sym(&self, c: char) -> bool {
        matches!(self.peek(), Some(Item::Sym(x)) if *x == c)
    }
    fn is_kw(&self, k: Kw) -> bool {
        matches!(self.peek(), Some(Item::Kw(x)) if *x == k)
    }

    /// Parses a whole line. Anything after a top-level `=` is ignored so that
    /// annotated examples like `1 m 20 cm = 120 cm` still evaluate.
    pub fn parse_line(&mut self) -> PResult {
        if self.items.is_empty() {
            return Err("empty".into());
        }
        if let (Some(Item::Ident(name) | Item::Var(name)), Some(Item::Sym('='))) =
            (self.peek(), self.peek_at(1))
        {
            let name = name.clone();
            self.pos += 2;
            let e = self.parse_convert()?;
            self.finish()?;
            return Ok(Expr::Assign(name, Box::new(e)));
        }
        let e = self.parse_convert()?;
        self.finish()?;
        Ok(e)
    }

    fn finish(&self) -> Result<(), String> {
        match self.peek() {
            None | Some(Item::Sym('=')) => Ok(()),
            Some(other) => Err(format!("unexpected {other:?}")),
        }
    }

    pub fn parse_convert(&mut self) -> PResult {
        let mut e = self.parse_bitor()?;
        loop {
            if self.is_kw(Kw::As) {
                // `A as a % of B`
                let mut k = 1;
                if matches!(self.peek_at(k), Some(Item::Kw(Kw::A))) {
                    k += 1;
                }
                if matches!(self.peek_at(k), Some(Item::Sym('%'))) {
                    let op = match self.peek_at(k + 1) {
                        Some(Item::Kw(Kw::Of)) => Some(Pct::Of),
                        Some(Item::Kw(Kw::On)) => Some(Pct::On),
                        Some(Item::Kw(Kw::Off)) => Some(Pct::Off),
                        _ => None,
                    };
                    if let Some(op) = op {
                        self.pos += k + 2;
                        let rhs = self.parse_bitor()?;
                        e = Expr::AsPct(op, Box::new(e), Box::new(rhs));
                        continue;
                    }
                }
            }
            if self.is_kw(Kw::In) || self.is_kw(Kw::As) {
                self.pos += 1;
                let t = self.parse_target()?;
                e = Expr::Convert(Box::new(e), t);
                continue;
            }
            break;
        }
        Ok(e)
    }

    fn parse_target(&mut self) -> Result<Target, String> {
        match self.peek().cloned() {
            Some(Item::Kw(Kw::Fmt(f))) => {
                self.pos += 1;
                Ok(Target::Fmt(f))
            }
            Some(Item::Sym('%')) => {
                self.pos += 1;
                Ok(Target::Percent)
            }
            Some(Item::Zone(z)) => {
                self.pos += 1;
                if self.is_kw(Kw::Time) {
                    self.pos += 1;
                }
                Ok(Target::Zone(z))
            }
            Some(Item::Unit(_) | Item::CurSym(_)) => Ok(Target::Unit(self.parse_unit_expr()?)),
            other => Err(format!("cannot convert to {other:?}")),
        }
    }

    fn unit_atom(&mut self) -> Option<Unit> {
        let mut u = match self.peek() {
            Some(Item::Unit(u) | Item::CurSym(u)) => u.clone(),
            _ => return None,
        };
        self.pos += 1;
        if self.is_sym('^') {
            if let Some(Item::Num(n, _)) = self.peek_at(1) {
                if n.fract() == 0.0 && n.abs() < 10.0 {
                    u = u.powi(*n as i8);
                    self.pos += 2;
                }
            }
        }
        Some(u)
    }

    fn parse_unit_expr(&mut self) -> Result<Unit, String> {
        let mut u = self.unit_atom().ok_or("expected a unit")?;
        loop {
            if self.is_sym('/') || self.is_kw(Kw::Per) {
                if matches!(self.peek_at(1), Some(Item::Unit(_) | Item::CurSym(_))) {
                    self.pos += 1;
                    let d = self.unit_atom().unwrap();
                    u = u.mul(&d.inv());
                    continue;
                }
            } else if self.is_sym('*') && matches!(self.peek_at(1), Some(Item::Unit(_))) {
                self.pos += 1;
                let d = self.unit_atom().unwrap();
                u = u.mul(&d);
                continue;
            } else if let Some(d) = self.unit_atom() {
                u = u.mul(&d);
                continue;
            }
            break;
        }
        Ok(u)
    }

    fn binary_loop(
        &mut self,
        next: fn(&mut Self) -> PResult,
        ops: fn(&Item) -> Option<BinOp>,
    ) -> PResult {
        let mut e = next(self)?;
        while let Some(op) = self.peek().and_then(ops) {
            self.pos += 1;
            let rhs = next(self)?;
            e = Expr::Bin(op, Box::new(e), Box::new(rhs));
        }
        Ok(e)
    }

    fn parse_bitor(&mut self) -> PResult {
        self.binary_loop(Self::parse_bitxor, |i| {
            matches!(i, Item::Sym('|')).then_some(BinOp::Or)
        })
    }
    fn parse_bitxor(&mut self) -> PResult {
        self.binary_loop(Self::parse_bitand, |i| {
            matches!(i, Item::Kw(Kw::Xor)).then_some(BinOp::Xor)
        })
    }
    fn parse_bitand(&mut self) -> PResult {
        self.binary_loop(Self::parse_shift, |i| {
            matches!(i, Item::Sym('&')).then_some(BinOp::And)
        })
    }
    fn parse_shift(&mut self) -> PResult {
        self.binary_loop(Self::parse_add, |i| match i {
            Item::Shl => Some(BinOp::Shl),
            Item::Shr => Some(BinOp::Shr),
            _ => None,
        })
    }
    fn parse_add(&mut self) -> PResult {
        self.binary_loop(Self::parse_mul, |i| match i {
            Item::Sym('+') | Item::Kw(Kw::Plus) => Some(BinOp::Add),
            Item::Sym('-') | Item::Kw(Kw::Minus) => Some(BinOp::Sub),
            _ => None,
        })
    }

    fn starts_operand(&self) -> bool {
        match self.peek() {
            Some(
                Item::Num(..)
                | Item::Time(..)
                | Item::Date(..)
                | Item::CurSym(_)
                | Item::Func(_)
                | Item::Var(_),
            )
            | Some(Item::Const(_) | Item::Zone(_) | Item::Month(_) | Item::Sym('(')) => true,
            Some(Item::Kw(k)) => {
                matches!(
                    k,
                    Kw::Prev
                        | Kw::Sum
                        | Kw::Avg
                        | Kw::Now
                        | Kw::Time
                        | Kw::Today
                        | Kw::Tomorrow
                        | Kw::Yesterday
                )
            }
            _ => false,
        }
    }

    fn parse_mul(&mut self) -> PResult {
        let mut e = self.parse_pct()?;
        loop {
            let op = match self.peek() {
                Some(Item::Sym('*') | Item::Kw(Kw::Times)) => Some(BinOp::Mul),
                Some(Item::Sym('/') | Item::Kw(Kw::Divide) | Item::Kw(Kw::Per)) => Some(BinOp::Div),
                Some(Item::Kw(Kw::Mod)) => Some(BinOp::Mod),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                // `km per hour` → divide by a bare unit
                let rhs = self.parse_pct()?;
                e = Expr::Bin(op, Box::new(e), Box::new(rhs));
            } else if self.starts_operand() {
                let rhs = self.parse_pct()?;
                e = Expr::Implicit(Box::new(e), Box::new(rhs));
            } else {
                break;
            }
        }
        Ok(e)
    }

    /// `X% of Y`, `X% on Y`, `X% off Y`, and `... of what is Y`.
    fn parse_pct(&mut self) -> PResult {
        let lhs = self.parse_unary()?;
        let op = match self.peek() {
            Some(Item::Kw(Kw::Of)) => Pct::Of,
            Some(Item::Kw(Kw::On)) => Pct::On,
            Some(Item::Kw(Kw::Off)) => Pct::Off,
            _ => return Ok(lhs),
        };
        self.pos += 1;
        if self.is_kw(Kw::WhatIs) {
            self.pos += 1;
            let rhs = self.parse_pct()?;
            return Ok(Expr::PctInv(op, Box::new(lhs), Box::new(rhs)));
        }
        if !self.starts_operand() && !matches!(self.peek(), Some(Item::Sym('-'))) {
            return Ok(lhs); // trailing "of" in prose
        }
        let rhs = self.parse_pct()?;
        Ok(Expr::PctOp(op, Box::new(lhs), Box::new(rhs)))
    }

    fn parse_unary(&mut self) -> PResult {
        if self.is_sym('-') || self.is_kw(Kw::Minus) {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.parse_unary()?)));
        }
        if self.is_sym('+') {
            self.pos += 1;
            return self.parse_unary();
        }
        self.parse_pow()
    }

    fn parse_pow(&mut self) -> PResult {
        let base = self.parse_postfix()?;
        if self.is_sym('^') {
            self.pos += 1;
            let exp = self.parse_unary()?;
            return Ok(Expr::Bin(BinOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn parse_postfix(&mut self) -> PResult {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek().cloned() {
                Some(Item::Scale(f)) => {
                    self.pos += 1;
                    e = Expr::Bin(BinOp::Mul, Box::new(e), Box::new(Expr::Num(f)));
                }
                Some(Item::Unit(_)) => {
                    let u = self.unit_atom().unwrap();
                    e = Expr::WithUnit(Box::new(e), u);
                }
                Some(Item::CurSym(u)) => {
                    self.pos += 1;
                    e = Expr::WithUnit(Box::new(e), u);
                }
                Some(Item::Sym('%')) => {
                    self.pos += 1;
                    e = Expr::Percent(Box::new(e));
                }
                Some(Item::Sym('!')) => {
                    self.pos += 1;
                    e = Expr::Fact(Box::new(e));
                }
                Some(Item::Kw(k @ (Kw::Am | Kw::Pm))) => {
                    let pm = k == Kw::Pm;
                    let (h, m, s) = match &e {
                        Expr::Num(n) if n.fract() == 0.0 && (1.0..=12.0).contains(n) => {
                            (*n as u32, 0, 0)
                        }
                        Expr::TimeLit(h, m, s) if (1..=12).contains(h) => (*h, *m, *s),
                        _ => break,
                    };
                    self.pos += 1;
                    let h = match (pm, h) {
                        (true, h) if h < 12 => h + 12,
                        (false, 12) => 0,
                        (_, h) => h,
                    };
                    e = Expr::TimeLit(h, m, s);
                }
                Some(Item::Zone(z)) if e.is_timeish() => {
                    self.pos += 1;
                    e = Expr::InZone(Box::new(e), z);
                }
                Some(Item::Kw(Kw::Ago)) => {
                    self.pos += 1;
                    e = Expr::Ago(Box::new(e));
                }
                Some(Item::Kw(Kw::FromNow)) => {
                    self.pos += 1;
                    e = Expr::FromNow(Box::new(e));
                }
                Some(Item::Month(m)) => {
                    let Expr::Num(d) = e else { break };
                    self.pos += 1;
                    let y = self.take_year();
                    e = Expr::DateLit(y, m, d as u32);
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn take_year(&mut self) -> i32 {
        let k = if self.is_sym(',') { 1 } else { 0 };
        if let Some(Item::Num(y, 10)) = self.peek_at(k) {
            if y.fract() == 0.0 && (1000.0..10000.0).contains(y) {
                let y = *y as i32;
                self.pos += k + 1;
                return y;
            }
        }
        0
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, String> {
        // assumes current item is '('
        self.pos += 1;
        let mut args = Vec::new();
        if self.is_sym(')') {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            args.push(self.parse_convert()?);
            if self.is_sym(';') || self.is_sym(',') {
                self.pos += 1;
                continue;
            }
            if self.is_sym(')') {
                self.pos += 1;
            }
            break;
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> PResult {
        let Some(item) = self.bump().cloned() else {
            return Err("unexpected end of expression".into());
        };
        Ok(match item {
            Item::Num(v, _) => Expr::Num(v),
            Item::Const(v) => Expr::Num(v),
            Item::Time(h, m, s) => Expr::TimeLit(h, m, s),
            Item::Date(y, m, d) => Expr::DateLit(y, m, d),
            Item::Var(name) => Expr::Var(name),
            Item::Sym('(') => {
                let e = self.parse_convert()?;
                if self.is_sym(')') {
                    self.pos += 1;
                } else if self.peek().is_some() {
                    return Err("expected ')'".into());
                }
                e
            }
            Item::CurSym(u) => {
                // `$20`, `$2k`, `$30 CAD`
                let inner = match self.peek() {
                    Some(Item::Num(..) | Item::Sym('(') | Item::Var(_) | Item::Const(_)) => {
                        let mut v = self.parse_primary()?;
                        if let Some(Item::Scale(f)) = self.peek() {
                            v = Expr::Bin(BinOp::Mul, Box::new(v), Box::new(Expr::Num(*f)));
                            self.pos += 1;
                        }
                        v
                    }
                    _ => Expr::Num(1.0),
                };
                let mut unit = u;
                if let Some(Item::Unit(other)) = self.peek() {
                    let money = other
                        .as_simple()
                        .is_some_and(|id| self.reg.def(id).dims == dim(MONEY, 1));
                    if money {
                        unit = other.clone();
                        self.pos += 1;
                    }
                }
                Expr::WithUnit(Box::new(inner), unit)
            }
            Item::Unit(u) => Expr::WithUnit(Box::new(Expr::Num(1.0)), u),
            Item::Func(name) => {
                let mut args = if self.is_sym('(') {
                    self.parse_args()?
                } else {
                    vec![self.parse_unary()?]
                };
                if matches!(name.as_str(), "root" | "log") && args.len() == 1 && self.is_sym('(') {
                    args.extend(self.parse_args()?);
                }
                Expr::Call(name, args)
            }
            Item::Kw(Kw::Prev) => Expr::Prev,
            Item::Kw(Kw::Sum) => Expr::Sum,
            Item::Kw(Kw::Avg) => Expr::Avg,
            Item::Kw(Kw::Now) | Item::Kw(Kw::Time) => Expr::Now(None),
            Item::Kw(Kw::Today) => Expr::Day(0),
            Item::Kw(Kw::Tomorrow) => Expr::Day(1),
            Item::Kw(Kw::Yesterday) => Expr::Day(-1),
            Item::Zone(z) => {
                if self.is_kw(Kw::Time) || self.is_kw(Kw::Now) {
                    self.pos += 1;
                }
                Expr::Now(Some(z))
            }
            Item::Month(m) => {
                let Some(Item::Num(d, _)) = self.peek().cloned() else {
                    return Err("expected a day after the month".into());
                };
                self.pos += 1;
                let y = self.take_year();
                Expr::DateLit(y, m, d as u32)
            }
            other => return Err(format!("unexpected {other:?}")),
        })
    }
}
