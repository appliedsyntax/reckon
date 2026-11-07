//! Currencies: metadata, exchange rates, cache file and online refresh.
//!
//! Rates are stored as "units per 1 USD". Refreshing shells out to the
//! system `curl` (shipped with macOS, Windows 10+ and most Linux distros)
//! so the engine needs no HTTP or TLS dependency.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::json;

pub struct CurrencyInfo {
    pub code: &'static str,
    /// Symbol shown before the amount (`$20`). `None` shows the code after.
    pub prefix_symbol: Option<&'static str>,
    /// Case-insensitive names and aliases.
    pub names: &'static [&'static str],
}

macro_rules! cur {
    ($code:literal, $sym:expr, [$($n:literal),*]) => {
        CurrencyInfo { code: $code, prefix_symbol: $sym, names: &[$($n),*] }
    };
}

pub static CURRENCIES: &[CurrencyInfo] = &[
    cur!(
        "USD",
        Some("$"),
        [
            "usd",
            "dollar",
            "dollars",
            "buck",
            "bucks",
            "us dollar",
            "us dollars"
        ]
    ),
    cur!("EUR", Some("€"), ["eur", "euro", "euros"]),
    cur!(
        "GBP",
        Some("£"),
        ["gbp", "sterling", "british pound", "british pounds", "quid"]
    ),
    cur!("JPY", Some("¥"), ["jpy", "yen"]),
    cur!("INR", Some("₹"), ["inr", "rupee", "rupees"]),
    cur!("CNY", None, ["cny", "yuan", "renminbi", "rmb"]),
    cur!("CAD", None, ["cad", "canadian dollar", "canadian dollars"]),
    cur!(
        "AUD",
        None,
        ["aud", "australian dollar", "australian dollars"]
    ),
    cur!(
        "NZD",
        None,
        ["nzd", "new zealand dollar", "new zealand dollars"]
    ),
    cur!(
        "CHF",
        None,
        ["chf", "franc", "francs", "swiss franc", "swiss francs"]
    ),
    cur!(
        "RUB",
        Some("₽"),
        ["rub", "rouble", "roubles", "ruble", "rubles"]
    ),
    cur!("KRW", Some("₩"), ["krw", "won"]),
    cur!("BRL", None, ["brl", "real", "reais"]),
    cur!(
        "MXN",
        None,
        ["mxn", "peso", "pesos", "mexican peso", "mexican pesos"]
    ),
    cur!(
        "SGD",
        None,
        ["sgd", "singapore dollar", "singapore dollars"]
    ),
    cur!(
        "HKD",
        None,
        ["hkd", "hong kong dollar", "hong kong dollars"]
    ),
    cur!("SEK", None, ["sek", "swedish krona"]),
    cur!("NOK", None, ["nok", "norwegian krone"]),
    cur!("DKK", None, ["dkk", "danish krone"]),
    cur!("PLN", None, ["pln", "zloty", "zlotys"]),
    cur!("CZK", None, ["czk", "koruna"]),
    cur!("HUF", None, ["huf", "forint", "forints"]),
    cur!("RON", None, ["ron", "leu", "lei"]),
    cur!("BGN", None, ["bgn", "lev", "leva"]),
    cur!("TRY", Some("₺"), ["lira", "turkish lira"]),
    cur!("ILS", Some("₪"), ["ils", "shekel", "shekels"]),
    cur!("ZAR", None, ["zar", "rand"]),
    cur!("AED", None, ["aed", "dirham", "dirhams"]),
    cur!("SAR", None, ["sar", "riyal", "riyals"]),
    cur!("QAR", None, ["qar"]),
    cur!("KWD", None, ["kwd"]),
    cur!("BHD", None, ["bhd"]),
    cur!("OMR", None, ["omr"]),
    cur!("JOD", None, ["jod"]),
    cur!("EGP", None, ["egp"]),
    cur!("MAD", None, ["mad"]),
    cur!("NGN", Some("₦"), ["ngn", "naira"]),
    cur!("KES", None, ["kes", "shilling", "shillings"]),
    cur!("THB", Some("฿"), ["thb", "baht"]),
    cur!("IDR", None, ["idr", "rupiah"]),
    cur!("MYR", None, ["myr", "ringgit"]),
    cur!("PHP", Some("₱"), ["php"]),
    cur!("VND", Some("₫"), ["vnd", "dong"]),
    cur!("TWD", None, ["twd"]),
    cur!("PKR", None, ["pkr"]),
    cur!("BDT", None, ["bdt", "taka"]),
    cur!("LKR", None, ["lkr"]),
    cur!("NPR", None, ["npr"]),
    cur!("KZT", Some("₸"), ["kzt", "tenge"]),
    cur!("UAH", Some("₴"), ["uah", "hryvnia"]),
    cur!("GEL", Some("₾"), ["gel", "lari"]),
    cur!("ARS", None, ["ars"]),
    cur!("CLP", None, ["clp"]),
    cur!("COP", None, ["cop"]),
    cur!("PEN", None, ["pen", "sol", "soles"]),
    cur!("ISK", None, ["isk"]),
    cur!("BTC", Some("₿"), ["btc", "bitcoin", "bitcoins"]),
    cur!("ETH", None, ["eth", "ether", "ethereum"]),
    cur!("SOL", None, ["solana"]),
    cur!("USDT", None, ["usdt", "tether"]),
    cur!("XRP", None, ["xrp", "ripple"]),
    cur!("DOGE", None, ["doge", "dogecoin"]),
    cur!("ADA", None, ["cardano"]),
    cur!("LTC", None, ["ltc", "litecoin"]),
];

