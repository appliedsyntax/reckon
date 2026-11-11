//! C ABI over `reckon-core`, declared in `include/reckon.h`.
//!
//! Used by the iOS app (Swift, through a bridging header) and the Android
//! app (Java, through a small JNI shim in `android/jni`).

#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;

use std::path::Path;

use reckon_core::{paths, sheets, themes, Engine, Hl, Rates};

pub struct ReckonEngine(Engine);

unsafe fn input<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok()
    }
}

fn output(s: String) -> *mut c_char {
    CString::new(s.replace('\0', ""))
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn reckon_engine_new(data_dir: *const c_char) -> *mut ReckonEngine {
    if let Some(dir) = input(data_dir) {
        paths::set_data_dir(dir.into());
    }
    Box::into_raw(Box::new(ReckonEngine(Engine::with_user_data())))
}

#[no_mangle]
pub unsafe extern "C" fn reckon_engine_free(engine: *mut ReckonEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

#[no_mangle]
pub unsafe extern "C" fn reckon_set_local_zone(
    engine: *mut ReckonEngine,
    iana: *const c_char,
    offset_seconds: c_int,
) {
    if let (Some(e), Some(name)) = (engine.as_mut(), input(iana)) {
        e.0.set_local_zone(name, offset_seconds);
    }
}

#[no_mangle]
pub unsafe extern "C" fn reckon_evaluate(
    engine: *mut ReckonEngine,
    document: *const c_char,
) -> *mut c_char {
    let (Some(e), Some(doc)) = (engine.as_ref(), input(document)) else {
        return output(String::new());
    };
    let lines: Vec<String> =
        e.0.evaluate(doc)
            .into_iter()
            .map(|o| o.text.unwrap_or_default())
            .collect();
    output(lines.join("\n"))
}

fn hl_name(hl: Hl) -> &'static str {
    match hl {
        Hl::Number => "number",
        Hl::Unit => "unit",
        Hl::Keyword => "keyword",
        Hl::Variable => "variable",
        Hl::Function => "function",
        Hl::Operator => "operator",
        Hl::Label => "label",
        Hl::Comment => "comment",
        Hl::Header => "header",
        Hl::Error => "error",
    }
}

/// UTF-16 length of `s`: iOS and Android index strings in UTF-16 units.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

#[no_mangle]
pub unsafe extern "C" fn reckon_evaluate_highlighted(
    engine: *mut ReckonEngine,
    document: *const c_char,
) -> *mut c_char {
    let (Some(e), Some(doc)) = (engine.as_ref(), input(document)) else {
        return output(String::new());
    };
    let mut out = Vec::new();
    for (line, o) in doc.split('\n').zip(e.0.evaluate(doc)) {
        // Results never contain tabs or newlines; strip them just in case.
        let result: String = o
            .text
            .unwrap_or_default()
            .chars()
            .filter(|c| *c != '\t' && *c != '\n')
            .collect();
        let spans: Vec<String> = o
            .spans
            .iter()
            .filter(|(r, _)| {
                r.start < r.end
                    && r.end <= line.len()
                    && line.is_char_boundary(r.start)
                    && line.is_char_boundary(r.end)
            })
            .map(|(r, hl)| {
                let start = utf16_len(&line[..r.start]);
                let len = utf16_len(&line[r.clone()]);
                format!("{start},{len},{}", hl_name(*hl))
            })
            .collect();
        out.push(format!("{result}\t{}", spans.join(";")));
    }
    output(out.join("\n"))
}

#[no_mangle]
pub extern "C" fn reckon_themes() -> *mut c_char {
    output(themes::themes_json())
}

#[no_mangle]
pub unsafe extern "C" fn reckon_sheets_list(dir: *const c_char) -> *mut c_char {
    output(input(dir).map_or_else(|| "[]".into(), |d| sheets::list_json(Path::new(d))))
}

#[no_mangle]
pub unsafe extern "C" fn reckon_sheets_prepare(
    dir: *const c_char,
    legacy: *const c_char,
    welcome: *const c_char,
) -> *mut c_char {
    let Some(d) = input(dir) else {
        return ptr::null_mut();
    };
    let legacy = input(legacy).map(Path::new);
    match sheets::prepare(Path::new(d), legacy, input(welcome).unwrap_or("")) {
        Ok(name) => output(name),
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn reckon_sheet_create(
    dir: *const c_char,
    text: *const c_char,
) -> *mut c_char {
    let (Some(d), Some(t)) = (input(dir), input(text)) else {
        return ptr::null_mut();
    };
    sheets::create(Path::new(d), t).map_or(ptr::null_mut(), output)
}

#[no_mangle]
pub unsafe extern "C" fn reckon_sheet_tidy(dir: *const c_char, file: *const c_char) -> *mut c_char {
    let (Some(d), Some(f)) = (input(dir), input(file)) else {
        return ptr::null_mut();
    };
    sheets::tidy(Path::new(d), f).map_or(ptr::null_mut(), output)
}

#[no_mangle]
pub unsafe extern "C" fn reckon_sheet_trash(
    dir: *const c_char,
    file: *const c_char,
) -> *mut c_char {
    let (Some(d), Some(f)) = (input(dir), input(file)) else {
        return ptr::null_mut();
    };
    sheets::trash(Path::new(d), f).map_or(ptr::null_mut(), |p| {
        output(p.to_string_lossy().into_owned())
    })
}

#[no_mangle]
pub unsafe extern "C" fn reckon_sheet_restore(
    dir: *const c_char,
    trashed: *const c_char,
) -> *mut c_char {
    let (Some(d), Some(t)) = (input(dir), input(trashed)) else {
        return ptr::null_mut();
    };
    sheets::restore(Path::new(d), Path::new(t)).map_or(ptr::null_mut(), output)
}

#[no_mangle]
pub unsafe extern "C" fn reckon_rates_stale(engine: *const ReckonEngine) -> c_int {
    engine.as_ref().map_or(1, |e| e.0.rates_stale() as c_int)
}

#[no_mangle]
pub extern "C" fn reckon_fiat_url() -> *mut c_char {
    output(Rates::FIAT_URL.to_string())
}

#[no_mangle]
pub extern "C" fn reckon_crypto_url() -> *mut c_char {
    output(Rates::crypto_url())
}

#[no_mangle]
pub unsafe extern "C" fn reckon_apply_rates(
    engine: *mut ReckonEngine,
    fiat_json: *const c_char,
    crypto_json: *const c_char,
) -> *mut c_char {
    let Some(e) = engine.as_mut() else {
        return output("no engine".into());
    };
    let Some(fiat) = input(fiat_json) else {
        return output("missing fiat rates".into());
    };
    match Rates::from_responses(fiat, input(crypto_json)).and_then(|r| e.0.apply_rates(r)) {
        Ok(()) => ptr::null_mut(),
        Err(err) => output(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn reckon_rates_source(engine: *const ReckonEngine) -> *mut c_char {
    output(
        engine
            .as_ref()
            .map(|e| e.0.rates().source.clone())
            .unwrap_or_default(),
    )
}

#[no_mangle]
pub unsafe extern "C" fn reckon_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        unsafe {
            std::env::set_var("RECKON_HOME", std::env::temp_dir().join("reckon-ffi-test"));
            let e = reckon_engine_new(ptr::null());
            let doc = CString::new("2 + 2\n\n# header\n$10 in USD").unwrap();
            let out = reckon_evaluate(e, doc.as_ptr());
            assert_eq!(CStr::from_ptr(out).to_str().unwrap(), "4\n\n\n$10");
            reckon_string_free(out);
            let doc = CString::new("# Trip\nx = 2 × 3\n€5 + 1").unwrap();
            let out = reckon_evaluate_highlighted(e, doc.as_ptr());
            let text = CStr::from_ptr(out).to_str().unwrap().to_string();
            reckon_string_free(out);
            let lines: Vec<&str> = text.split('\n').collect();
            assert_eq!(lines[0], "\t0,6,header");
            assert!(lines[1].starts_with("6\t0,1,variable;"), "{}", lines[1]);
            // "×" is 2 bytes in UTF-8 but one UTF-16 unit, so "3" is at 8
            // (byte 9). Likewise "€" (3 bytes) puts "5" at 1.
            assert!(
                lines[1].ends_with("6,1,operator;8,1,number"),
                "{}",
                lines[1]
            );
            assert!(lines[2].contains("1,1,number"), "{}", lines[2]);
            let json = reckon_themes();
            assert!(CStr::from_ptr(json)
                .to_str()
                .unwrap()
                .contains("\"name\":\"Paper\""));
            reckon_string_free(json);
            let err = reckon_apply_rates(e, CString::new("{}").unwrap().as_ptr(), ptr::null());
            assert!(!err.is_null());
            reckon_string_free(err);
            reckon_engine_free(e);
        }
    }
}
