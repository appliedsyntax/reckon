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
        ("$120 / 4 h", "30 USD/h"),
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