/// Approximate fallback rates (units per USD) used until a refresh succeeds.
const BUILTIN_RATES: &[(&str, f64)] = &[
    ("USD", 1.0),
    ("EUR", 0.86),
    ("GBP", 0.75),
    ("JPY", 150.0),
    ("INR", 88.0),
    ("CNY", 7.12),
    ("CAD", 1.39),
    ("AUD", 1.53),
    ("NZD", 1.73),
    ("CHF", 0.80),
    ("RUB", 80.0),
    ("KRW", 1400.0),
    ("BRL", 5.40),
    ("MXN", 18.5),
    ("SGD", 1.30),
    ("HKD", 7.78),
    ("SEK", 9.50),
    ("NOK", 10.1),
    ("DKK", 6.42),
    ("PLN", 3.65),
    ("CZK", 21.0),
    ("HUF", 335.0),
    ("RON", 4.38),
    ("BGN", 1.68),
    ("TRY", 41.5),
    ("ILS", 3.30),
    ("ZAR", 17.5),
    ("AED", 3.6725),
    ("SAR", 3.75),
    ("QAR", 3.64),
    ("KWD", 0.306),
    ("BHD", 0.376),
    ("OMR", 0.385),
    ("JOD", 0.709),
    ("EGP", 48.0),
    ("MAD", 9.1),
    ("NGN", 1500.0),
    ("KES", 129.0),
    ("THB", 32.5),
    ("IDR", 16500.0),
    ("MYR", 4.20),
    ("PHP", 57.5),
    ("VND", 26300.0),
    ("TWD", 30.5),
    ("PKR", 282.0),
    ("BDT", 122.0),
    ("LKR", 300.0),
    ("NPR", 140.0),
    ("KZT", 530.0),
    ("UAH", 41.5),
    ("GEL", 2.70),
    ("ARS", 1400.0),
    ("CLP", 950.0),
    ("COP", 3900.0),
    ("PEN", 3.40),
    ("ISK", 125.0),
    ("BTC", 1.0 / 100_000.0),
    ("ETH", 1.0 / 3_500.0),
    ("SOL", 1.0 / 180.0),
    ("USDT", 1.0),
    ("XRP", 1.0 / 2.5),
    ("DOGE", 1.0 / 0.2),
    ("ADA", 1.0 / 0.7),
    ("LTC", 1.0 / 90.0),
];

const CRYPTO_IDS: &[(&str, &str)] = &[
    ("bitcoin", "BTC"),
    ("ethereum", "ETH"),
    ("solana", "SOL"),
    ("tether", "USDT"),
    ("ripple", "XRP"),
    ("dogecoin", "DOGE"),
    ("cardano", "ADA"),
    ("litecoin", "LTC"),
];

const FIAT_URL: &str = "https://open.er-api.com/v6/latest/USD";

#[derive(Clone, Debug)]
pub struct Rates {
    per_usd: HashMap<String, f64>,
    /// Unix time of the last successful refresh, `None` for built-in values.
    pub updated: Option<i64>,
    pub source: String,
}

impl Default for Rates {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Rates {
    pub fn builtin() -> Self {
        Rates {
            per_usd: BUILTIN_RATES
                .iter()
                .map(|(c, r)| (c.to_string(), *r))
                .collect(),
            updated: None,
            source: "built-in (approximate)".into(),
        }
    }

    /// Value of one unit of `code` in USD.
    pub fn usd_value(&self, code: &str) -> Option<f64> {
        self.per_usd
            .get(code)
            .filter(|r| **r > 0.0)
            .map(|r| 1.0 / r)
    }

    pub fn codes(&self) -> impl Iterator<Item = &str> {
        self.per_usd.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.per_usd.len()
    }

