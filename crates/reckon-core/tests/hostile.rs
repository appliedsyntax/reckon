//! Input written to hurt: a sheet can arrive from a shared or synced folder
//! and is evaluated as soon as the app opens it, so no line may crash the
//! engine or hang it.
//!
//! In a release build each case runs on a 512 KB stack: half of the
//! smallest main-thread stack Reckon runs on (1 MB, on iOS and Windows).
//! Measured there, the deepest line the limits allow needs about 230 KB.
//! Debug builds use roughly ten times the stack per call, so they get a
//! larger one and check only that nothing recurses without bound. Run
//! `cargo test --release -p reckon-core --test hostile` for the real check.

use reckon_core::time::days_from_civil;
use reckon_core::{Engine, Rates, Zone};
use std::time::{Duration, Instant};

fn engine() -> Engine {
    let now = days_from_civil(2026, 11, 7) as f64 * 86400.0 + 12.0 * 3600.0;
    Engine::with_fixed_clock(now, Zone::utc())
}

const STACK_KB: usize = if cfg!(debug_assertions) {
    8 * 1024
} else {
    512
};

/// Runs `f` on a small stack. An overflow would abort the whole test run.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(STACK_KB * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn last_result(sheet: String) -> Option<String> {
    on_small_stack(move || engine().evaluate(&sheet).pop().unwrap().text)
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let n = 100_000;
    for sheet in [
        "(".repeat(n),
        format!("{}1{}", "(".repeat(n), ")".repeat(n)),
        format!("{}1", "-".repeat(n)),
        format!("{}1", "- ".repeat(n)),
        format!("{}2", "2^".repeat(n)),
        format!("{}4{}", "sqrt(".repeat(n), ")".repeat(n)),
        format!("5% of what is {}6", "5% of what is ".repeat(n)),
        format!("1{}", "+1".repeat(n)),
        format!("1{}", " * 2".repeat(n)),
        ",".repeat(n),
        format!("sum{}", ", 1".repeat(n)),
        "1 ".repeat(n),
        "$".repeat(n),
        format!("{}1", "$(".repeat(n)),
        "[".repeat(n),
        format!("{}1{}", "[".repeat(n), "]".repeat(n)),
        format!("median({}", "[1, ".repeat(n)),
        // Short enough to be parsed, but nested past the limit.
        format!("{}1", "(".repeat(300)),
        format!("{}1", "-".repeat(300)),
        format!("{}2", "2^".repeat(150)),
        format!("{}4", "sqrt(".repeat(150)),
        format!("{}1", "[".repeat(300)),
        // Short enough to be parsed, but with too many terms.
        format!("1{}", "+1".repeat(300)),
    ] {
        let shown: String = sheet.chars().take(12).collect();
        let started = Instant::now();
        assert_eq!(last_result(sheet), None, "{shown}…");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{shown}… was slow"
        );
    }
}

#[test]
fn ordinary_long_and_nested_lines_still_work() {
    // 200 terms, and 30 levels of parentheses: well inside the limits.
    let sum = format!("1{}", " + 1".repeat(199));
    assert_eq!(last_result(sum).as_deref(), Some("200"));
    let nested = format!("{}2 + 3{}", "(".repeat(30), ")".repeat(30));
    assert_eq!(last_result(nested).as_deref(), Some("5"));
    assert_eq!(last_result("2^3^2".into()).as_deref(), Some("512"));
    assert_eq!(last_result("--5".into()).as_deref(), Some("5"));
}

#[test]
fn the_deepest_allowed_lines_fit_on_the_small_stack() {
    // Just inside each limit, where recursion is deepest.
    for sheet in [
        format!("1{}", "+1".repeat(199)),
        format!("{}1{}", "(".repeat(40), ")".repeat(40)),
        format!("{}1", "-".repeat(90)),
        format!("{}2", "1^".repeat(40)),
        format!("{}1{}", "[".repeat(40), "]".repeat(40)),
        format!("median([1{}])", "; 1".repeat(190)),
    ] {
        assert!(last_result(sheet.clone()).is_some(), "{sheet}");
    }
}

