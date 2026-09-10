//! macOS platform layer: permissions, synthesized paste, and the Fn-key listener.
//!
//! Three things here need the **Accessibility** permission (System Settings →
//! Privacy & Security → Accessibility):
//!
//! - posting a synthetic Cmd+V into another app (auto-paste),
//! - the event tap that watches for the Fn / 🌐 key,
//! - reading the frontmost app's name.
//!
//! Nothing in this module prompts for a permission unless the caller asks for it.
//! `accessibility_granted()` is a silent check; `prompt_for_accessibility()` is the
//! one that shows the system dialog, and only the onboarding window calls it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

// Fn / 🌐 lives in the secondary-function bit of the event flags. CGEventFlags has no
// named constant for it, so use the NX_SECONDARYFNMASK value from IOKit's headers.
const NX_SECONDARYFNMASK: u64 = 0x0080_0000;

// Virtual keycode for "V" (Carbon kVK_ANSI_V), used to synthesize Cmd+V.
const KEYCODE_V: u16 = 0x09;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// True when this app already holds the Accessibility permission. Never prompts.
pub fn accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Ask macOS to show the "allow Accessibility" dialog.
///
/// The system shows the dialog at most once per app; afterwards the user has to
/// toggle it in System Settings, which is why the onboarding window also offers a
/// button that opens the pane directly.
pub fn prompt_for_accessibility() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

/// Microphone authorization, without prompting.
///
/// Mirrors `AVAuthorizationStatus`: 0 not determined, 1 restricted, 2 denied,
/// 3 authorized. Reported as a string so the frontend can show three states rather
/// than a bool that conflates "not asked yet" with "denied".
pub fn microphone_status() -> &'static str {
    use objc2::runtime::AnyClass;
    use objc2::msg_send;
    use objc2_foundation::NSString;

    let Some(class) = AnyClass::get(c"AVCaptureDevice") else {
        // AVFoundation missing is not something we can recover from; report unknown
        // rather than claiming the mic is denied.
        return "unknown";
    };
    let media_type = NSString::from_str("soun"); // AVMediaTypeAudio
    let status: isize =
        unsafe { msg_send![class, authorizationStatusForMediaType: &*media_type] };

    match status {
        0 => "not-determined",
        1 => "restricted",
        2 => "denied",
        3 => "granted",
        _ => "unknown",
    }
}

/// Ask for microphone access, and show the system prompt if it has not been asked yet.
///
/// The prompt has to come from AVFoundation rather than from opening the input stream.
/// `authorizationStatusForMediaType:` answers from a cache held for the life of the
/// process, and a grant that arrives through CoreAudio — which is what opening a cpal
/// stream uses — never invalidates that cache. The app then reads `not-determined`
/// until it restarts, even though the grant is recorded. Asking here refreshes the
/// cache when the user answers, so the next status read is correct.
///
/// Returns immediately. The answer arrives on a background queue, and callers read it
/// through `microphone_status`.
pub fn request_microphone_access() {
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::NSString;

    let Some(class) = AnyClass::get(c"AVCaptureDevice") else {
        log::warn!("AVCaptureDevice unavailable; cannot ask for microphone access");
        return;
    };
    let media_type = NSString::from_str("soun"); // AVMediaTypeAudio
    let handler = RcBlock::new(|granted: Bool| {
        log::info!("microphone prompt answered: granted={}", granted.as_bool());
    });
    unsafe {
        let _: () = msg_send![
            class,
            requestAccessForMediaType: &*media_type,
            completionHandler: &*handler,
        ];
    }
}

/// Name of the frontmost application, e.g. `Safari`.
pub fn frontmost_app() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        app.localizedName().map(|name| name.to_string())
    }
}

/// Post a synthetic Cmd+V to the frontmost app.
///
/// Requires Accessibility; without it macOS silently swallows the event, so check
/// [`accessibility_granted`] first and tell the user rather than failing quietly.
pub fn send_paste() -> Result<(), String> {
    if !accessibility_granted() {
        return Err("Accessibility permission is needed to paste into other apps".into());
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "could not create an event source".to_string())?;

    let key_down = CGEvent::new_keyboard_event(source.clone(), KEYCODE_V, true)
        .map_err(|_| "could not create the key-down event".to_string())?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::HID);

    let key_up = CGEvent::new_keyboard_event(source, KEYCODE_V, false)
        .map_err(|_| "could not create the key-up event".to_string())?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}

/// Watch for the Fn / 🌐 key and call `on_press` each time it goes down.
///
/// Fn cannot be a normal global shortcut: Carbon's `RegisterEventHotKey` (what
/// tauri-plugin-global-shortcut uses) takes a keycode plus Cmd/Opt/Ctrl/Shift, and Fn
/// is none of those — it only shows up as a modifier-flag change. So we watch
/// `FlagsChanged` on an event tap instead. The tap is passive (`ListenOnly`): it never
/// swallows the key, so the system's own Globe behavior still runs unless the user
/// turns it off in System Settings → Keyboard.
///
/// Returns an error when Accessibility has not been granted — starting a tap without
/// it fails, and a silent failure here reads to the user as "the hotkey is broken".
pub fn start_fn_listener<F>(on_press: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    if !accessibility_granted() {
        return Err("Accessibility permission is needed to watch for the Fn key".into());
    }

    std::thread::Builder::new()
        .name("voxable-fn-tap".into())
        .spawn(move || {
            // Fn down and Fn up both arrive as FlagsChanged; only fire on the
            // transition into "pressed" so one press is one toggle.
            let was_down = Arc::new(AtomicBool::new(false));

            let tap = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                vec![CGEventType::FlagsChanged],
                move |_proxy, _type, event| {
                    let down = event.get_flags().bits() & NX_SECONDARYFNMASK != 0;
                    if down && !was_down.swap(true, Ordering::SeqCst) {
                        on_press();
                    } else if !down {
                        was_down.store(false, Ordering::SeqCst);
                    }
                    None
                },
            );

            let Ok(tap) = tap else {
                log::error!("could not create the Fn event tap");
                return;
            };

            unsafe {
                let loop_source = match tap.mach_port.create_runloop_source(0) {
                    Ok(source) => source,
                    Err(_) => {
                        log::error!("could not create a run-loop source for the Fn tap");
                        return;
                    }
                };
                let current = core_foundation::runloop::CFRunLoop::get_current();
                current.add_source(&loop_source, core_foundation::runloop::kCFRunLoopCommonModes);
                tap.enable();
                log::info!("Fn key listener running");
                core_foundation::runloop::CFRunLoop::run_current();
            }
        })
        .map_err(|e| format!("could not start the Fn listener thread: {e}"))?;

    Ok(())
}

/// Open a System Settings privacy pane directly.
///
/// `pane` is the Apple preference anchor, e.g. `Privacy_Accessibility` or
/// `Privacy_Microphone`.
pub fn open_privacy_pane(pane: &str) -> Result<(), String> {
    // Keyboard is a settings extension, not a pane of Privacy & Security.
    let url = if pane == "keyboard" {
        "x-apple.systempreferences:com.apple.Keyboard-Settings.extension".to_string()
    } else {
        format!("x-apple.systempreferences:com.apple.preference.security?{pane}")
    };
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open System Settings: {e}"))
}
