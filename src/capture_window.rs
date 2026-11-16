//! Native capture surfaces must cover system chrome as well as normal windows.

pub fn title(display: &str) -> String {
    format!("{} Capture — {display}", crate::branding::NAME)
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSWindow, NSWindowAnimationBehavior};
    use objc2_foundation::NSArray;
    use std::{cell::RefCell, collections::BTreeSet};

    thread_local! {
        static REVEALS: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
    }

    fn windows(mut visit: impl FnMut(&NSWindow)) {
        let Some(_main_thread) = MainThreadMarker::new() else {
            return;
        };
        // SAFETY: AppKit is initialized and accessed only on its main thread.
        // These retained windows belong to this application, not other apps.
        let windows: Retained<NSArray<NSWindow>> = unsafe {
            let app: *mut objc2::runtime::AnyObject =
                objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
            objc2::msg_send![app, windows]
        };
        for window in windows {
            visit(&window);
        }
    }

    pub fn hide_before_capture() {
        REVEALS.with(|pending| pending.borrow_mut().clear());
        let launcher = format!("{} Capture Now", crate::branding::NAME);
        windows(|window| {
            let name = window.title().to_string();
            if name == crate::branding::NAME || name == launcher {
                window.setAnimationBehavior(NSWindowAnimationBehavior::None);
                window.setAlphaValue(0.);
                // eframe applies Visible(false) after painting. Order out now,
                // before the pending-capture update returns an empty scene.
                window.orderOut(None);
            }
        });
    }

    pub fn prepare() {
        use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
        let level = CGWindowLevelForKey(CGWindowLevelKey::OverlayWindowLevelKey) as isize;
        windows(|window| {
            if window.title().to_string().starts_with(&super::title("")) {
                window.setAnimationBehavior(NSWindowAnimationBehavior::None);
                // A native show/unhide can expose a backing surface before its
                // first painted frame. Keep that surface transparent until the
                // runtime has finished painting and applied viewport commands.
                if !window.isVisible() {
                    window.setAlphaValue(0.);
                }
                if window.level() != level {
                    window.setLevel(level);
                }
            }
        });
    }

    pub fn reveal_after_paint(name: &str) {
        REVEALS.with(|pending| {
            pending.borrow_mut().insert(name.into());
        });
    }

    pub fn after_paint() {
        let names = REVEALS.with(|pending| std::mem::take(&mut *pending.borrow_mut()));
        if names.is_empty() {
            return;
        }
        windows(|window| {
            let name = window.title().to_string();
            if names.contains(&name) && window.isVisible() {
                window.setAlphaValue(1.);
                if name == crate::branding::NAME {
                    window.setAnimationBehavior(NSWindowAnimationBehavior::Default);
                }
            }
        });
    }
}

pub fn hide_before_capture() {
    #[cfg(target_os = "macos")]
    macos::hide_before_capture();
}

pub fn prepare() {
    #[cfg(target_os = "macos")]
    macos::prepare();
}

pub fn reveal_after_paint(name: &str) {
    #[cfg(target_os = "macos")]
    macos::reveal_after_paint(name);
    #[cfg(not(target_os = "macos"))]
    let _ = name;
}

pub fn after_paint() {
    #[cfg(target_os = "macos")]
    macos::after_paint();
}

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
