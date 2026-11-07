//! C ABI over `reckon-core`, declared in `include/reckon.h`.
//!
//! Used by the iOS app (Swift, through a bridging header) and the Android
//! app (Java, through a small JNI shim in `android/jni`).

#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;

use reckon_core::{paths, Engine, Rates};

pub struct ReckonEngine(Engine);

unsafe fn input<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok()
    }
}

fn output(s: String) -> *mut c_char {
    CString::new(s.replace('\0', "")).map(CString::into_raw).unwrap_or(ptr::null_mut())
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
pub unsafe extern "C" fn reckon_set_local_zone(engine: *mut ReckonEngine, iana: *const c_char, offset_seconds: c_int) {
    if let (Some(e), Some(name)) = (engine.as_mut(), input(iana)) {
        e.0.set_local_zone(name, offset_seconds);
    }
}

#[no_mangle]
pub unsafe extern "C" fn reckon_evaluate(engine: *mut ReckonEngine, document: *const c_char) -> *mut c_char {
    let (Some(e), Some(doc)) = (engine.as_ref(), input(document)) else {
        return output(String::new());
    };
    let lines: Vec<String> = e.0.evaluate(doc).into_iter().map(|o| o.text.unwrap_or_default()).collect();
    output(lines.join("\n"))
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
    let Some(e) = engine.as_mut() else { return output("no engine".into()) };
    let Some(fiat) = input(fiat_json) else { return output("missing fiat rates".into()) };
    match Rates::from_responses(fiat, input(crypto_json)).and_then(|r| e.0.apply_rates(r)) {
        Ok(()) => ptr::null_mut(),
        Err(err) => output(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn reckon_rates_source(engine: *const ReckonEngine) -> *mut c_char {
    output(engine.as_ref().map(|e| e.0.rates().source.clone()).unwrap_or_default())
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
            let err = reckon_apply_rates(e, CString::new("{}").unwrap().as_ptr(), ptr::null());
            assert!(!err.is_null());
            reckon_string_free(err);
            reckon_engine_free(e);
        }
    }
}
