//! The event loop stays alive when the editor is hidden. No polling timer is used.
use eframe::{NativeOptions, UserEvent, egui};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

pub fn run(background: bool) -> eframe::Result {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let hidden = Arc::new(AtomicBool::new(false));
    let app_hidden = hidden.clone();
    let external_wake = Arc::new(AtomicBool::new(false));
    let app_wake = external_wake.clone();
    let proxy = event_loop.create_proxy();
    let options = NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title(crate::branding::NAME)
            // eframe configures backbuffer alpha support once from the root,
            // including for child viewports. Capture windows need a clear
            // backing surface while their first Metal frame is being presented.
            // The native editor window is made opaque separately below.
            .with_transparent(cfg!(target_os = "macos"))
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../assets/branding/myimage-app-icon-256.png"
                ))
                .expect("bundled Bediz app icon must be a valid PNG"),
            )
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([980.0, 640.0]),
        // Keep alpha support in the shared WGPU painter, but create the root
        // NSWindow with an opaque background so its title bar cannot show the
        // desktop through it. This one-shot hook does not affect capture windows.
        #[cfg(target_os = "macos")]
        window_builder: Some(Box::new(|viewport| viewport.with_transparent(false))),
        ..Default::default()
    };
    let inner = eframe::create_native(
        "MyImage",
        options,
        Box::new(move |cc| {
            install_root_waker(&cc.egui_ctx, app_wake, move |cumulative_pass_nr| {
                let _ = proxy.send_event(UserEvent::RequestRepaint {
                    viewport_id: egui::ViewportId::ROOT,
                    when: std::time::Instant::now(),
                    cumulative_pass_nr,
                });
            });
            Ok(Box::new(crate::app::MyImageApp::new(
                cc, background, app_hidden,
            )))
        }),
        &event_loop,
    );
    event_loop.run_app(&mut ResidentRuntime {
        inner,
        hidden,
        root: RootWindow::default(),
        wakeup: RootWakeup::default(),
        external_wake,
        modifiers: winit::keyboard::ModifiersState::empty(),
    })?;
    Ok(())
}

// Command delivery must bypass egui's repaint coalescing. A previous redraw can
// remain outstanding when macOS suppresses drawing for an inactive window.
#[derive(Clone)]
struct CommandWaker {
    pending: Arc<AtomicBool>,
    notify: Arc<dyn Fn(u64) + Send + Sync>,
}

pub(crate) fn install_root_waker(
    ctx: &egui::Context,
    pending: Arc<AtomicBool>,
    notify: impl Fn(u64) + Send + Sync + 'static,
) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("root-command-waker"),
            CommandWaker {
                pending,
                notify: Arc::new(notify),
            },
        );
    });
}

pub fn wake_root(ctx: &egui::Context) {
    let waker = ctx.data(|data| data.get_temp::<CommandWaker>(egui::Id::new("root-command-waker")));
    if let Some(waker) = waker {
        waker.pending.store(true, Ordering::Release);
        (waker.notify)(ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT));
    } else {
        // Headless tests and independently constructed contexts have no native loop.
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

struct ResidentRuntime<'a> {
    inner: eframe::EframeWinitApplication<'a>,
    hidden: Arc<AtomicBool>,
    root: RootWindow,
    wakeup: RootWakeup,
    external_wake: Arc<AtomicBool>,
    modifiers: winit::keyboard::ModifiersState,
}

