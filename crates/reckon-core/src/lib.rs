//! Reckon: a natural-language calculator engine.
//!
//! ```
//! use reckon_core::Engine;
//! let engine = Engine::new();
//! let out = engine.evaluate("price = $20\nprice + 15%");
//! assert_eq!(out[1].text.as_deref(), Some("$23"));
//! ```
//!
//! The engine evaluates whole documents: every line can see variables,
//! `prev`, `sum` and `avg` from the lines above it.

pub mod currency;
pub mod eval;
pub mod json;
pub mod lexer;
pub mod parser;
pub mod paths;
pub mod plugin;
pub mod sheets;
pub mod themes;
pub mod time;
pub mod tz;
pub mod units;
pub mod value;

use std::collections::HashMap;
use std::ops::Range;

pub use currency::Rates;
pub use eval::DocState;
pub use parser::Hl;
pub use tz::Zone;
pub use value::{format_value, NumFmt, Settings, Show, Value};

use eval::Eval;
use parser::{classify, highlight_class, ClassifyCtx, Parser, BUILTIN_FUNCS};
use tz::ZoneDb;
use units::{Registry, UnitCtx};

/// A function defined by a plugin: `fn name(a; b) = body`.
#[derive(Clone, Debug)]
pub struct UserFn {
    pub params: Vec<String>,
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Empty,
    Header,
    Comment,
    Expr,
}

/// Result of evaluating one line of a document.
#[derive(Clone, Debug)]
pub struct LineOutput {
    pub kind: LineKind,
    pub value: Option<Value>,
    /// Formatted result, if the line produced one.
    pub text: Option<String>,
    /// Why the line produced no result (not shown in the app by default).
    pub error: Option<String>,
    /// Byte ranges into the line with a highlight class.
    pub spans: Vec<(Range<usize>, Hl)>,
}

pub struct Engine {
    pub(crate) reg: Registry,
    pub(crate) rates: Rates,
    pub(crate) zones: ZoneDb,
    pub settings: Settings,
    pub(crate) functions: HashMap<String, UserFn>,
    pub(crate) globals: HashMap<String, Value>,
    fixed_now: Option<f64>,
    /// Errors from loading plugins.
    pub plugin_errors: Vec<String>,
    /// Names of loaded plugins.
    pub plugins: Vec<String>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Engine with built-in units, approximate currency rates and the
    /// system time zone database.
    pub fn new() -> Engine {
        Engine::build(ZoneDb::new(), None)
    }

    /// Engine with a fixed clock and fixed-offset zones (for tests).
    pub fn with_fixed_clock(now_unix: f64, local: Zone) -> Engine {
        Engine::build(ZoneDb::fixed(local), Some(now_unix))
    }

    fn build(zones: ZoneDb, fixed_now: Option<f64>) -> Engine {
        Engine {
            reg: Registry::new(),
            rates: Rates::builtin(),
            zones,
            settings: Settings::default(),
            functions: HashMap::new(),
            globals: HashMap::new(),
            fixed_now,
            plugin_errors: Vec::new(),
            plugins: Vec::new(),
        }
    }

    /// `new()` plus cached exchange rates and plugins from the user's data
    /// folder (see [`paths::data_dir`]).
    pub fn with_user_data() -> Engine {
        let mut e = Engine::new();
        e.load_cached_rates();
        e.load_plugins();
        e
    }

    pub fn now(&self) -> f64 {
        self.fixed_now.unwrap_or_else(time::unix_now)
    }

    // ---------------------------------------------------------- rates

    pub fn rates(&self) -> &Rates {
        &self.rates
    }

    pub fn set_rates(&mut self, rates: Rates) {
        for code in rates.codes() {
            if (3..=5).contains(&code.len()) && code.chars().all(|c| c.is_ascii_uppercase()) {
                self.reg.ensure_currency(code);
            }
        }
        self.rates.merge(rates);
    }

    /// Loads rates saved by a previous refresh. Returns true on success.
    pub fn load_cached_rates(&mut self) -> bool {
        match paths::rates_path().map(|p| Rates::load(&p)) {
            Some(Ok(r)) => {
                self.set_rates(r);
                true
            }
            _ => false,
        }
    }

