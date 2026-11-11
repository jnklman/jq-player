//! Labels of the character keys on the current keyboard layout, for the UI
//! where the webview cannot tell (`navigator.keyboard.getLayoutMap` only
//! exists in Chromium-based webviews). Keys are named by
//! `KeyboardEvent.code`; what is not known is left out, and the UI falls
//! back to the label stored with the key.

use std::collections::HashMap;

/// `KeyboardEvent.code` of the character keys → their PC scan code (set 1),
/// which on Linux is also the evdev key code.
const SCAN_CODES: [(&str, u16); 48] = [
    ("Backquote", 41),
    ("Digit1", 2),
    ("Digit2", 3),
    ("Digit3", 4),
    ("Digit4", 5),
    ("Digit5", 6),
    ("Digit6", 7),
    ("Digit7", 8),
    ("Digit8", 9),
    ("Digit9", 10),
    ("Digit0", 11),
    ("Minus", 12),
    ("Equal", 13),
    ("KeyQ", 16),
    ("KeyW", 17),
    ("KeyE", 18),
    ("KeyR", 19),
    ("KeyT", 20),
    ("KeyY", 21),
    ("KeyU", 22),
    ("KeyI", 23),
    ("KeyO", 24),
    ("KeyP", 25),
    ("BracketLeft", 26),
    ("BracketRight", 27),
    ("KeyA", 30),
    ("KeyS", 31),
    ("KeyD", 32),
    ("KeyF", 33),
    ("KeyG", 34),
    ("KeyH", 35),
    ("KeyJ", 36),
    ("KeyK", 37),
    ("KeyL", 38),
    ("Semicolon", 39),
    ("Quote", 40),
    ("Backslash", 43),
    ("KeyZ", 44),
    ("KeyX", 45),
    ("KeyC", 46),
    ("KeyV", 47),
    ("KeyB", 48),
    ("KeyN", 49),
    ("KeyM", 50),
    ("Comma", 51),
    ("Period", 52),
    ("Slash", 53),
    ("IntlBackslash", 86),
];

/// The unshifted character of each character key. Must run on the UI
/// (main) thread: GTK and the Windows keyboard layout are per thread.
pub fn labels() -> Result<HashMap<String, String>, String> {
    let mut labels = HashMap::new();
    for (code, scan) in SCAN_CODES {
        if let Some(text) = platform::label(scan)?.filter(|t| !t.trim().is_empty()) {
            labels.insert(code.to_owned(), text);
        }
    }
    Ok(labels)
}

#[cfg(target_os = "linux")]
mod platform {
    use gtk::gdk;

    /// Through GDK, which knows the layout on X11 and on Wayland. The first
    /// layout group is asked; GDK does not tell which one is active.
    pub fn label(scan: u16) -> Result<Option<String>, String> {
        let display = gdk::Display::default().ok_or_else(|| jq_i18n::t!("keylabels.no_display"))?;
        let keymap = gdk::Keymap::for_display(&display).ok_or_else(|| jq_i18n::t!("keylabels.no_layout"))?;
        // X and Wayland key codes are evdev codes plus 8.
        let Some((keyval, ..)) =
            keymap.translate_keyboard_state(u32::from(scan) + 8, gdk::ModifierType::empty(), 0)
        else {
            return Ok(None);
        };
        Ok(gdk::keys::Key::from(keyval).to_unicode().map(|c| c.to_string()))
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, ToUnicodeEx,
    };

    pub fn label(scan: u16) -> Result<Option<String>, String> {
        // SAFETY: plain Win32 calls with buffers that outlive them.
        unsafe {
            let layout = GetKeyboardLayout(0);
            let vk = MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, layout);
            if vk == 0 {
                return Ok(None);
            }
            let state = [0u8; 256];
            let mut text = [0u16; 8];
            // Flag 4: leave the keyboard state (dead keys) as it is.
            let n = ToUnicodeEx(
                vk,
                u32::from(scan),
                state.as_ptr(),
                text.as_mut_ptr(),
                text.len() as i32,
                4,
                layout,
            );
            // A dead key (n < 0) still writes its character.
            let len = if n < 0 { 1 } else { n as usize };
            Ok((len > 0).then(|| String::from_utf16_lossy(&text[..len.min(text.len())])))
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    pub fn label(_scan: u16) -> Result<Option<String>, String> {
        Err(jq_i18n::t!("keylabels.unavailable"))
    }
}
