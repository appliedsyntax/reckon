//! Time zones without dependencies.
//!
//! On macOS and Linux the system zoneinfo database (TZif files) is read
//! directly, including the POSIX TZ footer used for dates past the last
//! transition. Where no database exists (Windows) named zones fall back to
//! a fixed standard offset from the built-in city table.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::time::{days_from_civil, days_in_month, is_leap, weekday};

#[derive(Clone, Debug)]
struct TType {
    off: i32,
    abbr: String,
}

/// Parsed contents of one TZif file.
#[derive(Debug)]
pub struct TzData {
    trans: Vec<i64>,
    idx: Vec<u8>,
    types: Vec<TType>,
    footer: Option<PosixTz>,
}

impl TzData {
    pub fn parse(b: &[u8]) -> Result<TzData, String> {
        if b.len() < 44 || &b[0..4] != b"TZif" {
            return Err("not a TZif file".into());
        }
        let version = b[4];
        let counts = |at: usize| -> [usize; 6] {
            let mut c = [0usize; 6];
            for (k, v) in c.iter_mut().enumerate() {
                let i = at + 20 + k * 4;
                *v = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
            }
            c
        };
        let [isut, isstd, leap, timecnt, typecnt, charcnt] = counts(0);
        let v1_len = timecnt * 5 + typecnt * 6 + charcnt + leap * 8 + isstd + isut;
        let (start, tsize, c) = if version >= b'2' && b.len() >= 44 + v1_len + 44 {
            let h2 = 44 + v1_len;
            (h2 + 44, 8usize, counts(h2))
        } else {
            (44, 4usize, [isut, isstd, leap, timecnt, typecnt, charcnt])
        };
        let [isut, isstd, leap, timecnt, typecnt, charcnt] = c;
        let need = start
            + timecnt * (tsize + 1)
            + typecnt * 6
            + charcnt
            + leap * (tsize + 4)
            + isstd
            + isut;
        if b.len() < need {
            return Err("truncated TZif file".into());
        }
        let mut p = start;
        let mut trans = Vec::with_capacity(timecnt);
        for _ in 0..timecnt {
            let t = if tsize == 8 {
                i64::from_be_bytes(b[p..p + 8].try_into().unwrap())
            } else {
                i32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as i64
            };
            trans.push(t);
            p += tsize;
        }
        let idx = b[p..p + timecnt].to_vec();
        p += timecnt;
        let mut raw = Vec::with_capacity(typecnt);
        for _ in 0..typecnt {
            let off = i32::from_be_bytes(b[p..p + 4].try_into().unwrap());
            raw.push((off, b[p + 5] as usize));
            p += 6;
        }
        let chars = &b[p..p + charcnt];
        let types = raw
            .into_iter()
            .map(|(off, ai)| {
                let end = chars[ai.min(chars.len())..]
                    .iter()
                    .position(|c| *c == 0)
                    .map(|e| ai + e)
                    .unwrap_or(chars.len());
                TType {
                    off,
                    abbr: String::from_utf8_lossy(&chars[ai.min(end)..end]).into_owned(),
                }
            })
            .collect();
        p = need;
        let footer = if tsize == 8 && b.get(p) == Some(&b'\n') {
            let rest = &b[p + 1..];
            let end = rest.iter().position(|c| *c == b'\n').unwrap_or(rest.len());
            std::str::from_utf8(&rest[..end])
                .ok()
                .and_then(PosixTz::parse)
        } else {
            None
        };
        Ok(TzData {
            trans,
            idx,
            types,
            footer,
        })
    }

    /// UTC offset (seconds east) and abbreviation in effect at `utc`.
    pub fn lookup(&self, utc: i64) -> (i32, String) {
        let pos = self.trans.partition_point(|t| *t <= utc);
        if pos == self.trans.len() {
            if let Some(f) = &self.footer {
                return f.lookup(utc);
            }
        }
        let ty = if pos == 0 {
            self.types.first()
        } else {
            self.types.get(self.idx[pos - 1] as usize)
        };
        ty.map(|t| (t.off, t.abbr.clone()))
            .unwrap_or((0, "UTC".into()))
    }
}