    /// True when rates are built-in or older than 12 hours.
    pub fn rates_stale(&self) -> bool {
        match self.rates.updated {
            None => true,
            Some(u) => self.now() - u as f64 > 12.0 * 3600.0,
        }
    }

    /// Downloads fresh rates, saves them to the cache and applies them.
    pub fn refresh_rates(&mut self) -> Result<(), String> {
        self.apply_rates(Rates::fetch()?)
    }

    /// Saves rates to the cache and applies them.
    pub fn apply_rates(&mut self, r: Rates) -> Result<(), String> {
        if let Some(p) = paths::rates_path() {
            r.save(&p)?;
        }
        self.set_rates(r);
        Ok(())
    }

    /// Overrides the local time zone (IANA name from the OS, plus the
    /// current UTC offset used if the zone database is unavailable).
    pub fn set_local_zone(&mut self, iana: &str, offset_secs: i32) {
        self.zones.set_local(iana, offset_secs);
    }

    // ---------------------------------------------------------- evaluation

    /// Evaluates every line of a document.
    pub fn evaluate(&self, text: &str) -> Vec<LineOutput> {
        let now = self.now();
        let mut st = DocState::default();
        text.split('\n')
            .map(|line| self.eval_line(line.trim_end_matches('\r'), &mut st, now))
            .collect()
    }

    /// Evaluates a single line on its own and returns the formatted result.
    pub fn eval_str(&self, line: &str) -> Result<String, String> {
        let out = self.eval_line(line, &mut DocState::default(), self.now());
        match (out.text, out.error) {
            (Some(t), _) => Ok(t),
            (None, Some(e)) => Err(e),
            (None, None) => Err("no result".into()),
        }
    }

    pub fn format(&self, v: &Value) -> String {
        let ucx = UnitCtx {
            reg: &self.reg,
            rates: &self.rates,
            ppi: 96.0,
            em_px: 16.0,
        };
        format_value(v, &ucx, &self.settings)
    }

    fn classify_tokens(&self, toks: &[lexer::Token], st: &DocState) -> Vec<parser::It> {
        let is_var =
            |w: &str| st.vars.contains_key(w) || self.globals.contains_key(w) || w == "ppi";
        let is_func = |w: &str| self.functions.contains_key(w) || BUILTIN_FUNCS.contains(&w);
        let cx = ClassifyCtx {
            reg: &self.reg,
            zones: &self.zones,
            is_var: &is_var,
            is_func: &is_func,
        };
        classify(toks, &cx)
    }

    /// Evaluates an expression with the given state (used by functions).
    pub fn eval_expression(
        &self,
        text: &str,
        st: &mut DocState,
        now: f64,
        depth: usize,
    ) -> Result<Value, String> {
        let toks = lexer::tokenize(text);
        let items = self.classify_tokens(&toks, st);
        let expr = Parser::new(&items, &self.reg).parse_line()?;
        let mut ev = Eval {
            eng: self,
            st,
            now,
            depth,
            aggregate: false,
        };
        ev.eval(&expr)
    }

    pub fn eval_line(&self, line: &str, st: &mut DocState, now: f64) -> LineOutput {
        let prep = prepare(line);
        let mut out = LineOutput {
            kind: prep.kind,
            value: None,
            text: None,
            error: None,
            spans: prep.spans,
        };
        match prep.kind {
            LineKind::Empty | LineKind::Header => {
                st.block.clear();
                return out;
            }
            LineKind::Comment => return out,
            LineKind::Expr => {}
        }
        let toks = lexer::tokenize(&prep.expr);
        let items = self.classify_tokens(&toks, st);
        out.spans.extend(
            items
                .iter()
                .map(|it| (it.span.clone(), highlight_class(&it.item))),
        );
        out.spans.sort_by_key(|s| s.0.start);
        if items.is_empty() {
            return out;
        }
        let result = Parser::new(&items, &self.reg)
            .parse_line()
            .and_then(|expr| {
                let mut ev = Eval {
                    eng: self,
                    st,
                    now,
                    depth: 0,
                    aggregate: false,
                };
                let v = ev.eval(&expr)?;
                Ok((v, ev.aggregate))
            });
        match result {
            Ok((v, aggregate)) => {
                let ucx = UnitCtx {
                    reg: &self.reg,
                    rates: &self.rates,
                    ppi: st.ppi,
                    em_px: st.em_px,
                };
                out.text = Some(format_value(&v, &ucx, &self.settings));
                st.prev = Some(v.clone());
                if !aggregate {
                    st.block.push(v.clone());
                }
                out.value = Some(v);
            }
            Err(e) => out.error = Some(e),
        }
        out
    }
}