impl ApplicationHandler<UserEvent> for ResidentRuntime<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.root.observe(id, &event);
        if let WindowEvent::ModifiersChanged(modifiers) = &event {
            self.modifiers = modifiers.state();
        }
        if let WindowEvent::KeyboardInput { event, .. } = &event
            && event.state.is_pressed()
            && !event.repeat
            && (if cfg!(target_os = "macos") {
                self.modifiers.super_key() || self.modifiers.control_key()
            } else {
                self.modifiers.control_key()
            })
            && !self.modifiers.alt_key()
            && !self.modifiers.shift_key()
            && matches!(&event.logical_key,winit::keyboard::Key::Character(s) if s.eq_ignore_ascii_case("v"))
        {
            crate::image_clipboard::request_paste();
        }
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        self.inner.window_event(event_loop, id, event);
        if redraw {
            // Visibility commands are applied by eframe after painting. Reveal
            // capture backing surfaces only after that entire operation returns.
            crate::capture_window::after_paint();
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.wakeup.schedule(
            &event,
            self.hidden.load(Ordering::Relaxed) || self.root.occluded,
        );
        self.inner.user_event(event_loop, event);
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.inner.device_event(event_loop, id, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
        // Invisible windows may suppress redraw notifications. Coalesce explicit
        // wakeups until native input has been drained, then render once. Rendering
        // inside user_event can feed child repaint requests back into the same
        // queue indefinitely, starving mouse-up, keyboard and occlusion events.
        let redraw = self
            .wakeup
            .take(self.hidden.load(Ordering::Relaxed) || self.root.occluded);
        // External commands get one root update even if native occlusion state
        // is stale. Drain only here, after native input, never recursively from
        // user_event (immediate child viewports may enqueue more repaints).
        if let Some(id) = self.root.id
            && (self.external_wake.swap(false, Ordering::AcqRel) || redraw)
        {
            self.inner
                .window_event(event_loop, id, WindowEvent::RedrawRequested);
            crate::capture_window::after_paint();
        }

        // Painting can queue the next step of a capture after native events
        // have been drained. Run another event-loop turn for that work.
        if self.root.id.is_some() && self.external_wake.load(Ordering::Acquire) {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if self.hidden.load(Ordering::Relaxed) || self.root.occluded {
            event_loop.set_control_flow(self.wakeup.control_flow(event_loop.control_flow()));
        }
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.exiting(event_loop);
    }
    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.memory_warning(event_loop);
    }
}