#[derive(Debug, Clone, Copy)]
enum RuleDate {
    /// Mm.w.d
    Month { m: u32, w: u32, d: u32 },
    /// Jn: 1..=365, Feb 29 never counted
    Julian1(u32),
    /// n: 0..=365, Feb 29 counted
    Julian0(u32),
}

#[derive(Debug, Clone)]
struct PosixTz {
    std_name: String,
    std_off: i32,
    dst: Option<(String, i32, RuleDate, i32, RuleDate, i32)>,
}

impl PosixTz {
    fn parse(s: &str) -> Option<PosixTz> {
        let b = s.as_bytes();
        let mut i = 0;
        let std_name = parse_name(b, &mut i)?;
        let std_off = -parse_offset(b, &mut i)?;
        if i >= b.len() {
            return Some(PosixTz {
                std_name,
                std_off,
                dst: None,
            });
        }
        let dst_name = parse_name(b, &mut i)?;
        let dst_off = if i < b.len() && b[i] != b',' {
            -parse_offset(b, &mut i)?
        } else {
            std_off + 3600
        };
        if i >= b.len() || b[i] != b',' {
            // No rules: assume US rules (POSIX default).
            let s = RuleDate::Month { m: 3, w: 2, d: 0 };
            let e = RuleDate::Month { m: 11, w: 1, d: 0 };
            return Some(PosixTz {
                std_name,
                std_off,
                dst: Some((dst_name, dst_off, s, 7200, e, 7200)),
            });
        }
        i += 1;
        let (start, st) = parse_rule(b, &mut i)?;
        if i >= b.len() || b[i] != b',' {
            return None;
        }
        i += 1;
        let (end, et) = parse_rule(b, &mut i)?;
        Some(PosixTz {
            std_name,
            std_off,
            dst: Some((dst_name, dst_off, start, st, end, et)),
        })
    }

    fn lookup(&self, utc: i64) -> (i32, String) {
        let Some((dst_name, dst_off, start, st, end, et)) = &self.dst else {
            return (self.std_off, self.std_name.clone());
        };
        let year = crate::time::civil_from_days((utc + self.std_off as i64).div_euclid(86400)).0;
        let start_utc = rule_day(*start, year) * 86400 + *st as i64 - self.std_off as i64;
        let end_utc = rule_day(*end, year) * 86400 + *et as i64 - *dst_off as i64;
        let in_dst = if start_utc < end_utc {
            utc >= start_utc && utc < end_utc
        } else {
            !(utc >= end_utc && utc < start_utc)
        };
        if in_dst {
            (*dst_off, dst_name.clone())
        } else {
            (self.std_off, self.std_name.clone())
        }
    }
}

fn parse_name(b: &[u8], i: &mut usize) -> Option<String> {
    let s = *i;
    if b.get(*i) == Some(&b'<') {
        let end = b[*i..].iter().position(|c| *c == b'>')? + *i;
        *i = end + 1;
        return Some(String::from_utf8_lossy(&b[s + 1..end]).into_owned());
    }
    while *i < b.len() && b[*i].is_ascii_alphabetic() {
        *i += 1;
    }
    (*i - s >= 3).then(|| String::from_utf8_lossy(&b[s..*i]).into_owned())
}

/// `[+-]hh[:mm[:ss]]` in seconds (POSIX sign: positive = west of UTC).
fn parse_offset(b: &[u8], i: &mut usize) -> Option<i32> {
    let mut sign = 1;
    if let Some(&c) = b.get(*i) {
        if c == b'+' || c == b'-' {
            sign = if c == b'-' { -1 } else { 1 };
            *i += 1;
        }
    }
    let mut parts = [0i32; 3];
    for (k, part) in parts.iter_mut().enumerate() {
        if k > 0 {
            if b.get(*i) != Some(&b':') {
                break;
            }
            *i += 1;
        }
        let s = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *part = *part * 10 + (b[*i] - b'0') as i32;
            *i += 1;
        }
        if *i == s {
            return None;
        }
    }
    Some(sign * (parts[0] * 3600 + parts[1] * 60 + parts[2]))
}

fn parse_num(b: &[u8], i: &mut usize) -> Option<u32> {
    let s = *i;
    let mut n = 0u32;
    while *i < b.len() && b[*i].is_ascii_digit() {
        n = n * 10 + (b[*i] - b'0') as u32;
        *i += 1;
    }
    (*i > s).then_some(n)
}

