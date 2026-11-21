//! Native capture surfaces must cover system chrome as well as normal windows.

fn title_prefix() -> String {
    format!("{} Capture — ", crate::branding::NAME)
}

pub fn title(display: &crate::capture::DisplayInfo) -> String {
    format!("{}{} [{}]", title_prefix(), display.name, display.id)
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, rc::Retained};
    use objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSColor, NSEvent, NSPanel, NSTrackingArea,
        NSTrackingAreaOptions, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
        NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::{NSArray, NSPointInRect};
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        time::{Duration, Instant},
    };

    const REVEAL_RETRY: Duration = Duration::from_millis(32);
    const REVEAL_TIMEOUT: Duration = Duration::from_millis(640);

    enum Reveal {
        Editor,
        Overlay { deadline: Instant },
    }

    thread_local! {
        static REVEALS: RefCell<BTreeMap<String, Reveal>> = const { RefCell::new(BTreeMap::new()) };
        static OVERLAY_HOSTS: RefCell<BTreeMap<String, OverlayHost>> = const { RefCell::new(BTreeMap::new()) };
    }

    struct OverlayHost {
        panel: Retained<NSPanel>,
        window: Retained<NSWindow>,
    }

    impl OverlayHost {
        fn new(window: Retained<NSWindow>) -> Self {
            // A nonactivating panel can join another app's full-screen Space.
            // Keep winit's NSWindow and content view intact for rendering and input.
            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                NSPanel::alloc(window.mtm()),
                window.frame(),
                NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                NSBackingStoreType::Buffered,
                false,
            );
            panel.setCollectionBehavior(window.collectionBehavior());
            panel.setLevel(window.level());
            panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
            panel.setHidesOnDeactivate(false);
            panel.setHasShadow(false);
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setIgnoresMouseEvents(true);
            // SAFETY: Both windows are retained, on the main thread, and distinct.
            // Drop detaches the child and closes the panel while we still own both.
            unsafe {
                panel.setReleasedWhenClosed(false);
                panel.addChildWindow_ordered(&window, NSWindowOrderingMode::Above);
            }
            Self { panel, window }
        }
    }

    impl Drop for OverlayHost {
        fn drop(&mut self) {
            self.window.orderOut(None);
            self.panel.close();
        }
    }

    fn windows(mut visit: impl FnMut(&Retained<NSWindow>)) {
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
        let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::CanJoinAllApplications
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle;
        let prefix = super::title_prefix();
        windows(|window| {
            if window.title().to_string().starts_with(&prefix) {
                window.setAnimationBehavior(NSWindowAnimationBehavior::None);
                // The capture belongs to the current desktop, which may be
                // another app's full-screen space or Stage Manager group.
                if window.collectionBehavior() != behavior {
                    // Initialize a new surface before the first native show.
                    // Temporary visibility changes later should keep its alpha.
                    if !window.isVisible() {
                        window.setAlphaValue(0.);
                    }
                    window.setCollectionBehavior(behavior);
                    if let Some(view) = window.contentView() {
                        // winit's tracking rect is for an active window. Send
                        // real hover events to the same view while the browser
                        // still owns focus, including when crossing displays.
                        let options = NSTrackingAreaOptions::ActiveAlways
                            | NSTrackingAreaOptions::InVisibleRect
                            | NSTrackingAreaOptions::MouseMoved
                            | NSTrackingAreaOptions::MouseEnteredAndExited;
                        if !view
                            .trackingAreas()
                            .iter()
                            .any(|area| area.options() == options)
                        {
                            // SAFETY: The content view implements the mouse
                            // tracking callbacks and owns the area for its lifetime.
                            let tracking = unsafe {
                                NSTrackingArea::initWithRect_options_owner_userInfo(
                                    NSTrackingArea::alloc(),
                                    view.bounds(),
                                    options,
                                    Some(&view),
                                    None,
                                )
                            };
                            view.addTrackingArea(&tracking);
                        }
                    }
                }
                if window.level() != level {
                    window.setLevel(level);
                }
            }
        });
    }

    pub fn reveal_after_paint(name: &str) {
        REVEALS.with(|pending| {
            pending.borrow_mut().entry(name.into()).or_insert_with(|| {
                if name == crate::branding::NAME {
                    Reveal::Editor
                } else {
                    Reveal::Overlay {
                        deadline: Instant::now() + REVEAL_TIMEOUT,
                    }
                }
            });
        });
    }

    pub fn forget_overlay_reveals() {
        REVEALS.with(|pending| {
            pending
                .borrow_mut()
                .retain(|_, reveal| matches!(reveal, Reveal::Editor));
        });
        let hosts = OVERLAY_HOSTS.with(|hosts| std::mem::take(&mut *hosts.borrow_mut()));
        drop(hosts);
    }

    pub fn after_paint() -> Option<Instant> {
        let mut requests = REVEALS.with(|pending| std::mem::take(&mut *pending.borrow_mut()));
        if requests.is_empty() {
            return None;
        }
        let now = Instant::now();
        windows(|window| {
            let name = window.title().to_string();
            match requests.get(&name) {
                Some(Reveal::Editor) => {
                    if window.isVisible() {
                        window.setAlphaValue(1.);
                        window.setAnimationBehavior(NSWindowAnimationBehavior::Default);
                    }
                }
                Some(Reveal::Overlay { .. }) => {
                    let host_was_ordered = OVERLAY_HOSTS.with(|hosts| {
                        let mut hosts = hosts.borrow_mut();
                        if hosts
                            .get(&name)
                            .is_some_and(|host| !std::ptr::eq(&*host.window, &**window))
                        {
                            hosts.remove(&name);
                        }
                        let host = hosts
                            .entry(name.clone())
                            .or_insert_with(|| OverlayHost::new(window.clone()));
                        let needs_order = !host.panel.isVisible() || !host.panel.isOnActiveSpace();
                        if needs_order {
                            host.panel.orderFrontRegardless();
                        }
                        needs_order
                    });
                    if host_was_ordered || !window.isVisible() || !window.isOnActiveSpace() {
                        // Paint once on the panel's Space before making a new
                        // surface opaque, even if ordering took effect immediately.
                        window.orderFrontRegardless();
                        return;
                    }
                    window.setAlphaValue(1.);
                    // If Bediz already owns the keyboard, keep Space/Enter on
                    // the display under the pointer. Otherwise leave the other
                    // application active; Escape uses our capture-only hotkey.
                    if NSApplication::sharedApplication(window.mtm()).isActive()
                        && NSPointInRect(NSEvent::mouseLocation(), window.frame())
                        && !window.isKeyWindow()
                    {
                        window.makeKeyWindow();
                    }
                }
                None => return,
            }
            requests.remove(&name);
        });
        requests.retain(|name, reveal| match reveal {
            Reveal::Editor => false,
            Reveal::Overlay { deadline } if *deadline <= now => {
                log::warn!("Capture window did not reach the active space: {name}");
                false
            }
            Reveal::Overlay { .. } => true,
        });
        let retry = (!requests.is_empty()).then_some(now + REVEAL_RETRY);
        REVEALS.with(|pending| pending.borrow_mut().append(&mut requests));
        retry
    }
}

