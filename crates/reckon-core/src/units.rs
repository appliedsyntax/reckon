//! Unit registry and compound units.
//!
//! Every unit has a dimension vector (length, mass, time, ...) and a way to
//! convert to the base unit of that dimension. Compound units such as
//! `km/h` or `m²` are products of registered units raised to integer powers.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use std::collections::HashMap;

use crate::currency::{Rates, CURRENCIES};

pub const DIM_COUNT: usize = 7;
pub const LEN: usize = 0;
pub const MASS: usize = 1;
pub const TIME: usize = 2;
pub const TEMP: usize = 3;
pub const DATA: usize = 4;
pub const ANGLE: usize = 5;
pub const MONEY: usize = 6;

pub type Dims = [i8; DIM_COUNT];

pub const fn dim(i: usize, p: i8) -> Dims {
    let mut d = [0; DIM_COUNT];
    d[i] = p;
    d
}

#[derive(Clone, Debug)]
pub enum Conv {
    /// value_in_base = value * factor
    Linear(f64),
    /// value_in_base = value * factor + offset (temperatures)
    Affine { factor: f64, offset: f64 },
    /// Converted through exchange rates (base: USD).
    Currency(String),
    /// CSS pixel, depends on `ppi`.
    Px,
    /// CSS em, depends on `em` and `ppi`.
    Em,
}

#[derive(Clone, Debug)]
pub struct UnitDef {
    /// Stable identifier (`m`, `km`, `USD`, plugin ids, ...).
    pub key: String,
    /// Display symbol.
    pub symbol: String,
    /// Display form used when the value is not 1 (only for word symbols).
    pub plural: Option<String>,
    pub dims: Dims,
    pub conv: Conv,
    /// Render without a space between number and symbol (`45°`).
    pub tight: bool,
    /// Render symbol before the number (`$20`).
    pub prefix: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitId(pub u32);

/// A product of units raised to integer powers. Empty means dimensionless.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unit {
    pub parts: Vec<(UnitId, i8)>,
}

impl Unit {
    pub fn none() -> Self {
        Unit { parts: Vec::new() }
    }
    pub fn simple(id: UnitId) -> Self {
        Unit {
            parts: vec![(id, 1)],
        }
    }
    pub fn pow_of(id: UnitId, p: i8) -> Self {
        Unit {
            parts: vec![(id, p)],
        }
    }
    pub fn is_none(&self) -> bool {
        self.parts.is_empty()
    }
    pub fn as_simple(&self) -> Option<UnitId> {
        match self.parts.as_slice() {
            [(id, 1)] => Some(*id),
            _ => None,
        }
    }
    fn normalized(mut parts: Vec<(UnitId, i8)>) -> Self {
        parts.sort_by_key(|p| p.0);
        let mut out: Vec<(UnitId, i8)> = Vec::new();
        for (id, p) in parts {
            match out.last_mut() {
                Some(last) if last.0 == id => last.1 += p,
                _ => out.push((id, p)),
            }
        }
        out.retain(|p| p.1 != 0);
        Unit { parts: out }
    }
    pub fn mul(&self, other: &Unit) -> Unit {
        Unit::normalized(
            self.parts
                .iter()
                .chain(other.parts.iter())
                .copied()
                .collect(),
        )
    }
    pub fn inv(&self) -> Unit {
        Unit {
            parts: self.parts.iter().map(|(id, p)| (*id, -p)).collect(),
        }
    }
    pub fn powi(&self, n: i8) -> Unit {
        Unit::normalized(self.parts.iter().map(|(id, p)| (*id, p * n)).collect())
    }
}

