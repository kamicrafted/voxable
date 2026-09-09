//! Win32 helpers (Windows only). Non-Windows targets get stubs.
//!
//! - `get_foreground_app` — title of the focused window (LLM context).
//! - `set_noactivate` — mark the Flow Bar `WS_EX_NOACTIVATE` so triggering it
//!   never steals focus from the user's text field (that's what lets dictation
//!   paste straight into the active app).
//! - `send_ctrl_v` — synthesize Ctrl+V into whatever window currently has focus.

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// Return the title of the foreground window, or `None` if unavailable.
#[cfg(windows)]
pub fn get_foreground_app() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let read = GetWindowTextW(hwnd, &mut buf);
        if read <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buf[..read as usize]);
        let title = title.trim();
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    }
}

/// Add `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` to a window so it never takes focus
/// (and stays out of Alt-Tab). `hwnd_ptr` is the raw HWND pointer as `isize`.
#[cfg(windows)]
pub fn set_noactivate(hwnd_ptr: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    unsafe {
        let hwnd = HWND(hwnd_ptr as *mut core::ffi::c_void);
        let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new = cur | (WS_EX_NOACTIVATE.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new);
    }
}

/// Synthesize a Ctrl+V keystroke into the currently focused window.
#[cfg(windows)]
pub fn send_ctrl_v() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_CONTROL, VK_V,
    };

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    let inputs = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];

    unsafe {
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent as usize != inputs.len() {
            return Err("SendInput did not deliver all keystrokes".into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Non-Windows stubs
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub fn get_foreground_app() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn set_noactivate(_hwnd_ptr: isize) {}

#[cfg(not(windows))]
pub fn send_ctrl_v() -> Result<(), String> {
    Err("paste-to-active is only implemented on Windows".into())
}
