//! Plugins: plain-text `.reckon` files that add units, variables and
//! functions using the calculator's own expression language.
//!
//! ```text
//! # comments start with # or //
//! unit floz "fl oz, fluid ounce, fluid ounces" = 29.5735 ml
//! var tax = 8.25%
//! fn hypot(a; b) = sqrt(a^2 + b^2)
//! ```

use std::path::Path;

use crate::eval::DocState;
use crate::units::{Conv, UnitCtx, UnitDef};
use crate::value::Value;
use crate::{Engine, UserFn};

fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

impl Engine {
    /// Loads every `*.reckon` file in the user's plugin folder.
    pub fn load_plugins(&mut self) {
        if let Some(dir) = crate::paths::plugins_dir() {
            self.load_plugins_from(&dir);
        }
    }

    pub fn load_plugins_from(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "reckon"))
            .collect();
        files.sort();
        for path in files {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match std::fs::read_to_string(&path) {
                Ok(src) => {
                    if let Err(e) = self.load_plugin(&name, &src) {
                        self.plugin_errors.push(e);
                    }
                }
                Err(e) => self.plugin_errors.push(format!("{name}: {e}")),
            }
        }
    }

    /// Loads plugin source text. Lines before an error stay loaded.
    pub fn load_plugin(&mut self, name: &str, src: &str) -> Result<usize, String> {
        let mut count = 0;
        for (n, raw) in src.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let res = if let Some(rest) = line.strip_prefix("unit ") {
                self.plugin_unit(rest)
            } else if let Some(rest) = line
                .strip_prefix("var ")
                .or_else(|| line.strip_prefix("let "))
            {
                self.plugin_var(rest)
            } else if let Some(rest) = line.strip_prefix("fn ") {
                self.plugin_fn(rest)
            } else {
                Err("expected `unit`, `var` or `fn`".into())
            };
            res.map_err(|e| format!("{name}:{}: {e}", n + 1))?;
            count += 1;
        }
        self.plugins.push(name.to_string());
        Ok(count)
    }

    fn eval_standalone(&self, expr: &str) -> Result<Value, String> {
        let mut st = DocState::default();
        self.eval_expression(expr, &mut st, self.now(), 0)
    }

    /// `id "phrase, phrase" = expr`
    fn plugin_unit(&mut self, rest: &str) -> Result<(), String> {
        let rest = rest.trim();
        let (id, rest) = rest
            .split_once(char::is_whitespace)
            .ok_or("expected: unit id \"phrases\" = value")?;
        if self.reg.by_key(id).is_some() {
            return Err(format!("unit id `{id}` already exists"));
        }
        let rest = rest
            .trim_start()
            .strip_prefix('"')
            .ok_or("phrases must be in double quotes")?;
        let (phrases, rest) = rest.split_once('"').ok_or("missing closing quote")?;
        let expr = rest
            .trim_start()
            .strip_prefix('=')
            .ok_or("expected `=` and a value")?;
        let phrases: Vec<&str> = phrases
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if phrases.is_empty() {
            return Err("at least one phrase is required".into());
        }
        let Value::Num(q) = self.eval_standalone(expr)? else {
            return Err("unit value must be a number with a unit".into());
        };
        let ucx = UnitCtx {
            reg: &self.reg,
            rates: &self.rates,
            ppi: 96.0,
            em_px: 16.0,
        };
        if q.unit.parts.iter().any(|(u, _)| {
            matches!(
                ucx.reg.def(*u).conv,
                Conv::Affine { .. } | Conv::Px | Conv::Em
            )
        }) {
            return Err("units must be defined by a fixed ratio".into());
        }
        let dims = ucx.dims(&q.unit);
        let factor = q.v * ucx.factor(&q.unit)?;
        let def = UnitDef {
            key: id.to_string(),
            symbol: phrases[0].to_string(),
            plural: None,
            dims,
            conv: Conv::Linear(factor),
            tight: false,
            prefix: false,
        };
        self.reg.add(def, &phrases, &phrases);
        Ok(())
    }

    /// `name = expr`
    fn plugin_var(&mut self, rest: &str) -> Result<(), String> {
        let (name, expr) = rest.split_once('=').ok_or("expected: var name = value")?;
        let name = name.trim();
        if !valid_name(name) {
            return Err(format!("invalid variable name `{name}`"));
        }
        let v = self.eval_standalone(expr)?;
        self.globals.insert(name.to_string(), v);
        Ok(())
    }

    /// `name(a; b) = body`
    fn plugin_fn(&mut self, rest: &str) -> Result<(), String> {
        let (head, body) = rest
            .split_once('=')
            .ok_or("expected: fn name(a; b) = expression")?;
        let (name, params) = head
            .trim()
            .split_once('(')
            .ok_or("expected parameters in parentheses")?;
        let params = params.trim().strip_suffix(')').ok_or("missing `)`")?;
        let name = name.trim().to_lowercase();
        if !valid_name(&name) {
            return Err(format!("invalid function name `{name}`"));
        }
        let params: Vec<String> = params
            .split([';', ','])
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if let Some(bad) = params.iter().find(|p| !valid_name(p)) {
            return Err(format!("invalid parameter name `{bad}`"));
        }
        if body.trim().is_empty() {
            return Err("function body is empty".into());
        }
        self.functions.insert(
            name,
            UserFn {
                params,
                body: body.trim().to_string(),
            },
        );
        Ok(())
    }
}