    pub fn is_empty(&self) -> bool {
        self.per_usd.is_empty()
    }

    /// Overlays `other` on top of these rates (keeps codes `other` lacks).
    pub fn merge(&mut self, other: Rates) {
        self.per_usd.extend(other.per_usd);
        self.updated = other.updated;
        self.source = other.source;
    }

    pub fn load(path: &Path) -> Result<Rates, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut lines = text.lines();
        if lines.next() != Some("reckon-rates 1") {
            return Err("unrecognised rates file".into());
        }
        let mut rates = Rates {
            per_usd: HashMap::new(),
            updated: None,
            source: "cache".into(),
        };
        for line in lines {
            let mut it = line.splitn(2, ' ');
            match (it.next(), it.next()) {
                (Some("updated"), Some(v)) => rates.updated = v.trim().parse().ok(),
                (Some("source"), Some(v)) => rates.source = v.trim().to_string(),
                (Some(code), Some(v)) => {
                    if let Ok(r) = v.trim().parse::<f64>() {
                        rates.per_usd.insert(code.to_string(), r);
                    }
                }
                _ => {}
            }
        }
        Ok(rates)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut codes: Vec<_> = self.per_usd.iter().collect();
        codes.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = String::from("reckon-rates 1\n");
        if let Some(u) = self.updated {
            out.push_str(&format!("updated {u}\n"));
        }
        out.push_str(&format!("source {}\n", self.source));
        for (c, r) in codes {
            out.push_str(&format!("{c} {r}\n"));
        }
        std::fs::write(path, out).map_err(|e| e.to_string())
    }

    /// Downloads fresh fiat (and, best effort, crypto) rates.
    /// URL of the fiat rate service (base USD).
    pub const FIAT_URL: &'static str = FIAT_URL;

    /// URL of the crypto price service.
    pub fn crypto_url() -> String {
        let ids: Vec<&str> = CRYPTO_IDS.iter().map(|c| c.0).collect();
        format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
            ids.join(",")
        )
    }

    /// Builds rates from the raw JSON bodies of [`Rates::FIAT_URL`] and
    /// (optionally) [`Rates::crypto_url`]. Front ends without `curl`
    /// (iOS, Android) download these themselves and pass them in.
    pub fn from_responses(fiat: &str, crypto: Option<&str>) -> Result<Rates, String> {
        let j = json::parse(fiat)?;
        if j.get("result")
            .is_some_and(|r| *r != json::Json::Str("success".into()))
        {
            return Err("rate service returned an error".into());
        }
        let table = j
            .get("rates")
            .and_then(|r| r.as_obj())
            .ok_or("no rates in response")?;
        let mut per_usd: HashMap<String, f64> = table
            .iter()
            .filter_map(|(k, v)| v.as_f64().map(|r| (k.clone(), r)))
            .collect();
        let updated = j
            .get("time_last_update_unix")
            .and_then(|v| v.as_f64())
            .map(|v| v as i64)
            .unwrap_or_else(|| crate::time::unix_now() as i64);
        let mut source = "ExchangeRate-API".to_string();
        if let Some(Ok(j)) = crypto.map(json::parse) {
            for (id, code) in CRYPTO_IDS {
                if let Some(price) = j
                    .get(id)
                    .and_then(|o| o.get("usd"))
                    .and_then(|p| p.as_f64())
                {
                    if price > 0.0 {
                        per_usd.insert(code.to_string(), 1.0 / price);
                    }
                }
            }
            source.push_str(" + CoinGecko");
        }
        Ok(Rates {
            per_usd,
            updated: Some(updated),
            source,
        })
    }

    /// Downloads fresh fiat (and, best effort, crypto) rates with `curl`.
    pub fn fetch() -> Result<Rates, String> {
        let fiat = curl(FIAT_URL)?;
        let crypto = curl(&Self::crypto_url()).ok();
        Self::from_responses(&fiat, crypto.as_deref())
    }
}

fn curl(url: &str) -> Result<String, String> {
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "15", url])
        .output()
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

/// Maps a currency sign to its ISO code.
pub fn symbol_code(c: char) -> Option<&'static str> {
    Some(match c {
        '$' => "USD",
        '€' => "EUR",
        '£' => "GBP",
        '¥' => "JPY",
        '₹' => "INR",
        '₽' => "RUB",
        '₩' => "KRW",
        '₺' => "TRY",
        '₪' => "ILS",
        '฿' => "THB",
        '₫' => "VND",
        '₱' => "PHP",
        '₴' => "UAH",
        '₦' => "NGN",
        '₸' => "KZT",
        '₾' => "GEL",
        '₿' => "BTC",
        _ => return None,
    })
}