#[derive(Default)]
struct RootWakeup {
    deadline: Option<std::time::Instant>,
}
impl RootWakeup {
    fn schedule(&mut self, event: &UserEvent, dormant: bool) {
        if dormant && let UserEvent::RequestRepaint { when, .. } = event {
            self.deadline = Some(self.deadline.map_or(*when, |old| old.min(*when)));
        }
    }
    fn take(&mut self, dormant: bool) -> bool {
        self.take_at(dormant, std::time::Instant::now())
    }
    fn take_at(&mut self, dormant: bool, now: std::time::Instant) -> bool {
        if !dormant {
            self.deadline = None;
            return false;
        }
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
    fn control_flow(&self, current: ControlFlow) -> ControlFlow {
        match (current, self.deadline) {
            (ControlFlow::WaitUntil(native), Some(deadline)) => {
                ControlFlow::WaitUntil(native.min(deadline))
            }
            (_, Some(deadline)) => ControlFlow::WaitUntil(deadline),
            (ControlFlow::Poll, None) => ControlFlow::Wait,
            (other, None) => other,
        }
    }
}

// Eframe creates the root before immediate viewports. Keep its identity even
// when Capture Now is focused, hidden, or destroyed; child repaints cannot run
// the app's desktop-event queue while the root is invisible.
#[derive(Default)]
struct RootWindow {
    id: Option<WindowId>,
    occluded: bool,
}
impl RootWindow {
    fn observe(&mut self, id: WindowId, event: &WindowEvent) {
        self.id.get_or_insert(id);
        if self.id == Some(id)
            && let WindowEvent::Occluded(value) = event
        {
            self.occluded = *value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_wake_even_when_egui_has_an_outstanding_repaint() {
        use std::sync::atomic::AtomicUsize;
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let callbacks = Arc::new(AtomicUsize::new(0));
        let count = callbacks.clone();
        ctx.set_request_repaint_callback(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
        });
        ctx.request_repaint();
        let before = callbacks.load(Ordering::Relaxed);
        // A second request alone does not notify the native loop again.
        ctx.request_repaint();
        assert_eq!(callbacks.load(Ordering::Relaxed), before);
        let pending = Arc::new(AtomicBool::new(false));
        let notifications = Arc::new(AtomicUsize::new(0));
        let count = notifications.clone();
        install_root_waker(&ctx, pending.clone(), move |_| {
            count.fetch_add(1, Ordering::Relaxed);
        });
        // Hotkey delivery and a worker result must both escape that suppression.
        wake_root(&ctx);
        wake_root(&ctx);
        assert_eq!(notifications.load(Ordering::Relaxed), 2);
        assert!(pending.swap(false, Ordering::AcqRel));
        assert!(!pending.swap(false, Ordering::AcqRel));
        // No continuous repaint loop: only another explicit command re-arms it.
        wake_root(&ctx);
        assert!(pending.swap(false, Ordering::AcqRel));
    }

    #[test]
    fn child_repaints_coalesce_until_native_input_can_run() {
        let mut wakeup = RootWakeup::default();
        let child = egui::ViewportId::from_hash_of("second-screen");
        for viewport_id in [egui::ViewportId::ROOT, child, child] {
            wakeup.schedule(
                &UserEvent::RequestRepaint {
                    viewport_id,
                    when: std::time::Instant::now(),
                    cumulative_pass_nr: 0,
                },
                true,
            );
        }
        assert!(wakeup.take(true));
        assert!(!wakeup.take(true));
        wakeup.schedule(
            &UserEvent::RequestRepaint {
                viewport_id: child,
                when: std::time::Instant::now(),
                cumulative_pass_nr: 1,
            },
            true,
        );
        // A native visibility event can arrive before the coalesced wakeup.
        assert!(!wakeup.take(false));
        assert!(!wakeup.take(true));
        wakeup.schedule(
            &UserEvent::RequestRepaint {
                viewport_id: child,
                when: std::time::Instant::now() + std::time::Duration::from_secs(1),
                cumulative_pass_nr: 2,
            },
            true,
        );
        assert!(!wakeup.take(true));
    }

    #[test]
    fn delayed_capture_repaint_wakes_hidden_root_without_continuous_polling() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let later = now + Duration::from_millis(200);
        let mut wakeup = RootWakeup::default();
        wakeup.schedule(
            &UserEvent::RequestRepaint {
                viewport_id: egui::ViewportId::ROOT,
                when: later,
                cumulative_pass_nr: 1,
            },
            true,
        );
        assert!(!wakeup.take_at(true, now));
        assert_eq!(
            wakeup.control_flow(ControlFlow::Poll),
            ControlFlow::WaitUntil(later)
        );
        assert_eq!(
            wakeup.control_flow(ControlFlow::Wait),
            ControlFlow::WaitUntil(later)
        );
        let native = now + Duration::from_millis(50);
        assert_eq!(
            wakeup.control_flow(ControlFlow::WaitUntil(native)),
            ControlFlow::WaitUntil(native)
        );
        assert!(wakeup.take_at(true, later));
        assert!(!wakeup.take_at(true, later));
        assert_eq!(wakeup.control_flow(ControlFlow::Poll), ControlFlow::Wait);
        // Later child animation requests cannot postpone the capture deadline.
        for when in [later, later + Duration::from_secs(1)] {
            wakeup.schedule(
                &UserEvent::RequestRepaint {
                    viewport_id: egui::ViewportId::from_hash_of("scroll-preview"),
                    when,
                    cumulative_pass_nr: 2,
                },
                true,
            );
        }
        assert_eq!(
            wakeup.control_flow(ControlFlow::Wait),
            ControlFlow::WaitUntil(later)
        );
        assert!(!wakeup.take_at(false, later));
        assert_eq!(wakeup.control_flow(ControlFlow::Wait), ControlFlow::Wait);
    }

    #[test]
    fn launcher_events_never_replace_the_hidden_editor_wakeup_target() {
        let editor = WindowId::from(1);
        let launcher = WindowId::from(2);
        let mut root = RootWindow::default();
        root.observe(editor, &WindowEvent::Focused(true));
        root.observe(editor, &WindowEvent::Occluded(true));
        root.observe(launcher, &WindowEvent::Focused(true));
        root.observe(launcher, &WindowEvent::Occluded(false));
        root.observe(launcher, &WindowEvent::Destroyed);
        assert_eq!(root.id, Some(editor));
        assert!(root.occluded);
        root.observe(editor, &WindowEvent::Occluded(false));
        assert!(!root.occluded);
    }
}