fn parse_rule(b: &[u8], i: &mut usize) -> Option<(RuleDate, i32)> {
    let date = match b.get(*i)? {
        b'M' => {
            *i += 1;
            let m = parse_num(b, i)?;
            (b.get(*i) == Some(&b'.')).then_some(())?;
            *i += 1;
            let w = parse_num(b, i)?;
            (b.get(*i) == Some(&b'.')).then_some(())?;
            *i += 1;
            let d = parse_num(b, i)?;
            RuleDate::Month { m, w, d }
        }
        b'J' => {
            *i += 1;
            RuleDate::Julian1(parse_num(b, i)?)
        }
        _ => RuleDate::Julian0(parse_num(b, i)?),
    };
    let time = if b.get(*i) == Some(&b'/') {
        *i += 1;
        parse_offset(b, i)?
    } else {
        7200
    };
    Some((date, time))
}

/// Day number (since epoch) on which a rule fires in `year`.
fn rule_day(r: RuleDate, year: i64) -> i64 {
    match r {
        RuleDate::Month { m, w, d } => {
            let first = days_from_civil(year, m, 1);
            let wd = weekday(first) as i64;
            let mut day = 1 + (d as i64 - wd).rem_euclid(7) + (w as i64 - 1) * 7;
            while day > days_in_month(year, m) as i64 {
                day -= 7;
            }
            first + day - 1
        }
        RuleDate::Julian1(n) => {
            let extra = if is_leap(year) && n >= 60 { 1 } else { 0 };
            days_from_civil(year, 1, 1) + n as i64 - 1 + extra
        }
        RuleDate::Julian0(n) => days_from_civil(year, 1, 1) + n as i64,
    }
}

/// A time zone that values can be displayed in.
#[derive(Clone)]
pub enum Zone {
    Fixed { off: i32, name: String },
    Tz { name: String, data: Arc<TzData> },
}

impl fmt::Debug for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Zone({})", self.name())
    }
}