const SI: &[(&str, &str, f64)] = &[
    ("Y", "yotta", 1e24),
    ("Z", "zetta", 1e21),
    ("E", "exa", 1e18),
    ("P", "peta", 1e15),
    ("T", "tera", 1e12),
    ("G", "giga", 1e9),
    ("M", "mega", 1e6),
    ("k", "kilo", 1e3),
    ("h", "hecto", 1e2),
    ("da", "deca", 1e1),
    ("d", "deci", 1e-1),
    ("c", "centi", 1e-2),
    ("m", "milli", 1e-3),
    ("µ", "micro", 1e-6),
    ("n", "nano", 1e-9),
    ("p", "pico", 1e-12),
    ("f", "femto", 1e-15),
];
const BIG: &[&str] = &["Y", "Z", "E", "P", "T", "G", "M", "k"];
const SMALL: &[&str] = &["m", "µ", "n", "p", "f"];
const BINARY: &[(&str, &str, i32)] = &[
    ("Ki", "kibi", 1),
    ("Mi", "mebi", 2),
    ("Gi", "gibi", 3),
    ("Ti", "tebi", 4),
    ("Pi", "pebi", 5),
    ("Ei", "exbi", 6),
];

pub struct Registry {
    defs: Vec<UnitDef>,
    /// Case-sensitive phrases (symbols like `m`, `MB`, `K`).
    exact: HashMap<String, UnitId>,
    /// Lower-cased phrases (names like `kilometers`).
    folded: HashMap<String, UnitId>,
    by_key: HashMap<String, UnitId>,
    /// Longest phrase in words, used by the multi-word matcher.
    pub max_words: usize,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        let mut r = Registry {
            defs: Vec::new(),
            exact: HashMap::new(),
            folded: HashMap::new(),
            by_key: HashMap::new(),
            max_words: 1,
        };
        r.builtin();
        for c in CURRENCIES {
            r.ensure_currency(c.code);
        }
        r
    }

    pub fn def(&self, id: UnitId) -> &UnitDef {
        &self.defs[id.0 as usize]
    }

    pub fn by_key(&self, key: &str) -> Option<UnitId> {
        self.by_key.get(key).copied()
    }

    /// Registers a unit. `exact` phrases are case-sensitive; `names` are
    /// matched case-insensitively. Existing phrases are not overwritten.
    pub fn add(&mut self, def: UnitDef, exact: &[&str], names: &[&str]) -> UnitId {
        let id = if let Some(id) = self.by_key.get(&def.key) {
            *id
        } else {
            let id = UnitId(self.defs.len() as u32);
            self.by_key.insert(def.key.clone(), id);
            self.defs.push(def);
            id
        };
        for p in exact {
            self.max_words = self.max_words.max(p.split(' ').count());
            self.exact.entry((*p).to_string()).or_insert(id);
        }
        for p in names {
            self.max_words = self.max_words.max(p.split(' ').count());
            self.folded.entry(p.to_lowercase()).or_insert(id);
        }
        id
    }

    pub fn ensure_currency(&mut self, code: &str) -> UnitId {
        if let Some(id) = self.by_key.get(code) {
            return *id;
        }
        let info = CURRENCIES.iter().find(|c| c.code == code);
        let (symbol, prefix) = match info.and_then(|i| i.prefix_symbol) {
            Some(s) => (s.to_string(), true),
            None => (code.to_string(), false),
        };
        let def = UnitDef {
            key: code.to_string(),
            symbol,
            plural: None,
            dims: dim(MONEY, 1),
            conv: Conv::Currency(code.to_string()),
            tight: prefix,
            prefix,
        };
        let names: Vec<&str> = info.map(|i| i.names.to_vec()).unwrap_or_default();
        self.add(def, &[code], &names)
    }

    fn lin(
        &mut self,
        key: &str,
        sym: &str,
        plural: Option<&str>,
        dims: Dims,
        f: f64,
        exact: &[&str],
        names: &[&str],
    ) -> UnitId {
        self.add(
            UnitDef {
                key: key.into(),
                symbol: sym.into(),
                plural: plural.map(Into::into),
                dims,
                conv: Conv::Linear(f),
                tight: false,
                prefix: false,
            },
            exact,
            names,
        )
    }

    /// Adds a base unit plus all of its SI-prefixed forms.
    fn prefixed(
        &mut self,
        sym: &str,
        alt_syms: &[&str],
        long: &[&str],
        dims: Dims,
        base: f64,
        prefixes: &[&str],
        skip: &[&str],
    ) {
        let mut exact = vec![sym];
        exact.extend_from_slice(alt_syms);
        self.lin(sym, sym, None, dims, base, &exact, long);
        for (ps, pl, f) in SI {
            if !prefixes.contains(ps) {
                continue;
            }
            let key = format!("{ps}{sym}");
            let mut ex: Vec<String> = vec![key.clone()];
            for a in alt_syms {
                ex.push(format!("{ps}{a}"));
            }
            if *ps == "µ" {
                ex.push(format!("u{sym}"));
                ex.push(format!("μ{sym}"));
            }
            if *ps == "k" && dims == dim(DATA, 1) {
                ex.push(format!("K{sym}"));
            }
            let ex: Vec<String> = ex
                .into_iter()
                .filter(|e| !skip.contains(&e.as_str()))
                .collect();
            let names: Vec<String> = long.iter().map(|l| format!("{pl}{l}")).collect();
            let exr: Vec<&str> = ex.iter().map(String::as_str).collect();
            let nr: Vec<&str> = names.iter().map(String::as_str).collect();
            self.lin(&key, &key, None, dims, base * f, &exr, &nr);
        }
    }

    fn builtin(&mut self) {
        let l = dim(LEN, 1);
        let a = dim(LEN, 2);
        let v = dim(LEN, 3);
        let m = dim(MASS, 1);
        let t = dim(TIME, 1);
        let d = dim(DATA, 1);
        let all: Vec<&str> = SI.iter().map(|p| p.0).collect();

        // Length
        self.prefixed(
            "m",
            &[],
            &["meter", "meters", "metre", "metres"],
            l,
            1.0,
            &all,
            &["pm", "dam"],
        );
        self.lin("mil", "mil", None, l, 2.54e-5, &["mil"], &["mils", "thou"]);
        self.lin(
            "pt",
            "pt",
            None,
            l,
            0.0254 / 72.0,
            &["pt"],
            &["point", "points"],
        );
        self.lin(
            "line",
            "line",
            Some("lines"),
            l,
            0.0254 / 12.0,
            &[],
            &["line", "lines"],
        );
        self.lin("inch", "in", None, l, 0.0254, &["″"], &["inch", "inches"]);
        self.lin(
            "hand",
            "hand",
            Some("hands"),
            l,
            0.1016,
            &[],
            &["hand", "hands"],
        );
        self.lin("ft", "ft", None, l, 0.3048, &["ft", "′"], &["foot", "feet"]);
        self.lin("yd", "yd", None, l, 0.9144, &["yd"], &["yard", "yards"]);
        self.lin("rod", "rod", Some("rods"), l, 5.0292, &[], &["rod", "rods"]);
        self.lin(
            "chain",
            "chain",
            Some("chains"),
            l,
            20.1168,
            &[],
            &["chain", "chains"],
        );
        self.lin(
            "furlong",
            "furlong",
            Some("furlongs"),
            l,
            201.168,
            &[],
            &["furlong", "furlongs"],
        );
        self.lin("mi", "mi", None, l, 1609.344, &["mi"], &["mile", "miles"]);
        self.lin(
            "cable",
            "cable",
            Some("cables"),
            l,
            185.2,
            &[],
            &["cable", "cables"],
        );
        self.lin(
            "nmi",
            "nmi",
            None,
            l,
            1852.0,
            &["nmi", "NM"],
            &["nautical mile", "nautical miles"],
        );
        self.lin(
            "league",
            "league",
            Some("leagues"),
            l,
            4828.032,
            &[],
            &["league", "leagues"],
        );
        self.lin(
            "ly",
            "ly",
            None,
            l,
            9.4607304725808e15,
            &["ly"],
            &["light year", "light years", "lightyear", "lightyears"],
        );
        let defs = [
            ("px", Conv::Px, "px", &["pixel", "pixels"][..]),
            ("em", Conv::Em, "em", &["ems"][..]),
        ];
        for (key, conv, sym, names) in defs {
            self.add(
                UnitDef {
                    key: key.into(),
                    symbol: sym.into(),
                    plural: None,
                    dims: l,
                    conv,
                    tight: false,
                    prefix: false,
                },
                &[sym],
                names,
            );
        }

        // Area
        self.lin("ha", "ha", None, a, 1e4, &["ha"], &["hectare", "hectares"]);
        self.lin("are", "are", Some("ares"), a, 100.0, &[], &["are", "ares"]);
        self.lin(
            "acre",
            "acre",
            Some("acres"),
            a,
            4046.8564224,
            &["ac"],
            &["acre", "acres"],
        );

        // Volume
        self.prefixed(
            "L",
            &["l"],
            &["liter", "liters", "litre", "litres"],
            v,
            1e-3,
            &["k", "h", "d", "c", "m", "µ"],
            &[],
        );
        self.lin(
            "pint",
            "pint",
            Some("pints"),
            v,
            4.73176473e-4,
            &[],
            &["pint", "pints"],
        );
        self.lin(
            "qt",
            "qt",
            None,
            v,
            9.46352946e-4,
            &["qt"],
            &["quart", "quarts"],
        );
        self.lin(
            "gal",
            "gal",
            None,
            v,
            3.785411784e-3,
            &["gal"],
            &["gallon", "gallons"],
        );
        self.lin(
            "tsp",
            "tsp",
            None,
            v,
            4.92892159375e-6,
            &["tsp"],
            &["teaspoon", "teaspoons", "tea spoon", "tea spoons"],
        );
        self.lin(
            "tbsp",
            "tbsp",
            None,
            v,
            1.478676478125e-5,
            &["tbsp", "tbs"],
            &["tablespoon", "tablespoons", "table spoon", "table spoons"],
        );
        self.lin(
            "cup",
            "cup",
            Some("cups"),
            v,
            2.365882365e-4,
            &[],
            &["cup", "cups"],
        );

        // Mass (base: gram)
        self.prefixed(
            "g",
            &[],
            &["gram", "grams", "gramme", "grammes"],
            m,
            1.0,
            &all,
            &[],
        );
        self.lin(
            "t",
            "t",
            None,
            m,
            1e6,
            &["t"],
            &[
                "tonne",
                "tonnes",
                "ton",
                "tons",
                "metric ton",
                "metric tons",
            ],
        );
        self.lin("carat", "ct", None, m, 0.2, &["ct"], &["carat", "carats"]);
        self.lin(
            "centner",
            "centner",
            Some("centners"),
            m,
            1e5,
            &[],
            &["centner", "centners"],
        );
        self.lin(
            "lb",
            "lb",
            None,
            m,
            453.59237,
            &["lb", "lbs"],
            &["pound", "pounds"],
        );
        self.lin(
            "stone",
            "st",
            None,
            m,
            6350.29318,
            &["st"],
            &["stone", "stones"],
        );
        self.lin(
            "oz",
            "oz",
            None,
            m,
            28.349523125,
            &["oz"],
            &["ounce", "ounces"],
        );

        // Time (base: second)
        self.prefixed(
            "s",
            &["sec", "secs"],
            &["second", "seconds"],
            t,
            1.0,
            SMALL,
            &[],
        );
        self.lin(
            "min",
            "min",
            None,
            t,
            60.0,
            &["min", "mins"],
            &["minute", "minutes"],
        );
        self.lin(
            "h",
            "h",
            None,
            t,
            3600.0,
            &["h", "hr", "hrs"],
            &["hour", "hours"],
        );
        self.lin(
            "day",
            "day",
            Some("days"),
            t,
            86400.0,
            &[],
            &["day", "days"],
        );
        self.lin(
            "week",
            "week",
            Some("weeks"),
            t,
            604800.0,
            &["wk", "wks"],
            &["week", "weeks"],
        );
        self.lin(
            "month",
            "month",
            Some("months"),
            t,
            365.0 * 86400.0 / 12.0,
            &[],
            &["month", "months"],
        );
        self.lin(
            "year",
            "year",
            Some("years"),
            t,
            365.0 * 86400.0,
            &["yr", "yrs"],
            &["year", "years"],
        );

        // Temperature (base: kelvin)
        let temps: [(&str, &str, f64, f64, &[&str], &[&str]); 3] = [
            ("K", "K", 1.0, 0.0, &["K"], &["kelvin", "kelvins"]),
            (
                "°C",
                "°C",
                1.0,
                273.15,
                &["°C", "C", "degC"],
                &["celsius", "centigrade"],
            ),
            (
                "°F",
                "°F",
                5.0 / 9.0,
                459.67 * 5.0 / 9.0,
                &["°F", "F", "degF"],
                &["fahrenheit"],
            ),
        ];
        for (key, sym, factor, offset, ex, names) in temps {
            self.add(
                UnitDef {
                    key: key.into(),
                    symbol: sym.into(),
                    plural: None,
                    dims: dim(TEMP, 1),
                    conv: Conv::Affine { factor, offset },
                    tight: false,
                    prefix: false,
                },
                ex,
                names,
            );
        }

        // Data (base: bit)
        self.prefixed("b", &[], &["bit", "bits"], d, 1.0, BIG, &[]);
        self.prefixed("B", &[], &["byte", "bytes"], d, 8.0, BIG, &[]);
        for (base_sym, base_f, longs) in [
            ("b", 1.0, &["bit", "bits"][..]),
            ("B", 8.0, &["byte", "bytes"][..]),
        ] {
            for (ps, pl, n) in BINARY {
                let key = format!("{ps}{base_sym}");
                let names: Vec<String> = longs.iter().map(|x| format!("{pl}{x}")).collect();
                let nr: Vec<&str> = names.iter().map(String::as_str).collect();
                self.lin(&key, &key, None, d, base_f * 1024f64.powi(*n), &[&key], &nr);
            }
        }

        // Angle (base: radian)
        self.lin(
            "rad",
            "rad",
            None,
            dim(ANGLE, 1),
            1.0,
            &["rad"],
            &["radian", "radians"],
        );
        let deg = UnitDef {
            key: "deg".into(),
            symbol: "°".into(),
            plural: None,
            dims: dim(ANGLE, 1),
            conv: Conv::Linear(std::f64::consts::PI / 180.0),
            tight: true,
            prefix: false,
        };
        self.add(deg, &["°", "deg"], &["degree", "degrees"]);

        // Speed
        let speed = [1, 0, -1, 0, 0, 0, 0];
        self.lin("mph", "mph", None, speed, 0.44704, &["mph"], &[]);
        self.lin(
            "kph",
            "km/h",
            None,
            speed,
            1000.0 / 3600.0,
            &["kph", "kmh"],
            &[],
        );
        self.lin(
            "knot",
            "kn",
            None,
            speed,
            1852.0 / 3600.0,
            &["kn", "kt"],
            &["knot", "knots"],
        );
    }

    /// Resolves a phrase (one or more words joined by single spaces).
    pub fn lookup(&self, phrase: &str) -> Option<Unit> {
        if let Some(id) = self.exact.get(phrase) {
            return Some(Unit::simple(*id));
        }
        if let Some(id) = self.folded.get(&phrase.to_lowercase()) {
            return Some(Unit::simple(*id));
        }
        // Glued power prefixes: sqm, sqft, cbm, cucm
        let lower = phrase.to_lowercase();
        for (pre, p) in [("sq", 2i8), ("cb", 3), ("cu", 3)] {
            if let Some(rest) = phrase
                .strip_prefix(pre)
                .or_else(|| lower.strip_prefix(pre).filter(|r| r.len() > 2))
            {
                if let Some(u) = self.length_unit(rest) {
                    return Some(Unit::pow_of(u, p));
                }
            }
        }
        // Power suffixes: m2, cm3, m², ft³
        for (suf, p) in [("2", 2i8), ("3", 3), ("²", 2), ("³", 3)] {
            if let Some(rest) = phrase.strip_suffix(suf) {
                if let Some(u) = self.length_unit(rest) {
                    return Some(Unit::pow_of(u, p));
                }
            }
        }
        None
    }

    /// Looks up a unit that measures length (used by `sq`/`cubic`).
    pub fn length_unit(&self, phrase: &str) -> Option<UnitId> {
        let u = self
            .exact
            .get(phrase)
            .or_else(|| self.folded.get(&phrase.to_lowercase()))?;
        (self.def(*u).dims == dim(LEN, 1)).then_some(*u)
    }
}

