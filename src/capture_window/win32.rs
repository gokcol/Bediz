use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    ffi::c_void,
};
use windows::{
    Win32::{
        Foundation::{HWND, POINT},
        Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute},
        UI::WindowsAndMessaging::{
            FindWindowW, GetCursorPos, GetWindowThreadProcessId, MSG, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_RBUTTONDOWN, WM_RBUTTONUP,
        },
    },
    core::{BOOL, PCWSTR},
};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton},
    window::WindowId,
};

#[derive(Clone, Copy)]
struct ButtonPosition {
    window: WindowId,
    button: MouseButton,
    state: ElementState,
    position: PhysicalPosition<f64>,
}

thread_local! {
    static OVERLAYS: RefCell<HashSet<WindowId>> = RefCell::new(HashSet::new());
    static QUEUED_BUTTON: Cell<Option<ButtonPosition>> = const { Cell::new(None) };
}

fn window_id(hwnd: HWND) -> WindowId {
    // The pinned winit Windows backend represents its WindowId by the HWND.
    WindowId::from(hwnd.0 as usize as u64)
}

pub(crate) fn prepare_overlay(name: &str) {
    let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: The title is NUL-terminated and remains live for the call. The
    // returned window is verified to belong to us before we change it.
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), PCWSTR(name.as_ptr())) }) else {
        return;
    };
    let mut pid = 0;
    // SAFETY: hwnd came from FindWindowW and pid is writable for this query.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != std::process::id() {
        return;
    }
    if OVERLAYS.with_borrow_mut(|overlays| overlays.insert(window_id(hwnd))) {
        let disabled = BOOL(1);
        // SAFETY: DWM reads a BOOL from a live pointer for this process's HWND.
        if let Err(error) = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                (&disabled as *const BOOL).cast(),
                std::mem::size_of::<BOOL>() as u32,
            )
        } {
            log::debug!("Could not disable the capture overlay transition: {error}");
        }
    }
}

pub(crate) fn is_overlay(window: WindowId) -> bool {
    OVERLAYS.with_borrow(|overlays| overlays.contains(&window))
}

pub(crate) fn forget_overlays() {
    OVERLAYS.with_borrow_mut(HashSet::clear);
    QUEUED_BUTTON.set(None);
}

/// Read a queued Win32 message in winit's Windows event-loop hook. The hook
/// always leaves native dispatch to winit, including mouse capture and focus.
///
/// # Safety
/// `message` must be the live MSG pointer provided by winit for this callback.
pub(crate) unsafe fn observe_message(message: *const c_void) -> bool {
    // SAFETY: This function is installed only as winit's with_msg_hook callback.
    let message = unsafe { &*message.cast::<MSG>() };
    let button = match message.message {
        WM_LBUTTONDOWN => Some((MouseButton::Left, ElementState::Pressed)),
        WM_LBUTTONUP => Some((MouseButton::Left, ElementState::Released)),
        WM_RBUTTONDOWN => Some((MouseButton::Right, ElementState::Pressed)),
        WM_RBUTTONUP => Some((MouseButton::Right, ElementState::Released)),
        _ => None,
    };
    QUEUED_BUTTON.set(button.and_then(|(button, state)| {
        let window = window_id(message.hwnd);
        is_overlay(window).then(|| ButtonPosition {
            window,
            button,
            state,
            // Button lParam stores signed, client-relative physical pixels.
            position: PhysicalPosition::new(
                f64::from(message.lParam.0 as i16),
                f64::from((message.lParam.0 >> 16) as i16),
            ),
        })
    }));
    false
}

pub(crate) fn button_position(
    window: WindowId,
    button: MouseButton,
    state: ElementState,
) -> Option<PhysicalPosition<f64>> {
    let position = QUEUED_BUTTON.get().filter(|position| {
        position.window == window && position.button == button && position.state == state
    })?;
    QUEUED_BUTTON.set(None);
    Some(position.position)
}

pub(crate) fn desktop_pointer() -> Option<eframe::egui::Pos2> {
    let mut point = POINT::default();
    // SAFETY: This read-only query writes to a live POINT.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some(eframe::egui::pos2(point.x as f32, point.y as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::LPARAM;

    #[test]
    fn only_the_matching_overlay_button_uses_the_native_click_position() {
        let hwnd = HWND(std::ptr::dangling_mut());
        let window = window_id(hwnd);
        let message = MSG {
            hwnd,
            message: WM_LBUTTONDOWN,
            lParam: LPARAM((410 << 16) | (-12_i16) as u16 as isize),
            ..Default::default()
        };
        // No Win32 calls are made by the hook. The HWND is only an opaque ID.
        assert!(!unsafe { observe_message((&message as *const MSG).cast()) });
        assert_eq!(
            button_position(window, MouseButton::Left, ElementState::Pressed),
            None
        );

        OVERLAYS.with_borrow_mut(|overlays| overlays.insert(window));
        assert!(!unsafe { observe_message((&message as *const MSG).cast()) });
        assert_eq!(
            button_position(window, MouseButton::Right, ElementState::Pressed),
            None
        );
        assert_eq!(
            button_position(window, MouseButton::Left, ElementState::Pressed),
            Some(PhysicalPosition::new(-12., 410.)),
        );
        assert_eq!(
            button_position(window, MouseButton::Left, ElementState::Pressed),
            None
        );
        forget_overlays();
    }
}