impl PartialEq for Zone {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

impl Zone {
    pub fn utc() -> Zone {
        Zone::Fixed {
            off: 0,
            name: "UTC".into(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Zone::Fixed { name, .. } | Zone::Tz { name, .. } => name,
        }
    }

    pub fn offset_at(&self, utc: i64) -> (i32, String) {
        match self {
            Zone::Fixed { off, name } => (*off, name.clone()),
            Zone::Tz { data, .. } => {
                let (off, abbr) = data.lookup(utc);
                let abbr = if abbr.starts_with('+') || abbr.starts_with('-') {
                    format_offset(off)
                } else {
                    abbr
                };
                (off, abbr)
            }
        }
    }

    /// Converts a local wall-clock time (seconds) into UTC.
    pub fn to_utc(&self, local: i64) -> i64 {
        let (o1, _) = self.offset_at(local);
        let guess = local - o1 as i64;
        let (o2, _) = self.offset_at(guess);
        local - o2 as i64
    }
}

pub fn format_offset(off: i32) -> String {
    let sign = if off < 0 { '-' } else { '+' };
    let a = off.abs();
    if a % 3600 == 0 {
        format!("UTC{sign}{}", a / 3600)
    } else {
        format!("UTC{sign}{}:{:02}", a / 3600, (a % 3600) / 60)
    }
}

/// City and country names → IANA zone, with the standard offset in minutes
/// used when no zoneinfo database is available.
const PLACES: &[(&str, &str, i32)] = &[
    ("new york", "America/New_York", -300),
    ("nyc", "America/New_York", -300),
    ("boston", "America/New_York", -300),
    ("washington", "America/New_York", -300),
    ("miami", "America/New_York", -300),
    ("atlanta", "America/New_York", -300),
    ("toronto", "America/Toronto", -300),
    ("montreal", "America/Toronto", -300),
    ("detroit", "America/Detroit", -300),
    ("chicago", "America/Chicago", -360),
    ("dallas", "America/Chicago", -360),
    ("houston", "America/Chicago", -360),
    ("austin", "America/Chicago", -360),
    ("mexico city", "America/Mexico_City", -360),
    ("winnipeg", "America/Winnipeg", -360),
    ("denver", "America/Denver", -420),
    ("phoenix", "America/Phoenix", -420),
    ("calgary", "America/Edmonton", -420),
    ("los angeles", "America/Los_Angeles", -480),
    ("la", "America/Los_Angeles", -480),
    ("san francisco", "America/Los_Angeles", -480),
    ("sf", "America/Los_Angeles", -480),
    ("seattle", "America/Los_Angeles", -480),
    ("portland", "America/Los_Angeles", -480),
    ("las vegas", "America/Los_Angeles", -480),
    ("san diego", "America/Los_Angeles", -480),
    ("vancouver", "America/Vancouver", -480),
    ("anchorage", "America/Anchorage", -540),
    ("honolulu", "Pacific/Honolulu", -600),
    ("hawaii", "Pacific/Honolulu", -600),
    ("halifax", "America/Halifax", -240),
    ("st johns", "America/St_Johns", -210),
    ("sao paulo", "America/Sao_Paulo", -180),
    ("rio de janeiro", "America/Sao_Paulo", -180),
    ("brazil", "America/Sao_Paulo", -180),
    ("buenos aires", "America/Argentina/Buenos_Aires", -180),
    ("argentina", "America/Argentina/Buenos_Aires", -180),
    ("santiago", "America/Santiago", -240),
    ("lima", "America/Lima", -300),
    ("bogota", "America/Bogota", -300),
    ("caracas", "America/Caracas", -240),
    ("london", "Europe/London", 0),
    ("uk", "Europe/London", 0),
    ("england", "Europe/London", 0),
    ("edinburgh", "Europe/London", 0),
    ("dublin", "Europe/Dublin", 0),
    ("ireland", "Europe/Dublin", 0),
    ("lisbon", "Europe/Lisbon", 0),
    ("portugal", "Europe/Lisbon", 0),
    ("reykjavik", "Atlantic/Reykjavik", 0),
    ("iceland", "Atlantic/Reykjavik", 0),
    ("paris", "Europe/Paris", 60),
    ("france", "Europe/Paris", 60),
    ("berlin", "Europe/Berlin", 60),
    ("germany", "Europe/Berlin", 60),
    ("munich", "Europe/Berlin", 60),
    ("frankfurt", "Europe/Berlin", 60),
    ("hamburg", "Europe/Berlin", 60),
    ("madrid", "Europe/Madrid", 60),
    ("spain", "Europe/Madrid", 60),
    ("barcelona", "Europe/Madrid", 60),
    ("rome", "Europe/Rome", 60),
    ("italy", "Europe/Rome", 60),
    ("milan", "Europe/Rome", 60),
    ("amsterdam", "Europe/Amsterdam", 60),
    ("netherlands", "Europe/Amsterdam", 60),
    ("brussels", "Europe/Brussels", 60),
    ("belgium", "Europe/Brussels", 60),
    ("zurich", "Europe/Zurich", 60),
    ("geneva", "Europe/Zurich", 60),
    ("switzerland", "Europe/Zurich", 60),
    ("vienna", "Europe/Vienna", 60),
    ("austria", "Europe/Vienna", 60),
    ("prague", "Europe/Prague", 60),
    ("warsaw", "Europe/Warsaw", 60),
    ("poland", "Europe/Warsaw", 60),
    ("budapest", "Europe/Budapest", 60),
    ("stockholm", "Europe/Stockholm", 60),
    ("sweden", "Europe/Stockholm", 60),
    ("oslo", "Europe/Oslo", 60),
    ("norway", "Europe/Oslo", 60),
    ("copenhagen", "Europe/Copenhagen", 60),
    ("denmark", "Europe/Copenhagen", 60),
    ("belgrade", "Europe/Belgrade", 60),
    ("lagos", "Africa/Lagos", 60),
    ("nigeria", "Africa/Lagos", 60),
    ("athens", "Europe/Athens", 120),
    ("greece", "Europe/Athens", 120),
    ("helsinki", "Europe/Helsinki", 120),
    ("finland", "Europe/Helsinki", 120),
    ("kyiv", "Europe/Kyiv", 120),
    ("kiev", "Europe/Kyiv", 120),
    ("ukraine", "Europe/Kyiv", 120),
    ("bucharest", "Europe/Bucharest", 120),
    ("romania", "Europe/Bucharest", 120),
    ("sofia", "Europe/Sofia", 120),
    ("cairo", "Africa/Cairo", 120),
    ("egypt", "Africa/Cairo", 120),
    ("johannesburg", "Africa/Johannesburg", 120),
    ("cape town", "Africa/Johannesburg", 120),
    ("south africa", "Africa/Johannesburg", 120),
    ("jerusalem", "Asia/Jerusalem", 120),
    ("tel aviv", "Asia/Jerusalem", 120),
    ("israel", "Asia/Jerusalem", 120),
    ("istanbul", "Europe/Istanbul", 180),
    ("turkey", "Europe/Istanbul", 180),
    ("moscow", "Europe/Moscow", 180),
    ("russia", "Europe/Moscow", 180),
    ("nairobi", "Africa/Nairobi", 180),
    ("kenya", "Africa/Nairobi", 180),
    ("riyadh", "Asia/Riyadh", 180),
    ("saudi arabia", "Asia/Riyadh", 180),
    ("doha", "Asia/Qatar", 180),
    ("qatar", "Asia/Qatar", 180),
    ("tehran", "Asia/Tehran", 210),
    ("iran", "Asia/Tehran", 210),
    ("dubai", "Asia/Dubai", 240),
    ("abu dhabi", "Asia/Dubai", 240),
    ("uae", "Asia/Dubai", 240),
    ("baku", "Asia/Baku", 240),
    ("tbilisi", "Asia/Tbilisi", 240),
    ("kabul", "Asia/Kabul", 270),
    ("karachi", "Asia/Karachi", 300),
    ("pakistan", "Asia/Karachi", 300),
    ("tashkent", "Asia/Tashkent", 300),
    ("mumbai", "Asia/Kolkata", 330),
    ("delhi", "Asia/Kolkata", 330),
    ("new delhi", "Asia/Kolkata", 330),
    ("bangalore", "Asia/Kolkata", 330),
    ("bengaluru", "Asia/Kolkata", 330),
    ("chennai", "Asia/Kolkata", 330),
    ("kolkata", "Asia/Kolkata", 330),
    ("hyderabad", "Asia/Kolkata", 330),
    ("pune", "Asia/Kolkata", 330),
    ("india", "Asia/Kolkata", 330),
    ("colombo", "Asia/Colombo", 330),
    ("kathmandu", "Asia/Kathmandu", 345),
    ("nepal", "Asia/Kathmandu", 345),
    ("dhaka", "Asia/Dhaka", 360),
    ("bangladesh", "Asia/Dhaka", 360),
    ("almaty", "Asia/Almaty", 300),
    ("yangon", "Asia/Yangon", 390),
    ("bangkok", "Asia/Bangkok", 420),
    ("thailand", "Asia/Bangkok", 420),
    ("jakarta", "Asia/Jakarta", 420),
    ("hanoi", "Asia/Ho_Chi_Minh", 420),
    ("ho chi minh", "Asia/Ho_Chi_Minh", 420),
    ("vietnam", "Asia/Ho_Chi_Minh", 420),
    ("singapore", "Asia/Singapore", 480),
    ("kuala lumpur", "Asia/Kuala_Lumpur", 480),
    ("malaysia", "Asia/Kuala_Lumpur", 480),
    ("hong kong", "Asia/Hong_Kong", 480),
    ("beijing", "Asia/Shanghai", 480),
    ("shanghai", "Asia/Shanghai", 480),
    ("shenzhen", "Asia/Shanghai", 480),
    ("china", "Asia/Shanghai", 480),
    ("taipei", "Asia/Taipei", 480),
    ("taiwan", "Asia/Taipei", 480),
    ("manila", "Asia/Manila", 480),
    ("philippines", "Asia/Manila", 480),
    ("perth", "Australia/Perth", 480),
    ("tokyo", "Asia/Tokyo", 540),
    ("osaka", "Asia/Tokyo", 540),
    ("japan", "Asia/Tokyo", 540),
    ("seoul", "Asia/Seoul", 540),
    ("korea", "Asia/Seoul", 540),
    ("south korea", "Asia/Seoul", 540),
    ("adelaide", "Australia/Adelaide", 570),
    ("darwin", "Australia/Darwin", 570),
    ("sydney", "Australia/Sydney", 600),
    ("melbourne", "Australia/Melbourne", 600),
    ("canberra", "Australia/Sydney", 600),
    ("brisbane", "Australia/Brisbane", 600),
    ("auckland", "Pacific/Auckland", 720),
    ("wellington", "Pacific/Auckland", 720),
    ("new zealand", "Pacific/Auckland", 720),
    ("fiji", "Pacific/Fiji", 720),
];

/// Zone abbreviations (case-sensitive). `None` means a fixed UTC offset.
const ABBREVIATIONS: &[(&str, Option<&str>, i32)] = &[
    ("UTC", None, 0),
    ("GMT", None, 0),
    ("Z", None, 0),
    ("PST", Some("America/Los_Angeles"), -480),
    ("PDT", Some("America/Los_Angeles"), -480),
    ("PT", Some("America/Los_Angeles"), -480),
    ("MST", Some("America/Denver"), -420),
    ("MDT", Some("America/Denver"), -420),
    ("MT", Some("America/Denver"), -420),
    ("CST", Some("America/Chicago"), -360),
    ("CDT", Some("America/Chicago"), -360),
    ("CT", Some("America/Chicago"), -360),
    ("EST", Some("America/New_York"), -300),
    ("EDT", Some("America/New_York"), -300),
    ("ET", Some("America/New_York"), -300),
    ("AKST", Some("America/Anchorage"), -540),
    ("AKDT", Some("America/Anchorage"), -540),
    ("HST", Some("Pacific/Honolulu"), -600),
    ("AST", Some("America/Halifax"), -240),
    ("ADT", Some("America/Halifax"), -240),
    ("NST", Some("America/St_Johns"), -210),
    ("BRT", Some("America/Sao_Paulo"), -180),
    ("ART", Some("America/Argentina/Buenos_Aires"), -180),
    ("BST", Some("Europe/London"), 0),
    ("WET", Some("Europe/Lisbon"), 0),
    ("WEST", Some("Europe/Lisbon"), 0),
    ("CET", Some("Europe/Paris"), 60),
    ("CEST", Some("Europe/Paris"), 60),
    ("WAT", Some("Africa/Lagos"), 60),
    ("EET", Some("Europe/Athens"), 120),
    ("EEST", Some("Europe/Athens"), 120),
    ("SAST", Some("Africa/Johannesburg"), 120),
    ("MSK", Some("Europe/Moscow"), 180),
    ("EAT", Some("Africa/Nairobi"), 180),
    ("GST", Some("Asia/Dubai"), 240),
    ("PKT", Some("Asia/Karachi"), 300),
    ("IST", Some("Asia/Kolkata"), 330),
    ("ICT", Some("Asia/Bangkok"), 420),
    ("WIB", Some("Asia/Jakarta"), 420),
    ("SGT", Some("Asia/Singapore"), 480),
    ("HKT", Some("Asia/Hong_Kong"), 480),
    ("AWST", Some("Australia/Perth"), 480),
    ("JST", Some("Asia/Tokyo"), 540),
    ("KST", Some("Asia/Seoul"), 540),
    ("ACST", Some("Australia/Adelaide"), 570),
    ("ACDT", Some("Australia/Adelaide"), 570),
    ("AEST", Some("Australia/Sydney"), 600),
    ("AEDT", Some("Australia/Sydney"), 600),
    ("NZST", Some("Pacific/Auckland"), 720),
    ("NZDT", Some("Pacific/Auckland"), 720),
];

/// Longest place name in words.
pub fn max_place_words() -> usize {
    PLACES
        .iter()
        .map(|p| p.0.split(' ').count())
        .max()
        .unwrap_or(1)
}

pub struct ZoneDb {
    dirs: Vec<PathBuf>,
    cache: Mutex<HashMap<String, Option<Arc<TzData>>>>,
    pub local: Zone,
}

impl Default for ZoneDb {
    fn default() -> Self {
        Self::new()
    }
}

impl ZoneDb {
    pub fn new() -> ZoneDb {
        let mut dirs = Vec::new();
        if let Some(d) = std::env::var_os("TZDIR") {
            dirs.push(PathBuf::from(d));
        }
        for d in [
            "/usr/share/zoneinfo",
            "/var/db/timezone/zoneinfo",
            "/usr/lib/zoneinfo",
            "/usr/share/lib/zoneinfo",
        ] {
            dirs.push(PathBuf::from(d));
        }
        dirs.retain(|d| d.is_dir());
        let mut db = ZoneDb {
            dirs,
            cache: Mutex::new(HashMap::new()),
            local: Zone::utc(),
        };
        db.local = db.detect_local();
        db
    }