struct Prepared {
    kind: LineKind,
    /// The line with labels, quotes and comments blanked out (same byte
    /// offsets as the original line).
    expr: String,
    spans: Vec<(Range<usize>, Hl)>,
}

/// Splits a line into header / comment / label / quoted text / expression.
fn prepare(line: &str) -> Prepared {
    let trimmed = line.trim_start();
    let lead = line.len() - trimmed.len();
    if trimmed.is_empty() {
        return Prepared {
            kind: LineKind::Empty,
            expr: String::new(),
            spans: vec![],
        };
    }
    if trimmed.starts_with('#') {
        return Prepared {
            kind: LineKind::Header,
            expr: String::new(),
            spans: vec![(lead..line.len(), Hl::Header)],
        };
    }
    if trimmed.starts_with("//") {
        return Prepared {
            kind: LineKind::Comment,
            expr: String::new(),
            spans: vec![(lead..line.len(), Hl::Comment)],
        };
    }
    let mut spans = Vec::new();
    let mut keep = vec![true; line.len()];
    let blank = |keep: &mut Vec<bool>, r: Range<usize>| keep[r].iter_mut().for_each(|k| *k = false);

    // "quoted text" is a comment
    let mut quote_start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        if matches!(c, '"' | '“' | '”') {
            match quote_start.take() {
                None => quote_start = Some(i),
                Some(s) => {
                    let e = i + c.len_utf8();
                    blank(&mut keep, s..e);
                    spans.push((s..e, Hl::Comment));
                }
            }
        }
    }
    if let Some(s) = quote_start {
        blank(&mut keep, s..line.len());
        spans.push((s..line.len(), Hl::Comment));
    }
    // trailing // comment
    if let Some(p) = find_kept(line, &keep, "//") {
        blank(&mut keep, p..line.len());
        spans.push((p..line.len(), Hl::Comment));
    }
    // Label: text before a ':' that is not part of a time like 2:30
    let bytes = line.as_bytes();
    for (i, c) in line.char_indices() {
        if c == ':' && keep[i] {
            let prev_digit = i > 0 && bytes[i - 1].is_ascii_digit();
            let next_digit = bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit());
            if prev_digit && next_digit {
                continue;
            }
            if line[..i].trim().is_empty() {
                break;
            }
            blank(&mut keep, 0..i + 1);
            spans.push((lead..i + 1, Hl::Label));
            break;
        }
    }
    let expr: String = line
        .char_indices()
        .flat_map(|(i, c)| {
            let n = c.len_utf8();
            let s: Vec<char> = if keep[i] { vec![c] } else { vec![' '; n] };
            s
        })
        .collect();
    Prepared {
        kind: LineKind::Expr,
        expr,
        spans,
    }
}

fn find_kept(line: &str, keep: &[bool], pat: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = line[from..].find(pat) {
        let at = from + p;
        if keep[at] {
            return Some(at);
        }
        from = at + pat.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_keeps_offsets() {
        let p = prepare("Price: $11 \"note\" + 2");
        assert_eq!(p.expr.len(), "Price: $11 \"note\" + 2".len());
        assert!(p.expr.trim_start().starts_with("$11"));
        assert!(!p.expr.contains("note"));
        let p = prepare("2:30 pm");
        assert_eq!(p.expr, "2:30 pm");
        let p = prepare("Café: 5 €");
        assert_eq!(p.expr.trim(), "5 €");
    }
}