pub fn desktop_pointer() -> Option<eframe::egui::Pos2> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::CGEvent;
        let event = CGEvent::new(None)?;
        let location = CGEvent::location(Some(&event));
        Some(eframe::egui::pos2(location.x as f32, location.y as f32))
    }
    #[cfg(not(target_os = "macos"))]
    None
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

pub fn forget_overlay_reveals() {
    #[cfg(target_os = "macos")]
    macos::forget_overlay_reveals();
}

pub fn after_paint() -> Option<std::time::Instant> {
    #[cfg(target_os = "macos")]
    return macos::after_paint();
    #[cfg(not(target_os = "macos"))]
    None
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

/// Keep our scrolling HUD out of screen samples. ScreenCaptureKit already
/// excludes this application; Windows needs per-window capture affinity.
pub fn exclude_scroll_controls() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::{
            Win32::{
                Foundation::{HWND, LPARAM},
                UI::WindowsAndMessaging::{
                    EnumWindows, GetWindowTextW, GetWindowThreadProcessId,
                    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
                },
            },
            core::BOOL,
        };
        unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> BOOL {
            // SAFETY: EnumWindows invokes this callback synchronously; `data`
            // points to the live local error slot below. Only our own windows
            // are changed, and GetWindowTextW bounds its write to the slice.
            unsafe {
                let mut pid = 0;
                GetWindowThreadProcessId(window, Some(&mut pid));
                if pid == std::process::id() {
                    let mut name = [0u16; 256];
                    let count = GetWindowTextW(window, &mut name).max(0) as usize;
                    if String::from_utf16_lossy(&name[..count])
                        .starts_with(&format!("{} Scroll — ", crate::branding::NAME))
                        && let Err(error) = SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE)
                    {
                        *(data.0 as *mut Option<String>) = Some(format!(
                            "Could not hide capture controls from the image: {error}"
                        ));
                    }
                }
            }
            BOOL(1)
        }
        let mut error: Option<String> = None;
        // SAFETY: Callback and user data stay valid for this synchronous call.
        unsafe {
            EnumWindows(
                Some(visit),
                LPARAM((&mut error as *mut Option<String>) as isize),
            )
        }
        .map_err(|e| e.to_string())?;
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(())
}
