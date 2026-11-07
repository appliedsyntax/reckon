//! `reckon` — command-line front end for the Reckon calculator engine.

use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;

use reckon_core::{paths, Engine, LineKind};

const HELP: &str = "\
reckon — natural-language calculator

USAGE:
    reckon [OPTIONS] [EXPRESSION...]
    reckon -f FILE
    echo '20 inches in cm' | reckon

With no expression and an interactive terminal, starts a REPL where
variables, prev, sum and avg carry over between lines.

OPTIONS:
    -f, --file FILE      Evaluate every line of FILE
    -a, --annotate       Print each input line next to its result
    -p, --precision N    Maximum decimal places (default 4)
        --24h            Use a 24-hour clock
        --update-rates   Download current exchange rates and exit
        --rates          Show where exchange rates come from and exit
        --plugins        List loaded plugins and errors and exit
        --no-user-data   Ignore cached rates and plugins
    -h, --help           Show this help
    -V, --version        Show the version

EXAMPLES:
    reckon '$30 in EUR'
    reckon 'today + 2 weeks'
    reckon '2:30 pm HKT in Berlin'
    reckon -a -f budget.reckon
";

#[derive(Default)]
struct Opts {
    exprs: Vec<String>,
    file: Option<String>,
    annotate: bool,
    precision: Option<usize>,
    hour24: bool,
    update_rates: bool,
    show_rates: bool,
    show_plugins: bool,
    no_user_data: bool,
}

fn parse_args() -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut args = std::env::args().skip(1);
    let mut rest_are_exprs = false;
    while let Some(a) = args.next() {
        if rest_are_exprs {
            o.exprs.push(a);
            continue;
        }
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("reckon {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-f" | "--file" => o.file = Some(args.next().ok_or("--file needs a path")?),
            "-a" | "--annotate" => o.annotate = true,
            "-p" | "--precision" => {
                let v = args.next().ok_or("--precision needs a number")?;
                o.precision = Some(v.parse().map_err(|_| format!("invalid precision: {v}"))?);
            }
            "--24h" => o.hour24 = true,
            "--update-rates" => o.update_rates = true,
            "--rates" => o.show_rates = true,
            "--plugins" => o.show_plugins = true,
            "--no-user-data" => o.no_user_data = true,
            "--" => rest_are_exprs = true,
            // A leading '-' followed by a digit or '$' is a negative number, not a flag.
            s if s.starts_with('-')
                && s.len() > 1
                && !s[1..].starts_with(|c: char| c.is_ascii_digit() || c == '.') =>
            {
                return Err(format!("unknown option {s} (see --help)"));
            }
            _ => o.exprs.push(a),
        }
    }
    Ok(o)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("reckon: {e}");
            return ExitCode::from(2);
        }
    };
    let mut engine = if opts.no_user_data {
        Engine::new()
    } else {
        Engine::with_user_data()
    };
    if let Some(p) = opts.precision {
        engine.settings.precision = p.min(15);
    }
    engine.settings.hour24 = opts.hour24;

    if opts.update_rates {
        return match engine.refresh_rates() {
            Ok(()) => {
                let r = engine.rates();
                println!("Updated {} exchange rates from {}", r.len(), r.source);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("reckon: could not update rates: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if opts.show_rates {
        let r = engine.rates();
        println!("source:  {}", r.source);
        match r.updated {
            Some(u) => println!(
                "updated: {}",
                engine
                    .eval_str(&format!("fromunix({u})"))
                    .unwrap_or_default()
            ),
            None => println!("updated: never (run `reckon --update-rates`)"),
        }
        println!("count:   {}", r.len());
        if let Some(p) = paths::rates_path() {
            println!("cache:   {}", p.display());
        }
        return ExitCode::SUCCESS;
    }
    if opts.show_plugins {
        match paths::plugins_dir() {
            Some(d) => println!("folder:  {}", d.display()),
            None => println!("folder:  (no home directory)"),
        }
        println!(
            "loaded:  {}",
            if engine.plugins.is_empty() {
                "none".into()
            } else {
                engine.plugins.join(", ")
            }
        );
        for e in &engine.plugin_errors {
            println!("error:   {e}");
        }
        return ExitCode::SUCCESS;
    }
    for e in &engine.plugin_errors {
        eprintln!("reckon: plugin error: {e}");
    }

    if !opts.exprs.is_empty() {
        return run_single(&engine, &opts.exprs.join(" "));
    }
    let text = if let Some(path) = &opts.file {
        match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("reckon: {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else if !io::stdin().is_terminal() {
        let mut t = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut t) {
            eprintln!("reckon: {e}");
            return ExitCode::FAILURE;
        }
        t
    } else {
        return repl(&engine);
    };
    run_document(&engine, &text, opts.annotate);
    ExitCode::SUCCESS
}

fn run_single(engine: &Engine, expr: &str) -> ExitCode {
    match engine.eval_str(expr) {
        Ok(t) => {
            println!("{t}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("reckon: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_document(engine: &Engine, text: &str, annotate: bool) {
    let text = text.strip_suffix('\n').unwrap_or(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let out = engine.evaluate(text);
    let width = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .min(60);
    let stdout = io::stdout();
    let mut w = stdout.lock();
    for (line, o) in lines.iter().zip(&out) {
        let result = o.text.as_deref().unwrap_or("");
        let _ = if annotate {
            match o.kind {
                LineKind::Expr if !result.is_empty() => writeln!(w, "{line:<width$}  = {result}"),
                _ => writeln!(w, "{line}"),
            }
        } else {
            writeln!(w, "{result}")
        };
    }
}

fn repl(engine: &Engine) -> ExitCode {
    println!(
        "reckon {} — type an expression, :clear to reset, :q to quit",
        env!("CARGO_PKG_VERSION")
    );
    let mut doc: Vec<String> = Vec::new();
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("> ");
        let _ = io::stdout().flush();
        let Some(Ok(line)) = lines.next() else { break };
        match line.trim() {
            ":q" | ":quit" | "exit" | "quit" => break,
            ":clear" => {
                doc.clear();
                continue;
            }
            _ => {}
        }
        doc.push(line);
        let out = engine.evaluate(&doc.join("\n"));
        if let Some(last) = out.last() {
            match (&last.text, &last.error) {
                (Some(t), _) => println!("  {t}"),
                (None, Some(e)) => println!("  ? {e}"),
                _ => {}
            }
        }
    }
    ExitCode::SUCCESS
}
