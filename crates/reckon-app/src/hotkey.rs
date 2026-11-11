//! The global quick-calc shortcut, without extra crates:
//!
//! - macOS: Carbon's `RegisterEventHotKey` (no accessibility permission
//!   needed; events arrive on the main run loop GPUI already runs).
//! - Windows: `RegisterHotKey` on a small thread with its own message loop.
//! - Linux (and as a second way in on macOS): `reckon-app --quick` talks to
//!   the running app over a Unix socket in the data folder. Bind that command
//!   to a key in the desktop's keyboard settings; this works on Wayland, which
//!   has no global-shortcut API for apps.
//!
//! Every trigger sends `()` on one channel.

use std::sync::mpsc::Sender;

/// Default shortcut: Control-Option-Space on macOS, Ctrl+Alt+Space
/// elsewhere. Unlike ⌘Space / ⌥Space it does not clash with Spotlight,
/// Raycast, Alfred or the Windows window menu.
pub const DEFAULT: &str = "ctrl-alt-space";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// ⌘ on macOS, the Windows key elsewhere.
    pub cmd: bool,
    /// Lowercase letter, digit, or ' '.
    pub key: char,
}

/// Parses `"ctrl-alt-space"`, `"cmd-shift-k"`, … Needs at least one of
/// ctrl, alt or cmd.
pub fn parse(s: &str) -> Option<Shortcut> {
    let mut sc = Shortcut {
        ctrl: false,
        alt: false,
        shift: false,
        cmd: false,
        key: '\0',
    };
    for part in s.trim().to_lowercase().split(['-', '+']) {
        match part.trim() {
            "ctrl" | "control" => sc.ctrl = true,
            "alt" | "option" | "opt" => sc.alt = true,
            "shift" => sc.shift = true,
            "cmd" | "command" | "super" | "win" | "meta" => sc.cmd = true,
            "space" => sc.key = ' ',
            k if k.chars().count() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => {
                sc.key = k.chars().next()?
            }
            _ => return None,
        }
    }
    (sc.key != '\0' && (sc.ctrl || sc.alt || sc.cmd)).then_some(sc)
}

/// How the shortcut is written on this platform: "⌃⌥Space" or
/// "Ctrl+Alt+Space".
pub fn label(sc: &Shortcut) -> String {
    let key = if sc.key == ' ' {
        "Space".to_string()
    } else {
        sc.key.to_ascii_uppercase().to_string()
    };
    if cfg!(target_os = "macos") {
        let mut s = String::new();
        if sc.ctrl {
            s.push('⌃')
        }
        if sc.alt {
            s.push('⌥')
        }
        if sc.shift {
            s.push('⇧')
        }
        if sc.cmd {
            s.push('⌘')
        }
        s + &key
    } else {
        let mut parts = Vec::new();
        if sc.ctrl {
            parts.push("Ctrl")
        }
        if sc.alt {
            parts.push("Alt")
        }
        if sc.shift {
            parts.push("Shift")
        }
        if sc.cmd {
            parts.push("Win")
        }
        parts.push(&key);
        parts.join("+")
    }
}