#[test]
fn functions_that_fan_out_stop_at_the_call_budget() {
    // Each function calls the next one twice: 2^30 calls without a budget.
    let mut src = String::new();
    for i in 1..30 {
        src.push_str(&format!("fn f{i}(x) = f{}(x) + f{}(x)\n", i + 1, i + 1));
    }
    src.push_str("fn f30(x) = x\n");
    let started = Instant::now();
    let result = on_small_stack(move || {
        let mut e = engine();
        e.load_plugin("bomb", &src).unwrap();
        e.evaluate("f1(1)").pop().unwrap()
    });
    assert_eq!(result.text, None);
    assert_eq!(result.error.as_deref(), Some("too many function calls"));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn the_call_budget_is_per_line() {
    // Two lines of about 6,000 calls each: more than the budget together,
    // within it separately.
    let mut src = String::new();
    for i in 1..12 {
        src.push_str(&format!("fn f{i}(x) = f{}(x) + f{}(x)\n", i + 1, i + 1));
    }
    src.push_str("fn f12(x) = x + f13(x)\nfn f13(x) = x\n");
    let results = on_small_stack(move || {
        let mut e = engine();
        e.load_plugin("p", &src).unwrap();
        let out = e.evaluate("f1(1)\nf1(1)");
        out.into_iter().map(|o| o.text).collect::<Vec<_>>()
    });
    assert_eq!(
        results,
        [Some("4,096".to_string()), Some("4,096".to_string())]
    );
}

#[test]
fn lists_stop_growing_at_their_limits() {
    // A list that doubles on every line stops at the most one list holds.
    let mut sheet = String::from("a = [1; 2; 3; 4; 5; 6; 7; 8]\n");
    sheet.push_str(&"a = [a; a]\n".repeat(40));
    sheet.push_str("count(a)");
    assert_eq!(last_result(sheet).as_deref(), Some("8,192"));

    // Every result is kept while the sheet is open, so thousands of lines
    // that each build another long list must stop too. Reusing a list is
    // free and still works afterwards.
    let mut sheet = String::from("a = [1; 2; 3; 4; 5; 6; 7; 8]\n");
    sheet.push_str(&"a = [a; a]\n".repeat(10));
    sheet.push_str(&"[a; 1]\n".repeat(5_000));
    let started = Instant::now();
    let out = on_small_stack(move || {
        let mut out = engine().evaluate(&format!("{sheet}a\ncount(a)\n[a; 1]"));
        let last = out.pop().unwrap();
        let count = out.pop().unwrap();
        let built = out.iter().filter(|o| o.text.is_some()).count();
        (built, count.text, last.error)
    });
    assert!(out.0 < 40, "{} lists were built", out.0);
    assert_eq!(out.1.as_deref(), Some("8,192"));
    assert_eq!(
        out.2.as_deref(),
        Some("this sheet has too many list values")
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn statistics_in_functions_stop_at_the_call_budget() {
    // Each call sorts 8,192 numbers; without a budget the 2^20 calls here
    // would take hours.
    let mut src = String::new();
    for i in 1..20 {
        src.push_str(&format!("fn g{i}(x) = g{}(x) + g{}(x)\n", i + 1, i + 1));
    }
    src.push_str("fn g20(x) = median(x)\n");
    let mut sheet = String::from("a = [1; 2; 3; 4; 5; 6; 7; 8]\n");
    sheet.push_str(&"a = [a; a]\n".repeat(10));
    sheet.push_str("g1(a)");
    let started = Instant::now();
    let result = on_small_stack(move || {
        let mut e = engine();
        e.load_plugin("stats", &src).unwrap();
        e.evaluate(&sheet).pop().unwrap()
    });
    assert_eq!(result.error.as_deref(), Some("too many function calls"));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn hostile_rate_responses_are_rejected_or_cleaned() {
    // Deep nesting must not overflow the stack.
    let deep = "[".repeat(200_000);
    assert!(on_small_stack(move || Rates::from_responses(&deep, None)).is_err());
    let deep = format!("{}1{}", "{\"a\":".repeat(200_000), "}".repeat(200_000));
    assert!(on_small_stack(move || Rates::from_responses(&deep, None)).is_err());

    // Unusable numbers and odd codes are dropped; a time in the future is
    // pulled back to now so the rates still go stale and get refreshed.
    let body = r#"{"result":"success","time_last_update_unix":99999999999,
        "rates":{"USD":1,"EUR":0.9,"BAD":0,"NEG":-2,"HUGE":1e999,"A B":3,"X\nY 1":4}}"#;
    let rates = Rates::from_responses(body, None).unwrap();
    let mut codes: Vec<_> = rates.codes().map(str::to_string).collect();
    codes.sort();
    assert_eq!(codes, ["EUR", "USD"]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(rates.updated.unwrap() <= now + 1);

    assert!(Rates::from_responses(r#"{"result":"success","rates":{"USD":0}}"#, None).is_err());
}
