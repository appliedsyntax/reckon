//! End-to-end examples, mostly taken from the feature documentation.
//! The clock is fixed at Saturday 2026-11-07 12:00 UTC, the local zone is
//! UTC, and zones use fixed offsets so results are deterministic.

use reckon_core::time::days_from_civil;
use reckon_core::{Engine, LineKind, Zone};

fn engine() -> Engine {
    let now = days_from_civil(2026, 11, 7) as f64 * 86400.0 + 12.0 * 3600.0;
    Engine::with_fixed_clock(now, Zone::utc())
}

fn eval(src: &str) -> String {
    let e = engine();
    let out = e.evaluate(src);
    let last = out.last().unwrap();
    last.text
        .clone()
        .unwrap_or_else(|| format!("<no result: {:?}>", last.error))
}

fn check(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (input, want) in cases {
        let got = eval(input);
        if got != *want {
            failures.push(format!("{input:?}\n    want {want:?}\n     got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn operations() {
    check(&[
        ("8 times 9", "72"),
        ("1 meter 20 cm", "120 cm"),
        ("1 meter 20 cm = 120 cm", "120 cm"),
        ("6 (3)", "18"),
        ("2 + 3 * 4", "14"),
        ("(2 + 3) * 4", "20"),
        ("2^10", "1,024"),
        ("-2^2", "-4"),
        ("10 divided by 4", "2.5"),
        ("7 multiplied by 6", "42"),
        ("10 minus 3 plus 1", "8"),
        ("10 mod 3", "1"),
        ("6 & 3", "2"),
        ("6 | 3", "7"),
        ("6 xor 3", "5"),
        ("1 << 4", "16"),
        ("256 >> 2", "64"),
        ("5!", "120"),
        ("1/3", "0.3333"),
        ("pi", "3.1416"),
        ("2 pi", "6.2832"),
    ]);
}

#[test]
fn numbers_and_formats() {
    check(&[
        ("0b110111011", "443"),
        ("0o1435343 in hex", "0x63AE3"),
        ("0xFF", "255"),
        ("255 in bin", "0b11111111"),
        ("5 300 in sci", "5.3e3"),
        ("1,000,000 + 1", "1,000,001"),
        ("$2k", "$2,000"),
        ("2M eur", "€2,000,000"),
        ("3 billion", "3,000,000,000"),
    ]);
}

#[test]
fn percentages() {
    check(&[
        ("$10 - 40%", "$6"),
        ("20% of $10", "$2"),
        ("5% on $30", "$31.50"),
        ("6% off 40 EUR", "€37.60"),
        ("$50 as a % of $100", "50%"),
        ("$70 as a % on $20", "250%"),
        ("$20 as a % off $70", "71.4286%"),
        ("5% of what is 6 EUR", "€120"),
        ("5% on what is 6 EUR", "€5.71"),
        ("5% off what is 6 EUR", "€6.32"),
        ("0.25 in %", "25%"),
        ("10% + 5%", "15%"),
    ]);
}

#[test]
fn currency() {
    check(&[
        ("$30 in Euro", "€25.80"),
        ("$30 CAD + 5 USD - 7EUR", "25.64 CAD"),
        ("100 yen in dollars", "$0.67"),
        ("1 btc in usd", "$100,000"),
        ("€10 + £10 in usd", "$24.96"),
    ]);
}

#[test]
fn variables_prev_sum_avg() {
    check(&[
        ("v = $20\nv2 = 5%\nv times 7 - v2", "$133"),
        ("v = $20\nv + $10", "$30"),
        ("Cost: $20 + $5\nDiscounted: prev - 20% discount", "$20"),
        ("Line 1: $10\nLine 2: $15\nResult: sum", "$25"),
        ("Line 1: $10\nLine 2: $20\nResult: average", "$15"),
        ("$10\n\n$15\ntotal", "$15"),
        ("1\n2\nsum\nsum", "3"),
        ("# Groceries\n$4\n$6\nsum", "$10"),
    ]);
}

#[test]
fn lists() {
    check(&[
        ("[1, 2, 3]", "[1, 2, 3]"),
        ("[1; 2; 3]", "[1, 2, 3]"),
        // Inside brackets a comma always separates values.
        ("[1,200,300]", "[1, 200, 300]"),
        ("[1200, 2.5]", "[1200, 2.5]"),
        ("[3 kg, 500 g, 2 kg]", "[3 kg, 0.5 kg, 2 kg]"),
        ("[10%, 20%]", "[10%, 20%]"),
        ("[$10, $12.50, €9]", "[$10, $12.50, $10.47]"),
        ("[1, 2, 3, 4, 5, 6, 7]", "[1, 2, 3, 4, 5, 6, 7]"),
        // A list too wide for the results column shows how many it holds.
        ("[48 km, 105 km, 149 km, 205 km]", "[48 km, … 4 values]"),
        ("[1; 2; 3; 4; 5; 6; 7; 8; 9; 10]", "[1, 2, 3, … 10 values]"),
        (
            "[123456789.123, 2, 3, 4, 5, 6, 7, 8]",
            "[123456789.123, … 8 values]",
        ),
        ("[]", "[]"),
        ("x = [1, 2]\nx = [x, 3]\nx", "[1, 2, 3]"),
        ("x = [1, 2]\nprev", "[1, 2]"),
        // A list is not one of the lines `sum` adds up.
        ("5\n[1, 2]\n10\nsum", "15"),
        (
            "[1, 2 kg, 3%]",
            "<no result: Some(\"cannot mix percentages and numbers\")>",
        ),
        (
            "[1 kg, 2 m]",
            "<no result: Some(\"cannot convert m to kg\")>",
        ),
        (
            "[today, 1]",
            "<no result: Some(\"dates cannot go in a list\")>",
        ),
        (
            "[1, 2] + 1",
            "<no result: Some(\"cannot combine these values\")>",
        ),
    ]);
}

#[test]
fn statistics() {
    check(&[
        ("median(3, 1, 2)", "2"),
        ("median(1, 2, 3, 4)", "2.5"),
        ("median([3, 1, 2])", "2"),
        ("median [3, 1, 2]", "2"),
        ("x = [3, 1, 2]\nmedian(x, 10)", "2.5"),
        ("stdev(2, 4, 4, 4, 5, 5, 7, 9)", "2.1381"),
        ("stddev(2, 4, 4, 4, 5, 5, 7, 9)", "2.1381"),
        ("pstdev(2, 4, 4, 4, 5, 5, 7, 9)", "2"),
        ("variance(2, 4, 4, 4, 5, 5, 7, 9)", "4.5714"),
        ("pvariance(2, 4, 4, 4, 5, 5, 7, 9)", "4"),
        ("sum([1, 2, 3])", "6"),
        ("sum(1, 2, 3)", "6"),
        ("mean([1, 2, 3, 6])", "3"),
        ("avg(2, 4)", "3"),
        ("average([2, 4])", "3"),
        ("count([4, 5, 6])", "3"),
        ("stdev([1, 2, 3, 4]) * 2", "2.582"),
        // Units and percentages carry through.
        ("median(3 kg, 500 g, 2 kg)", "2 kg"),
        ("stdev(1 m, 3 m)", "1.4142 m"),
        ("variance(1 m, 3 m)", "2 m²"),
        ("median(10%, 20%, 60%)", "20%"),
        ("mean([$10, $12.50, €9])", "$10.99"),
        // With nothing in the parentheses, or none, the lines above.
        ("10\n20\n60\nmedian", "20"),
        ("10\n20\n60\nmedian + 1", "21"),
        ("10\n20\n60\nstdev", "26.4575"),
        ("10\n20\n60\ncount()", "3"),
        ("10\n20\n60\nmedian\nmedian", "20"),
        ("$10\n$20\ntoday\n5%\nmedian", "$15"),
        ("10\n20\n\n4\n6\n8\nmedian", "6"),
        ("10\n20\nsum()", "30"),
        // `sum (2)` keeps its old meaning: the sum, times two.
        ("10\n20\nsum (2)", "60"),
        (
            "stdev(5)",
            "<no result: Some(\"stdev needs at least two values\")>",
        ),
        (
            "median([])",
            "<no result: Some(\"median needs at least one value\")>",
        ),
        (
            "median(1, 2%)",
            "<no result: Some(\"cannot mix percentages and numbers\")>",
        ),
    ]);
}

#[test]
fn straight_line_fits() {
    let xy = "x = [1, 2, 3, 4, 5]\ny = [2, 4, 5, 4, 5]\n";
    let trip = "hours = [1 h, 2 h, 3 h]\nkm = [50 km, 100 km, 150 km]\n";
    let with = |lines: &str, last: &str| format!("{lines}{last}");
    let cases = [
        (with(xy, "slope(x, y)"), "0.6"),
        (with(xy, "intercept(x, y)"), "2.2"),
        (with(xy, "correlation(x, y)"), "0.7746"),
        (with(xy, "corr(x, y)"), "0.7746"),
        (with(xy, "correl(y, x)"), "0.7746"),
        (with(xy, "rsquared(x, y)"), "0.6"),
        (with(xy, "r2(x, y)"), "0.6"),
        (with(xy, "predict(x, y, 10)"), "8.2"),
        (with(xy, "slope(x, y) * 10 + intercept(x, y)"), "8.2"),
        (with("", "slope([1, 2, 3], [3, 5, 7])"), "2"),
        (with("", "correlation([1, 2, 3], [9, 5, 1])"), "-1"),
        // Units: kilometres against hours is a speed.
        (with(trip, "slope(hours, km)"), "50 km/h"),
        (with(trip, "intercept(hours, km)"), "0 km"),
        (with(trip, "predict(hours, km, 90 min)"), "75 km"),
        (with(trip, "correlation(hours, km)"), "1"),
        (
            with("", "slope([1, 1], [2, 3])"),
            "<no result: Some(\"there is no line when every x is the same\")>",
        ),
        (
            with("", "correlation([1, 2], [3, 3])"),
            "<no result: Some(\"there is no correlation when a list never changes\")>",
        ),
        (
            with("", "slope([1, 2, 3], [1, 2])"),
            "<no result: Some(\"the two lists must hold the same number of values\")>",
        ),
        (
            with("", "slope([1, 2], 3)"),
            "<no result: Some(\"slope needs two lists: slope(x, y)\")>",
        ),
        (
            with(trip, "predict(hours, km, 5 kg)"),
            "<no result: Some(\"cannot convert kg to h\")>",
        ),
    ];
    let cases: Vec<(&str, &str)> = cases.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    check(&cases);
}

#[test]
fn method_comparison() {
    // Thirty pairs with one outlier; the expected values are from R (see
    // the tests in stats.rs).
    let data = "\
ref = [398.8, 183.6, 88.2, 74, 140.9, 332, 173.2, 397.8, 99.7, 218.6, 111.1, 135.5, 323.6, 78.1, 207.3, 74.8, 250.3, 53.7, 394.5, 140.9, 270.6, 159, 392.4, 366.1, 397.9, 80.3, 264.8, 220.3, 382.1, 174.7]
new = [405.6, 204.4, 95.3, 74.8, 144.8, 335, 172.7, 405.5, 123, 222.2, 115.2, 180.7, 332.3, 85.7, 207.2, 80.8, 254.1, 56.5, 413.4, 167, 290.8, 162.3, 421.2, 388.8, 414.4, 76.4, 269.4, 223, 395.7, 168]
";
    let mass = "a = [1 kg, 2 kg, 3 kg, 4 kg]\nb = [1100 g, 2050 g, 3200 g, 3900 g]\n";
    let trip = "hours = [1 h, 2 h, 3 h]\nkm = [50 km, 100 km, 160 km]\n";
    let with = |lines: &str, last: &str| format!("{lines}{last}");
    let no = |e: &str| format!("<no result: Some({e:?})>");
    let cases = [
        (with(data, "slope_ci(ref, new)"), "[0.986, 1.0576]".to_string()),
        (with(data, "intercept_ci(ref, new)"), "[-3.6557, 14.1719]".into()),
        (with(data, "pb_slope(ref, new)"), "1.0262".into()),
        (with(data, "pb_intercept(ref, new)"), "0.7102".into()),
        (with(data, "pb_slope_ci(ref, new)"), "[0.9994, 1.0602]".into()),
        (with(data, "pb_intercept_ci(ref, new)"), "[-5.5717, 6.5421]".into()),
        (with(data, "deming_slope(ref, new)"), "1.0261".into()),
        (with(data, "deming_slope(ref, new, 1)"), "1.0261".into()),
        (with(data, "deming_slope(ref, new, 0.25)"), "1.0236".into()),
        (with(data, "deming_intercept(ref, new)"), "4.3177".into()),
        (with(data, "deming_slope_ci(ref, new)"), "[0.9973, 1.0549]".into()),
        (with(data, "deming_intercept_ci(ref, new)"), "[-4.2115, 12.8468]".into()),
        // Each line on its own and all four together give the same numbers.
        (
            with(data, "pb_slope(ref, new)\npb_slope_ci(ref, new)\npb_intercept(ref, new)"),
            "0.7102".into(),
        ),
        // y is put in x's unit; the slope has none.
        (with(mass, "pb_slope(a, b)"), "0.9417".into()),
        (with(mass, "pb_intercept(a, b)"), "0.1625 kg".into()),
        (with(mass, "deming_intercept_ci(a, b)"), "[-0.4864 kg, 0.818 kg]".into()),
        // The least-squares slope keeps its units, so its interval does too.
        (with(trip, "slope_ci(hours, km)"), "[18.3203 km/h, 91.6797 km/h]".into()),
        (with(trip, "pb_slope(hours, km)"), no("both lists need the same kind of unit")),
        (
            with("", "pb_slope([1, 2, 3, 4], [8, 6, 4, 2])"),
            no("Passing-Bablok needs x and y that rise together"),
        ),
        (
            with("", "pb_slope_ci([1, 2, 3], [3, 4, 7])"),
            no("pb_slope_ci needs more points for a confidence interval"),
        ),
        (
            with("", "deming_slope_ci([1, 2], [3, 4])"),
            no("deming_slope_ci needs at least three points"),
        ),
        (
            with("", "slope_ci([1, 2], [3, 4])"),
            no("slope_ci needs at least three points"),
        ),
        (
            with("", "deming_slope([1, 2, 3], [5, 5, 5])"),
            no("there is no line when x and y do not vary together"),
        ),
        (
            with("", "deming_slope([1, 2, 3], [2, 4, 7], 0)"),
            no("the error ratio must be a positive number"),
        ),
        (
            with("", "deming_slope([1, 2, 3])"),
            no("deming_slope needs two lists, and an error ratio if it is not 1: deming_slope(x, y)"),
        ),
        (
            with("", "pb_slope([1, 2, 3], [2, 4, 7], 1)"),
            no("pb_slope needs two lists: pb_slope(x, y)"),
        ),
        (
            with("", "pb_slope([1, 2, 3], [1, 2])"),
            no("the two lists must hold the same number of values"),
        ),
        (
            with("", "pb_slope([10%, 20%], [1, 2])"),
            no("cannot mix percentages and numbers"),
        ),
    ];
    let cases: Vec<(&str, &str)> = cases
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    check(&cases);
}

/// The sample sheets that come with the app show features, so every line
/// that is not a heading or a note has to give a result.
#[test]
fn sample_sheets_give_a_result_on_every_line() {
    for (id, text) in reckon_core::sheets::SAMPLES {
        let out = engine().evaluate(text);
        for (line, o) in text.lines().zip(&out) {
            let wanted = o.kind == LineKind::Expr;
            assert_eq!(
                o.text.is_some(),
                wanted,
                "{id}: {line:?} gave {:?}",
                o.error
            );
            // Results sit in a narrow column, most of all on a phone.
            let width = o.text.as_deref().map_or(0, |t| t.chars().count());
            assert!(width <= 24, "{id}: {line:?} gives {width} characters");
        }
    }
}

/// These words were ignored in notes before they became functions, and a
/// note that uses them must keep its result.
#[test]
fn statistics_words_in_notes() {
    check(&[
        ("median price 500", "500"),
        ("50000 median income", "50,000"),
        ("$5 slope", "$5"),
        ("correlation 0.9", "0.9"),
        ("head count 12", "12"),
        ("intercept 4 + 1", "5"),
        ("12 count", "12"),
    ]);
}

#[test]
fn css() {
    check(&[
        ("12 pt in px", "16 px"),
        ("em = 20px\n1.2 em in px", "24 px"),
        ("1 inch in px", "96 px"),
        ("ppi = 326\n1 cm in px", "128.3465 px"),
    ]);
}

#[test]
fn units() {
    check(&[
        ("1 mm", "1 mm"),
        ("3 GB in MB", "3,000 MB"),
        ("1 KiB in B", "1,024 B"),
        ("1 kB in b", "8,000 b"),
        ("1 inch in cm", "2.54 cm"),
        ("5 ft 3 inches", "63 in"),
        ("1 mile in km", "1.6093 km"),
        ("2 nautical miles in m", "3,704 m"),
        ("20 sq cm", "20 cm²"),
        ("30 square inches", "30 in²"),
        ("11 sqm", "11 m²"),
        ("20 cu cm", "20 cm³"),
        ("11 cbm", "11 m³"),
        ("1 acre in sq m", "4,046.8564 m²"),
        ("1 hectare in acres", "2.4711 acres"),
        ("2 m * 3 m", "6 m²"),
        ("1 gallon in liters", "3.7854 L"),
        ("3 tea spoons in ml", "14.7868 mL"),
        ("1 kg in pounds", "2.2046 lb"),
        ("1 stone in kg", "6.3503 kg"),
        ("100 °C in °F", "212 °F"),
        ("0 kelvin in celsius", "-273.15 °C"),
        ("180 degrees in radians", "3.1416 rad"),
        ("60 mph in km/h", "96.5606 km/h"),
        ("10 km / 2 h", "5 km/h"),
        ("1 hour / 30 min", "2"),
        ("round(1 month in days)", "30 days"),
        ("1 year in days", "365 days"),
        ("90 min in hours", "1.5 h"),
        ("$120 / 4 h", "$30/h"),
    ]);
}

#[test]
fn functions() {
    check(&[
        ("sqrt 16", "4"),
        ("sqrt(16 m^2)", "4 m"),
        ("cbrt 8", "2"),
        ("root 2 (8)", "2.8284"),
        ("log 2 (8)", "3"),
        ("log 1000", "3"),
        ("ln 1", "0"),
        ("fact 5", "120"),
        ("abs(-4)", "4"),
        ("round 3.45", "3"),
        ("round(3.456; 2)", "3.46"),
        ("ceil 3.76", "4"),
        ("floor 2.56", "2"),
        ("sin 45°", "0.7071"),
        ("cos(pi)", "-1"),
        ("tan 0", "0"),
    ]);
}

#[test]
fn dates_and_times() {
    check(&[
        ("today", "Sat, 7 Nov 2026"),
        ("tomorrow", "Sun, 8 Nov 2026"),
        ("today + 2 weeks", "Sat, 21 Nov 2026"),
        ("now", "12:00 pm"),
        ("now + 90 min", "1:30 pm"),
        ("3 days ago", "Wed, 4 Nov 2026"),
        ("2 weeks from now", "Sat, 21 Nov 2026"),
        ("fromunix(1446587186)", "Tue, 3 Nov 2015, 9:46 pm"),
        ("2026-12-25 - today", "48 days"),
        ("March 5 2027 - today", "118 days"),
        ("Jan 31 + 1 month", "Sat, 28 Feb 2026"),
        ("5 March 2025", "Wed, 5 Mar 2025"),
        ("3pm", "3:00 pm"),
        ("2:30 pm - 9:15 am", "5.25 h"),
        ("PST time", "4:00 am PST"),
        ("New York time", "7:00 am UTC-5"),
        ("Time in Madrid", "1:00 pm UTC+1"),
        ("2:30 pm HKT in Berlin", "7:30 am UTC+1"),
        ("now in UTC+2", "2:00 pm UTC+2"),
    ]);
}

#[test]
fn formatting_lines() {
    let e = engine();
    let out = e.evaluate("# This is header\n$275 for the \"Model 227\"\n// This is comment\nPrice: $11 + $34.45\nhello world");
    assert_eq!(out[0].kind, LineKind::Header);
    assert_eq!(out[1].text.as_deref(), Some("$275"));
    assert_eq!(out[2].kind, LineKind::Comment);
    assert_eq!(out[3].text.as_deref(), Some("$45.45"));
    assert_eq!(out[4].text, None);
    assert_eq!(out[4].error, None);
}

#[test]
fn plugins() {
    let mut e = engine();
    let src = "# sample\nunit horse \"horse, horses\" = 2.4 m\nunit floz \"fl oz, fluid ounce, fluid ounces\" = 29.5735 ml\nvar xxx = 5 USD\nfn zum(a; b) = a + b\nfn hypot(a; b) = sqrt(a^2 + b^2)";
    assert_eq!(e.load_plugin("sample", src), Ok(5));
    let cases = [
        ("3 horses in m", "7.2 m"),
        ("xxx * 2", "$10"),
        ("zum(1; 5)", "6"),
        ("hypot(3; 4)", "5"),
        ("2 fl oz in ml", "59.147 mL"),
    ];
    for (input, want) in cases {
        assert_eq!(e.eval_str(input).as_deref(), Ok(want), "{input}");
    }
    assert!(e.load_plugin("bad", "unit m \"x\" = 1 m").is_err());
    assert!(e.load_plugin("bad", "nonsense").is_err());
}

#[test]
fn lists_as_wide_as_there_is_room_and_copied_whole() {
    let e = engine();
    let out = e.evaluate("x = [405.6, 204.4, 95.3, 74.8, 144.8, 335, 0.1, 1234567.25]");
    let v = out[0].value.clone().unwrap();
    assert_eq!(out[0].text.as_deref(), Some("[405.6, … 8 values]"));
    assert_eq!(
        e.format_width(&v, Some(40)),
        "[405.6, 204.4, 95.3, 74.8, … 8 values]"
    );
    assert_eq!(
        e.format_width(&v, Some(200)),
        "[405.6, 204.4, 95.3, 74.8, 144.8, 335, 0.1, 1234567.25]"
    );
    assert_eq!(
        e.format_for_copy(&v),
        "[405.6, 204.4, 95.3, 74.8, 144.8, 335, 0.1, 1234567.25]"
    );
    let out = e.evaluate("y = [0.1 + 0.2, 1/3, 2 kg, 1e-9]");
    let v = out[0].value.clone().unwrap();
    assert_eq!(
        e.format_for_copy(&v),
        "[0.3 kg, 0.333333333333333 kg, 2 kg, 0.000000001 kg]"
    );
    let out = e.evaluate("z = [1 kg, 2.5 kg]\n50");
    assert_eq!(
        e.format_for_copy(out[0].value.as_ref().unwrap()),
        "[1 kg, 2.5 kg]"
    );
    assert_eq!(e.format_for_copy(out[1].value.as_ref().unwrap()), "50");
}

#[test]
fn machine_learning_units() {
    check(&[
        ("7B params", "7,000,000,000 params"),
        ("2T tokens", "2,000,000,000,000 tokens"),
        // `7B` alone is still seven bytes.
        ("7B", "7 B"),
        ("70B params * bf16 in GB", "140 GB"),
        ("70B params * bf16", "140,000,000,000 B"),
        ("8B params * int4 in GiB", "3.7253 GiB"),
        ("70B params * bf16 / (7B params)", "20 B"),
        ("2 * 80 * 8 * 128 * bf16 / token", "327,680 B/tok"),
        (
            "kv = 2 * 80 * 8 * 128 * bf16 / token\nkv * 32768 tokens * 8 in GiB",
            "80 GiB",
        ),
        ("6 * 7B params * 2T tokens", "8.4e22"),
        ("6 * 7B params * 2T tokens in ZFLOP", "84 ZFLOP"),
        ("12 FLOPs", "12 FLOP"),
        ("1 teraflops", "1 TFLOPS"),
        ("989 TFLOPS * 30 days in EFLOP", "2,563.488 EFLOP"),
        (
            "8.4e22 FLOP / (64 * 989 TFLOPS * 40%) in days",
            "38.3998 days",
        ),
        ("1500 tokens/s * 1 h", "5,400,000 tokens"),
        ("1500 tokens/s * 1 h in Mtok", "5.4 Mtok"),
        ("50M tokens * $3/Mtok", "$150"),
        ("50 million tokens * $3 per million tokens", "$150"),
        (
            "1000 * (2000 tokens * $3/Mtok + 500 tokens * $15/Mtok)",
            "$13.50",
        ),
        ("$2.50/h / (1500 tokens/s) in $/Mtok", "$0.46/Mtok"),
        (
            "3.35 TB/s / (70B params * bf16) in tokens/s",
            "23.9286 tok/s",
        ),
        (
            "5 tokens in GB",
            "<no result: Some(\"cannot convert tok to GB\")>",
        ),
        // Different kinds of count do not mix; a plain number goes with any.
        (
            "3 tokens + 2 params",
            "<no result: Some(\"cannot convert param to tok\")>",
        ),
        (
            "1M tokens in FLOP",
            "<no result: Some(\"cannot convert tok to FLOP\")>",
        ),
        (
            "7B params in tokens",
            "<no result: Some(\"cannot convert param to tok\")>",
        ),
        ("5 Mtok + 2000 tokens", "5.002 Mtok"),
        ("8.4e22 in EFLOP", "84,000 EFLOP"),
    ]);
}

#[test]
fn machine_learning_functions() {
    let ab = "a = [1, 0, 1, 1, 0, 1, 1, 1, 0, 1]\nb = [1, 1, 1, 1, 1, 1, 1, 1, 0, 1]\n";
    let xy = "x = [0.61, 0.72, 0.55, 0.80, 0.67, 0.59, 0.70, 0.64, 0.75, 0.58]\n\
              y = [0.66, 0.74, 0.60, 0.79, 0.71, 0.65, 0.73, 0.70, 0.77, 0.62]\n";
    let with = |lines: &str, last: &str| format!("{lines}{last}");
    let no = |e: &str| format!("<no result: Some({e:?})>");
    let cases = [
        with("", "sigmoid(0)"),
        with("", "sigmoid 2"),
        with("", "logit(88%)"),
        with("", "log2(1024)"),
        with("", "softmax(1, 2, 3)"),
        with("", "softmax([2, 1, 0.1])"),
        with("", "logsumexp(1, 2, 3)"),
        with("", "perplexity(2.1)"),
        with("", "accuracy_ci(82, 100)"),
        with("", "accuracy_ci(82%, 100)"),
        with("", "accuracy_ci(0, 20)"),
        with("", "mcnemar(30, 12)"),
        with("", "mcnemar(7, 1)"),
        with(ab, "mcnemar(a, b)"),
        with(xy, "paired_ci(x, y)"),
        with("", "items_needed(80%, 2%)"),
        with("", "holm(0.01, 0.04, 0.03, 0.2)"),
        with("", "logit(1.5)"),
        with("", "accuracy_ci(101, 100)"),
        with("", "accuracy_ci(1.5, 10)"),
        with("", "mcnemar([1, 2], [0, 1])"),
        with("", "mcnemar([1, 0], [1])"),
        with("", "holm(2)"),
        with("", "softmax([1 kg, 2 kg])"),
        with("", "items_needed(80%, 0%)"),
        with("", "paired_ci([1], [2])"),
    ];
    let want = [
        "0.5".to_string(),
        "0.8808".into(),
        "1.9924".into(),
        "10".into(),
        "[0.09, 0.2447, 0.6652]".into(),
        "[0.659, 0.2424, 0.0986]".into(),
        "3.4076".into(),
        "8.1662".into(),
        "[73.3326%, 88.2998%]".into(),
        "[73.3326%, 88.2998%]".into(),
        "[0%, 16.1125%]".into(),
        "0.0087".into(),
        "0.0703".into(),
        "0.5".into(),
        "[0.0205, 0.0515]".into(),
        "1,537".into(),
        "[0.04, 0.09, 0.09, 0.2]".into(),
        no("logit needs a probability between 0 and 1"),
        no("accuracy_ci needs correct ≤ total, and a total above 0"),
        no("the number correct must be a whole number of items"),
        no("mcnemar needs lists of 1 (right) and 0 (wrong)"),
        no("the two lists must hold the same number of values"),
        no("holm needs p-values between 0 and 1"),
        no("softmax works on plain numbers"),
        no("items_needed needs percentages between 0% and 100%"),
        no("paired_ci needs at least two items"),
    ];
    let cases: Vec<(&str, &str)> = cases
        .iter()
        .zip(&want)
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    check(&cases);
}

/// A typo must not quietly give a number.
#[test]
fn unknown_functions_and_units_are_errors() {
    check(&[
        (
            "sigmoyd(0)",
            "<no result: Some(\"unknown function sigmoyd\")>",
        ),
        (
            "989TFLOPZ",
            "<no result: Some(\"unknown unit TFLOPZ; did you mean TFLOPS?\")>",
        ),
        (
            "4bytess",
            "<no result: Some(\"unknown unit bytess; did you mean bytes?\")>",
        ),
        // Other words stuck to a number are ignored, as they always were.
        ("2pcs at $5", "$10"),
        ("Screen: 1080p", "1,080"),
        ("5kgz", "5"),
        ("2x", "2"),
    ]);
}

/// Everyday notes give what they gave before the unknown-unit check: only a
/// near-miss of a real unit is an error. The expected results are main's at
/// 0b7f5e5.
#[test]
fn everyday_notes_are_unchanged() {
    let rows: &[(&str, &str)] = &[
        ("# Shopping", ""),
        ("2pcs at $5", "$10"),
        ("3 apples at $1.20", "$3.60"),
        ("Screen: 1080p", "1,080"),
        ("TV 4K 55in for $499", ""),
        ("2x faster", "2"),
        ("10x cheaper than last year", "10 years"),
        ("Paid $20 for 3kg of rice", "60 kg·USD"),
        ("5kgs of flour 12", ""),
        ("12oz coffee $4", ""),
        ("Bought 2pkts of milk 3", ""),
        ("the 3rd box costs 4", "12"),
        ("my 2nd try: 7", "7"),
        ("1st quarter revenue $1,200", ""),
        ("Q3 total 4500", "$5,015,700"),
        ("Room 101: 3 nights at $80", "$240"),
        ("Flight 6E2134 $230", "$∞"),
        ("iPhone 15 Pro $999", "$14,985"),
        ("Model T 1908", "1,908"),
        ("covid-19 test $50", "-$950"),
        ("24/7 support 30", "102.8571"),
        ("5G plan $40", "$200"),
        ("A4 paper 500 sheets 6", "3,000"),
        ("B2B sales 1200", "1,200"),
        ("Size 10.5 shoes $80", "$840"),
        ("v2.0 release 3 days", "0 days"),
        ("Monday: 42", "42"),
        ("Tuesday 38", "38"),
        ("sum", "$∞"),
        ("# Work", ""),
        ("8 GPUs * $2/h * 24 h", "$384"),
        ("standup 15min", "15 min"),
        ("meeting 1hr 30min", "90 min"),
        ("3hrs at $40/h", ""),
        ("commute 25mins each way * 2", "50 min"),
        ("2wks vacation", "2 weeks"),
        ("6mos rent $1,200 * 6", "$43,200"),
        ("5yrs at 3%", "0.15 years"),
        ("deadline in 3days", ""),
        ("version 3b", "3 b"),
        ("batch size 32", "32"),
        ("lr 3e-4", "0.0003"),
        ("epochs 10", "10"),
        ("seq len 4096", "4,096"),
        ("1.5x speedup on 8 nodes", ""),
        ("50ms latency p99", "50 ms"),
        ("200req/s for 1 h", "720,000"),
        ("2TB disk at $0.02/GB", ""),
        ("12GB VRAM", "12 GB"),
        ("16gb ram", "16"),
        ("100Mbps link for 1 h in GB", ""),
        ("total", "$767,738"),
    ];
    let sheet: Vec<&str> = rows.iter().map(|r| r.0).collect();
    let out = engine().evaluate(&sheet.join("\n"));
    let mut failures = Vec::new();
    for ((line, want), o) in rows.iter().zip(&out) {
        let got = o.text.as_deref().unwrap_or("");
        if got != *want {
            failures.push(format!("{line:?}: want {want:?}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn money_with_its_sign_and_small_amounts() {
    check(&[
        // Sign or code, typed either way, money per something shows the sign.
        ("$5/h", "$5/h"),
        ("5 USD/h", "$5/h"),
        ("USD 5/h", "$5/h"),
        ("₹200/h", "₹200/h"),
        ("200 INR/h", "₹200/h"),
        ("£2/kg", "£2/kg"),
        ("2 GBP/kg", "£2/kg"),
        ("₹200/h / (1500 tokens/s) in INR/Mtok", "₹37.04/Mtok"),
        ("$5/kg/h", "$5/kg·h"),
        // A currency with no sign keeps its code.
        ("5 CHF/h", "5 CHF/h"),
        // Under ten cents, three significant digits instead of cents.
        ("2000 tokens * $3/Mtok", "$0.006"),
        ("2000 tokens * $3/Mtok + 500 tokens * $15/Mtok", "$0.0135"),
        ("$0.05", "$0.05"),
        ("$0.075", "$0.075"),
        ("$1 / 3", "$0.33"),
        ("$0.1", "$0.10"),
        ("$27.6", "$27.60"),
        ("$20", "$20"),
        ("[$0.004, $0.5, $2]", "[$0.004, $0.50, $2]"),
        ("$1e-20", "$1e-20"),
    ]);
}

#[test]
fn bland_altman_agreement() {
    // The thirty pairs of `method_comparison`; values from R (see stats.rs).
    let data = "\
ref = [398.8, 183.6, 88.2, 74, 140.9, 332, 173.2, 397.8, 99.7, 218.6, 111.1, 135.5, 323.6, 78.1, 207.3, 74.8, 250.3, 53.7, 394.5, 140.9, 270.6, 159, 392.4, 366.1, 397.9, 80.3, 264.8, 220.3, 382.1, 174.7]
new = [405.6, 204.4, 95.3, 74.8, 144.8, 335, 172.7, 405.5, 123, 222.2, 115.2, 180.7, 332.3, 85.7, 207.2, 80.8, 254.1, 56.5, 413.4, 167, 290.8, 162.3, 421.2, 388.8, 414.4, 76.4, 269.4, 223, 395.7, 168]
";
    let mass = "a = [1 kg, 2 kg, 3 kg]\nb = [1100 g, 2050 g, 2900 g]\n";
    let with = |lines: &str, last: &str| format!("{lines}{last}");
    let no = |e: &str| format!("<no result: Some({e:?})>");
    let cases = [
        (with(data, "ba_bias(ref, new)"), "10.0467".to_string()),
        (
            with(data, "ba_bias_ci(ref, new)"),
            "[5.8056, 14.2877]".into(),
        ),
        (
            with(data, "ba_limits(ref, new)"),
            "[-12.2139, 32.3073]".into(),
        ),
        (
            with(data, "ba_lower_ci(ref, new)"),
            "[-19.5596, -4.8683]".into(),
        ),
        (
            with(data, "ba_upper_ci(ref, new)"),
            "[24.9616, 39.6529]".into(),
        ),
        // Swapping the methods turns the differences round.
        (with(data, "ba_bias(new, ref)"), "-10.0467".into()),
        // Differences are in x's unit.
        (with(mass, "ba_bias(a, b)"), "0.0167 kg".into()),
        (
            with(mass, "ba_limits(a, b)"),
            "[-0.1873 kg, 0.2207 kg]".into(),
        ),
        (with("", "ba_bias([10%, 20%], [12%, 21%])"), "1.5%".into()),
        (
            with("", "ba_bias([1], [2])"),
            no("ba_bias needs at least two points"),
        ),
        (
            with("", "ba_limits([1, 2], [1, 2, 3])"),
            no("the two lists must hold the same number of values"),
        ),
        (
            with("", "ba_bias([1, 2, 3])"),
            no("ba_bias needs two lists: ba_bias(x, y)"),
        ),
    ];
    let cases: Vec<(&str, &str)> = cases
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    check(&cases);
}