/// Registers the system-wide shortcut. Returns an error message when the
/// system refuses it (usually because another app owns it).
pub fn register(sc: Shortcut, tx: Sender<()>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::register(sc, tx);
    #[cfg(windows)]
    return win::register(sc, tx);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (sc, tx);
        Err("global shortcuts are set in the desktop's keyboard settings (run `reckon-app --quick`)".into())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};

    use super::Shortcut;

    #[repr(C)]
    struct EventTypeSpec {
        class: u32,
        kind: u32,
    }

    #[repr(C)]
    struct EventHotKeyID {
        signature: u32,
        id: u32,
    }

    type Handler = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> *mut c_void;
        fn InstallEventHandler(
            target: *mut c_void,
            handler: Handler,
            count: u32,
            list: *const EventTypeSpec,
            user_data: *mut c_void,
            out: *mut *mut c_void,
        ) -> i32;
        fn RegisterEventHotKey(
            key_code: u32,
            modifiers: u32,
            id: EventHotKeyID,
            target: *mut c_void,
            options: u32,
            out: *mut *mut c_void,
        ) -> i32;
    }

    const K_EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
    const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
    const CMD: u32 = 1 << 8;
    const SHIFT: u32 = 1 << 9;
    const OPTION: u32 = 1 << 11;
    const CONTROL: u32 = 1 << 12;

    static SENDER: OnceLock<Mutex<Sender<()>>> = OnceLock::new();

    extern "C" fn on_hot_key(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 {
        if let Some(tx) = SENDER.get().and_then(|m| m.lock().ok()) {
            let _ = tx.send(());
        }
        0
    }

    /// Virtual key codes of the ANSI layout (they name physical keys).
    fn key_code(c: char) -> Option<u32> {
        const LETTERS: &[(char, u32)] = &[
            ('a', 0),
            ('s', 1),
            ('d', 2),
            ('f', 3),
            ('h', 4),
            ('g', 5),
            ('z', 6),
            ('x', 7),
            ('c', 8),
            ('v', 9),
            ('b', 11),
            ('q', 12),
            ('w', 13),
            ('e', 14),
            ('r', 15),
            ('y', 16),
            ('t', 17),
            ('1', 18),
            ('2', 19),
            ('3', 20),
            ('4', 21),
            ('6', 22),
            ('5', 23),
            ('9', 25),
            ('7', 26),
            ('8', 28),
            ('0', 29),
            ('o', 31),
            ('u', 32),
            ('i', 34),
            ('p', 35),
            ('l', 37),
            ('j', 38),
            ('k', 40),
            ('n', 45),
            ('m', 46),
            (' ', 49),
        ];
        LETTERS.iter().find(|(k, _)| *k == c).map(|(_, v)| *v)
    }

    pub fn register(sc: Shortcut, tx: Sender<()>) -> Result<(), String> {
        let code = key_code(sc.key).ok_or("unsupported key")?;
        let mut mods = 0;
        if sc.cmd {
            mods |= CMD
        }
        if sc.shift {
            mods |= SHIFT
        }
        if sc.alt {
            mods |= OPTION
        }
        if sc.ctrl {
            mods |= CONTROL
        }
        if SENDER.set(Mutex::new(tx)).is_err() {
            return Err("already registered".into());
        }
        unsafe {
            let target = GetApplicationEventTarget();
            let spec = EventTypeSpec {
                class: K_EVENT_CLASS_KEYBOARD,
                kind: K_EVENT_HOT_KEY_PRESSED,
            };
            let mut handler_ref = std::ptr::null_mut();
            let err = InstallEventHandler(
                target,
                on_hot_key,
                1,
                &spec,
                std::ptr::null_mut(),
                &mut handler_ref,
            );
            if err != 0 {
                return Err(format!("InstallEventHandler failed ({err})"));
            }
            let id = EventHotKeyID {
                signature: u32::from_be_bytes(*b"RCKN"),
                id: 1,
            };
            let mut hot_key_ref = std::ptr::null_mut();
            let err = RegisterEventHotKey(code, mods, id, target, 0, &mut hot_key_ref);
            if err != 0 {
                return Err(format!("the shortcut is taken by another app ({err})"));
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
mod win {
    use std::sync::mpsc::{channel, Sender};

    use super::Shortcut;

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: isize, id: i32, modifiers: u32, vk: u32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    }

    const MOD_ALT: u32 = 0x1;
    const MOD_CONTROL: u32 = 0x2;
    const MOD_SHIFT: u32 = 0x4;
    const MOD_WIN: u32 = 0x8;
    const MOD_NOREPEAT: u32 = 0x4000;
    const WM_HOTKEY: u32 = 0x0312;

    pub fn register(sc: Shortcut, tx: Sender<()>) -> Result<(), String> {
        let vk = if sc.key == ' ' {
            0x20
        } else {
            sc.key.to_ascii_uppercase() as u32
        };
        let mut mods = MOD_NOREPEAT;
        if sc.alt {
            mods |= MOD_ALT
        }
        if sc.ctrl {
            mods |= MOD_CONTROL
        }
        if sc.shift {
            mods |= MOD_SHIFT
        }
        if sc.cmd {
            mods |= MOD_WIN
        }
        let (ok_tx, ok_rx) = channel();
        // The hot key belongs to the thread that registers it, and its
        // WM_HOTKEY messages arrive in that thread's queue.
        std::thread::Builder::new()
            .name("hotkey".into())
            .spawn(move || unsafe {
                let ok = RegisterHotKey(0, 1, mods, vk) != 0;
                let _ = ok_tx.send(ok);
                if !ok {
                    return;
                }
                let mut msg: Msg = std::mem::zeroed();
                while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                    if msg.message == WM_HOTKEY {
                        let _ = tx.send(());
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        match ok_rx.recv() {
            Ok(true) => Ok(()),
            _ => Err("the shortcut is taken by another app".into()),
        }
    }
}

/// `reckon-app --quick` support: the running app listens on a socket in the
/// data folder; a second process connects to ask for the quick box.
#[cfg(unix)]
pub mod socket {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;

    /// `quick.sock` in the data folder, or in the temp folder when that path
    /// is too long for a socket (about 104 bytes on macOS).
    fn path() -> Option<PathBuf> {
        let p = reckon_core::paths::data_dir()?.join("quick.sock");
        if p.as_os_str().len() < 100 {
            return Some(p);
        }
        let user = std::env::var("USER").unwrap_or_default();
        Some(std::env::temp_dir().join(format!("reckon-quick-{user}.sock")))
    }

    /// Asks a running app to show the quick box. False when none answers.
    pub fn ask_running_app() -> bool {
        path()
            .and_then(|p| UnixStream::connect(p).ok())
            .is_some_and(|mut s| s.write_all(b"quick\n").is_ok())
    }

    pub fn listen(tx: Sender<()>) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // A socket file left by a crashed run would make bind fail.
        let _ = std::fs::remove_file(&p);
        let Ok(listener) = UnixListener::bind(&p) else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("quick-socket".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    if conn.is_ok() && tx.send(()).is_err() {
                        break;
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let sc = parse(DEFAULT).unwrap();
        assert!(sc.ctrl && sc.alt && !sc.cmd && sc.key == ' ');
        assert_eq!(parse("Cmd+Shift+K").unwrap().key, 'k');
        assert!(parse("space").is_none()); // needs a modifier
        assert!(parse("ctrl-f13").is_none());
    }
}
