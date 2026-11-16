//! Native capture surfaces must cover system chrome as well as normal windows.

pub fn title(display: &str) -> String {
    format!("{} Capture — {display}", crate::branding::NAME)
}

#[cfg(target_os = "macos")]
pub fn prepare() {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::NSWindow;
    use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
    use objc2_foundation::NSArray;

    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };
    // winit's AlwaysOnTop maps to the floating level, below the Dock and menu
    // bar. Use the actual Quartz overlay level for our temporary capture windows.
    // This runs during hidden placement, before any overlay is revealed.
    let level = CGWindowLevelForKey(CGWindowLevelKey::OverlayWindowLevelKey) as isize;
    // SAFETY: AppKit is initialized, accessed on its main thread, and this list
    // contains only windows owned by this application. Retain it while iterating.
    let windows: Retained<NSArray<NSWindow>> = unsafe {
        let app: *mut objc2::runtime::AnyObject =
            objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
        objc2::msg_send![app, windows]
    };
    for window in windows {
        if window.title().to_string().starts_with(&title("")) && window.level() != level {
            window.setLevel(level);
            log::debug!(
                "Prepared capture window above system chrome: {}",
                window.title()
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prepare() {}

/// Recover a mouse release that happened outside a capture viewport. egui-winit
/// drops mouse-button events when that viewport has no current pointer position.
pub fn primary_button_down() -> Option<bool> {
    #[cfg(target_os = "macos")]
    if objc2::MainThreadMarker::new().is_some() {
        // SAFETY: AppKit class method on the main thread; no event is generated.
        let buttons: usize =
            unsafe { objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons] };
        return Some(buttons & 1 != 0);
    }
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON},
            WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON},
        };
        // SAFETY: Read-only query; the high bit is the current physical state.
        Some(unsafe {
            let button = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
                VK_RBUTTON
            } else {
                VK_LBUTTON
            };
            GetAsyncKeyState(i32::from(button.0)) < 0
        })
    }
}