    /// A database that always uses fixed offsets (deterministic tests).
    pub fn fixed(local: Zone) -> ZoneDb {
        ZoneDb {
            dirs: Vec::new(),
            cache: Mutex::new(HashMap::new()),
            local,
        }
    }

    fn load_data(&self, iana: &str) -> Option<Arc<TzData>> {
        if iana.contains("..") {
            return None;
        }
        let mut cache = self.cache.lock().ok()?;
        if let Some(hit) = cache.get(iana) {
            return hit.clone();
        }
        let data = self
            .dirs
            .iter()
            .find_map(|d| std::fs::read(d.join(iana)).ok())
            .or_else(|| android_tzdata(iana))
            .and_then(|b| TzData::parse(&b).ok())
            .map(Arc::new);
        cache.insert(iana.to_string(), data.clone());
        data
    }

    /// Sets the local zone from an IANA name supplied by the host OS (iOS and
    /// Android do not expose `/etc/localtime`). Falls back to a fixed offset.
    pub fn set_local(&mut self, iana: &str, offset_secs: i32) {
        self.local = match self.load_data(iana) {
            Some(data) => Zone::Tz { name: iana.to_string(), data },
            None => Zone::Fixed { off: offset_secs, name: format_offset(offset_secs) },
        };
    }

    /// Loads an IANA zone, falling back to a fixed offset labelled
    /// `fallback_label`.
    pub fn zone(&self, iana: &str, fallback_min: i32, fallback_label: &str) -> Zone {
        match self.load_data(iana) {
            Some(data) => Zone::Tz {
                name: iana.to_string(),
                data,
            },
            None => Zone::Fixed {
                off: fallback_min * 60,
                name: fallback_label.to_string(),
            },
        }
    }