/// Everything needed to turn a unit into a number of base units.
pub struct UnitCtx<'a> {
    pub reg: &'a Registry,
    pub rates: &'a Rates,
    pub ppi: f64,
    pub em_px: f64,
}

impl UnitCtx<'_> {
    pub fn factor_of(&self, id: UnitId) -> Result<f64, String> {
        match &self.reg.def(id).conv {
            Conv::Linear(f) => Ok(*f),
            Conv::Affine { factor, .. } => Ok(*factor),
            Conv::Currency(code) => self
                .rates
                .usd_value(code)
                .ok_or_else(|| format!("no exchange rate for {code}")),
            Conv::Px => Ok(0.0254 / self.ppi),
            Conv::Em => Ok(self.em_px * 0.0254 / self.ppi),
        }
    }

    pub fn factor(&self, u: &Unit) -> Result<f64, String> {
        let mut f = 1.0;
        for (id, p) in &u.parts {
            f *= self.factor_of(*id)?.powi(*p as i32);
        }
        Ok(f)
    }

    pub fn dims(&self, u: &Unit) -> Dims {
        let mut d = [0i8; DIM_COUNT];
        for (id, p) in &u.parts {
            let ud = self.reg.def(*id).dims;
            for i in 0..DIM_COUNT {
                d[i] += ud[i] * p;
            }
        }
        d
    }

    pub fn compatible(&self, a: &Unit, b: &Unit) -> bool {
        self.dims(a) == self.dims(b)
    }

    pub fn convert(&self, v: f64, from: &Unit, to: &Unit) -> Result<f64, String> {
        if from == to {
            return Ok(v);
        }
        if !self.compatible(from, to) {
            return Err(format!(
                "cannot convert {} to {}",
                self.label(from, false),
                self.label(to, false)
            ));
        }
        if let (Some(a), Some(b)) = (from.as_simple(), to.as_simple()) {
            if let (
                Conv::Affine {
                    factor: fa,
                    offset: oa,
                },
                Conv::Affine {
                    factor: fb,
                    offset: ob,
                },
            ) = (&self.reg.def(a).conv, &self.reg.def(b).conv)
            {
                return Ok((v * fa + oa - ob) / fb);
            }
        }
        Ok(v * self.factor(from)? / self.factor(to)?)
    }

    /// Human readable unit, e.g. `km/h`, `m²`, `days`.
    pub fn label(&self, u: &Unit, plural: bool) -> String {
        if let Some(id) = u.as_simple() {
            let d = self.reg.def(id);
            return match (&d.plural, plural) {
                (Some(p), true) => p.clone(),
                _ => d.symbol.clone(),
            };
        }
        let fmt = |parts: Vec<(UnitId, i8)>| -> String {
            parts
                .iter()
                .map(|(id, p)| {
                    let d = self.reg.def(*id);
                    // `$/h` reads badly; use the code inside compound units.
                    let s = if d.prefix { &d.key } else { &d.symbol };
                    if *p == 1 {
                        s.clone()
                    } else {
                        format!("{s}{}", superscript(*p))
                    }
                })
                .collect::<Vec<_>>()
                .join("·")
        };
        let num: Vec<_> = u.parts.iter().filter(|p| p.1 > 0).copied().collect();
        let den: Vec<_> = u
            .parts
            .iter()
            .filter(|p| p.1 < 0)
            .map(|(id, p)| (*id, -p))
            .collect();
        match (num.is_empty(), den.is_empty()) {
            (_, true) => fmt(num),
            (true, false) => format!("1/{}", fmt(den)),
            (false, false) => format!("{}/{}", fmt(num), fmt(den)),
        }
    }
}

pub fn superscript(n: i8) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}