    /// Resolves a city, country, abbreviation or IANA name.
    pub fn resolve(&self, phrase: &str) -> Option<Zone> {
        if let Some((abbr, iana, off)) = ABBREVIATIONS.iter().find(|a| a.0 == phrase) {
            return Some(match iana {
                None => Zone::Fixed {
                    off: off * 60,
                    name: abbr.to_string(),
                },
                Some(iana) => self.zone(iana, *off, abbr),
            });
        }
        let lower = phrase.to_lowercase();
        if let Some((_, iana, off)) = PLACES.iter().find(|p| p.0 == lower) {
            return Some(self.zone(iana, *off, &format_offset(off * 60)));
        }
        if phrase.contains('/')
            && phrase
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/_-+".contains(c))
        {
            if let Some(data) = self.load_data(phrase) {
                return Some(Zone::Tz {
                    name: phrase.to_string(),
                    data,
                });
            }
        }
        None
    }

    fn detect_local(&self) -> Zone {
        if let Ok(tz) = std::env::var("TZ") {
            let tz = tz.trim_start_matches(':');
            if !tz.is_empty() {
                if Path::new(tz).is_absolute() {
                    if let Some(z) = std::fs::read(tz).ok().and_then(|b| TzData::parse(&b).ok()) {
                        return Zone::Tz {
                            name: tz.to_string(),
                            data: Arc::new(z),
                        };
                    }
                } else if let Some(data) = self.load_data(tz) {
                    return Zone::Tz {
                        name: tz.to_string(),
                        data,
                    };
                } else if let Some(p) = PosixTz::parse(tz) {
                    let data = TzData {
                        trans: vec![],
                        idx: vec![],
                        types: vec![],
                        footer: Some(p),
                    };
                    return Zone::Tz {
                        name: tz.to_string(),
                        data: Arc::new(data),
                    };
                }
            }
        }
        if let Ok(bytes) = std::fs::read("/etc/localtime") {
            if let Ok(data) = TzData::parse(&bytes) {
                let name = std::fs::read_link("/etc/localtime")
                    .ok()
                    .and_then(|p| {
                        let s = p.to_string_lossy().into_owned();
                        s.split_once("zoneinfo/").map(|(_, n)| n.to_string())
                    })
                    .unwrap_or_else(|| "Local".into());
                return Zone::Tz {
                    name,
                    data: Arc::new(data),
                };
            }
        }
        #[cfg(windows)]
        if let Some(z) = windows_local() {
            return z;
        }
        Zone::utc()
    }
}

/// Android packs every zone into one `tzdata` file: a 12-byte version
/// header, big-endian offsets (index, data, zone.tab), then 52-byte
/// index entries (40-byte name, start, length, unused) pointing at TZif data.
fn android_tzdata(iana: &str) -> Option<Vec<u8>> {
    if !cfg!(target_os = "android") && std::env::var_os("RECKON_TZDATA").is_none() {
        return None;
    }
    let candidates = [
        std::env::var("RECKON_TZDATA").unwrap_or_default(),
        "/apex/com.android.tzdata/etc/tz/tzdata".to_string(),
        "/system/usr/share/zoneinfo/tzdata".to_string(),
    ];
    let b = candidates.iter().filter(|p| !p.is_empty()).find_map(|p| std::fs::read(p).ok())?;
    if b.len() < 24 || !b.starts_with(b"tzdata") {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
    let (index, data) = (be(12), be(16));
    let mut at = index;
    while at + 52 <= data.min(b.len()) {
        let name_end = b[at..at + 40].iter().position(|c| *c == 0).unwrap_or(40);
        if &b[at..at + name_end] == iana.as_bytes() {
            let (start, len) = (data + be(at + 40), be(at + 44));
            return b.get(start..start + len).map(<[u8]>::to_vec);
        }
        at += 52;
    }
    None
}

#[cfg(windows)]
fn windows_local() -> Option<Zone> {
    #[repr(C)]
    struct SystemTime([u16; 8]);
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    extern "system" {
        fn GetTimeZoneInformation(tzi: *mut TimeZoneInformation) -> u32;
    }
    // SAFETY: the struct layout matches TIME_ZONE_INFORMATION and the
    // pointer is valid for the duration of the call.
    let mut tzi: TimeZoneInformation = unsafe { std::mem::zeroed() };
    let r = unsafe { GetTimeZoneInformation(&mut tzi) };
    if r == u32::MAX {
        return None;
    }
    let bias = tzi.bias
        + if r == 2 {
            tzi.daylight_bias
        } else {
            tzi.standard_bias
        };
    let off = -bias * 60;
    Some(Zone::Fixed {
        off,
        name: format_offset(off),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_rules() {
        let p = PosixTz::parse("PST8PDT,M3.2.0,M11.1.0").unwrap();
        // 2026-07-01 12:00 UTC is summer: PDT
        let summer = days_from_civil(2026, 7, 1) * 86400 + 43200;
        assert_eq!(p.lookup(summer), (-7 * 3600, "PDT".into()));
        let winter = days_from_civil(2026, 1, 15) * 86400;
        assert_eq!(p.lookup(winter), (-8 * 3600, "PST".into()));
        // Southern hemisphere
        let p = PosixTz::parse("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(p.lookup(summer).1, "AEST");
        assert_eq!(p.lookup(winter).1, "AEDT");
        let p = PosixTz::parse("<+0530>-5:30").unwrap();
        assert_eq!(p.lookup(0).0, 19800);
    }

    #[test]
    fn system_zoneinfo_if_present() {
        let db = ZoneDb::new();
        if let Some(Zone::Tz { data, .. }) = db.resolve("Tokyo") {
            assert_eq!(data.lookup(1_700_000_000).0, 9 * 3600);
        }
        if let Some(z @ Zone::Tz { .. }) = db.resolve("New York") {
            let summer = days_from_civil(2030, 7, 1) * 86400;
            assert_eq!(z.offset_at(summer).0, -4 * 3600);
        }
    }
}
