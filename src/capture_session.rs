use super::*;
use crate::{capture::CaptureMode, scroll_capture::Stitcher};

const SELECTION_FADE_SECONDS: f64 = 0.16;

fn selection_shade_alpha(elapsed: f64) -> u8 {
    let t = (elapsed / SELECTION_FADE_SECONDS).clamp(0., 1.);
    // Smooth endpoints without delaying pointer input.
    (145. * t * t * (3. - 2. * t)).round() as u8
}

#[derive(Default)]
pub(super) struct RegionDrag {
    start: Option<Pos2>,
    end: Option<Pos2>,
    active: bool,
    completed: bool,
    pointer_left: bool,
    adjustment: Option<(Pos2, Rect, [bool; 4])>,
}
impl RegionDrag {
    fn recover_release(
        &self,
        events: &mut Vec<egui::Event>,
        primary_down: Option<bool>,
        modifiers: egui::Modifiers,
    ) {
        if !self.active
            || primary_down != Some(false)
            || events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::WindowFocused(false)
                        | egui::Event::PointerButton {
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            ..
                        }
                )
            })
        {
            return;
        }
        if let Some(pos) = self.end.or(self.start) {
            // Finish the previous drag before a possible new press, using the
            // last position it actually reached, not the re-entry position.
            events.insert(
                0,
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                },
            );
        }
    }

    /// A click snaps to the frontmost window; a drag remains a free region.
    fn update_smart_region(
        &mut self,
        events: &[egui::Event],
        bounds: Rect,
        controls: Rect,
        windows: &[(u32, Rect)],
    ) {
        for event in events {
            let snap = if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } = event
            {
                (self.active
                    && self.adjustment.is_none()
                    && self.start.is_some_and(|p| p.distance(*pos) < 3.))
                .then(|| {
                    windows
                        .iter()
                        .find(|(_, r)| r.contains(*pos))
                        .map(|(_, r)| r.intersect(bounds))
                })
                .flatten()
            } else {
                None
            };
            self.update(std::slice::from_ref(event), bounds, controls);
            if let Some(rect) = snap {
                self.start = Some(rect.min);
                self.end = Some(rect.max);
                self.completed = true;
            }
        }
    }
    fn update_window(
        &mut self,
        events: &[egui::Event],
        bounds: Rect,
        controls: Rect,
        windows: &[(u32, Rect)],
        selected: &mut Option<u32>,
    ) {
        for event in events {
            if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } = event
                && bounds.contains(*pos)
                && !controls.contains(*pos)
                && self.bounds().is_none_or(|r| !r.expand(7.).contains(*pos))
            {
                *selected = windows
                    .iter()
                    .find(|(_, r)| r.contains(*pos))
                    .map(|(id, _)| *id);
                *self = Self::default();
            }
            let Some((_, target)) = windows.iter().find(|(id, _)| Some(*id) == *selected) else {
                continue;
            };
            let target = target.intersect(bounds);
            let click = matches!(event, egui::Event::PointerButton {pos, button: egui::PointerButton::Primary, pressed: false, ..}
                if self.active && self.adjustment.is_none() && self.start.is_some_and(|a| a.distance(*pos) < 3.));
            self.update(std::slice::from_ref(event), target, controls);
            if click {
                self.start = Some(target.min);
                self.end = Some(target.max);
            }
        }
    }
    fn update(&mut self, events: &[egui::Event], bounds: Rect, controls: Rect) {
        for event in events {
            match *event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if bounds.contains(pos) && !controls.contains(pos) => {
                    self.adjustment = self.bounds().and_then(|rect| {
                        let edges = Self::edges(rect, pos);
                        (edges.iter().any(|v| *v) || rect.contains(pos))
                            .then_some((pos, rect, edges))
                    });
                    if self.adjustment.is_none() {
                        self.start = Some(bounds.clamp(pos));
                        self.end = None;
                    }
                    self.active = true;
                    self.completed = false;
                    self.pointer_left = false;
                }
                egui::Event::PointerMoved(pos) if self.active => self.move_to(pos, bounds),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } if self.active => {
                    self.move_to(pos, bounds);
                    self.active = false;
                    self.pointer_left = false;
                    self.adjustment = None;
                    self.completed = self.bounds().is_some();
                }
                // Leaving a viewport is not releasing the mouse. Keep an
                // in-progress drag so a transient native leave cannot truncate it.
                egui::Event::PointerGone if self.active => self.pointer_left = true,
                egui::Event::WindowFocused(false) => {
                    self.active = false;
                    self.pointer_left = false;
                    self.adjustment = None;
                }
                _ => {}
            }
        }
    }
    fn edges(rect: Rect, pos: Pos2) -> [bool; 4] {
        if !rect.expand(7.).contains(pos) {
            return [false; 4];
        }
        [
            (pos.x - rect.left()).abs() <= 7.,
            (pos.x - rect.right()).abs() <= 7.,
            (pos.y - rect.top()).abs() <= 7.,
            (pos.y - rect.bottom()).abs() <= 7.,
        ]
    }
    fn cursor(&self, pos: Pos2) -> egui::CursorIcon {
        let Some(rect) = self.bounds() else {
            return egui::CursorIcon::Crosshair;
        };
        let [l, r, t, b] = Self::edges(rect, pos);
        match (l || r, t || b) {
            (true, true) if (l && t) || (r && b) => egui::CursorIcon::ResizeNwSe,
            (true, true) => egui::CursorIcon::ResizeNeSw,
            (true, false) => egui::CursorIcon::ResizeHorizontal,
            (false, true) => egui::CursorIcon::ResizeVertical,
            _ if rect.contains(pos) => egui::CursorIcon::Move,
            _ => egui::CursorIcon::Crosshair,
        }
    }
    fn move_to(&mut self, pos: Pos2, bounds: Rect) {
        if let Some((origin, rect, [l, r, t, b])) = self.adjustment {
            let delta = pos - origin;
            let target = if l || r || t || b {
                Rect::from_min_max(
                    Pos2::new(
                        if l {
                            pos.x.clamp(bounds.left(), rect.right() - 1.)
                        } else {
                            rect.left()
                        },
                        if t {
                            pos.y.clamp(bounds.top(), rect.bottom() - 1.)
                        } else {
                            rect.top()
                        },
                    ),
                    Pos2::new(
                        if r {
                            pos.x.clamp(rect.left() + 1., bounds.right())
                        } else {
                            rect.right()
                        },
                        if b {
                            pos.y.clamp(rect.top() + 1., bounds.bottom())
                        } else {
                            rect.bottom()
                        },
                    ),
                )
            } else {
                rect.translate(Vec2::new(
                    delta
                        .x
                        .clamp(bounds.left() - rect.left(), bounds.right() - rect.right()),
                    delta
                        .y
                        .clamp(bounds.top() - rect.top(), bounds.bottom() - rect.bottom()),
                ))
            };
            self.start = Some(target.min);
            self.end = Some(target.max);
        } else {
            self.end = Some(bounds.clamp(pos));
        }
    }
    fn bounds(&self) -> Option<Rect> {
        self.start
            .zip(self.end)
            .map(|(a, b)| Rect::from_two_pos(a, b))
            .filter(|r| r.width() >= 1. && r.height() >= 1.)
    }
}

#[derive(Default)]
pub(super) struct MultiRegions {
    rects: Vec<Rect>,
    selected: Option<usize>,
}
impl MultiRegions {
    fn promote(&mut self, drag: &RegionDrag) {
        if let Some(rect) = drag.bounds() {
            self.rects.push(rect);
            self.selected = Some(self.rects.len() - 1);
        }
    }
    fn remove_selected(&mut self, drag: &mut RegionDrag) {
        if let Some(index) = self.selected.take() {
            self.rects.remove(index);
        }
        *drag = RegionDrag::default();
    }
    fn preview(&self, drag: &RegionDrag) -> Vec<Rect> {
        let mut rects: Vec<_> = self
            .rects
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if Some(i) == self.selected {
                    drag.bounds().unwrap_or(*r)
                } else {
                    *r
                }
            })
            .collect();
        if self.selected.is_none()
            && let Some(rect) = drag.bounds()
        {
            rects.push(rect);
        }
        rects
    }
    fn update(
        &mut self,
        drag: &mut RegionDrag,
        events: &[egui::Event],
        bounds: Rect,
        controls: Rect,
        windows: &[(u32, Rect)],
    ) {
        for event in events {
            if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
                ..
            } = event
                && bounds.contains(*pos)
                && !controls.contains(*pos)
            {
                let hit = (!modifiers.command)
                    .then(|| self.rects.iter().rposition(|r| r.expand(7.).contains(*pos)))
                    .flatten();
                if hit.is_none() && self.rects.len() >= 32 {
                    continue;
                }
                self.selected = hit;
                *drag = RegionDrag::default();
                if let Some(index) = hit {
                    drag.start = Some(self.rects[index].min);
                    drag.end = Some(self.rects[index].max);
                }
            }
            drag.completed = false;
            drag.update_smart_region(std::slice::from_ref(event), bounds, controls, windows);
            if drag.completed
                && let Some(rect) = drag.bounds()
            {
                if let Some(index) = self.selected {
                    self.rects[index] = rect;
                } else if self.rects.len() < 32 {
                    self.rects.push(rect);
                    self.selected = Some(self.rects.len() - 1);
                }
            }
        }
    }
}

fn capture_on_completion(enabled: bool, mode: CaptureMode, completed: bool) -> bool {
    enabled
        && completed
        && matches!(
            mode,
            CaptureMode::Region | CaptureMode::Window | CaptureMode::Text
        )
}

pub(super) struct CapturedFrame {
    pub display: DisplayInfo,
    pub image: image::RgbaImage,
    pub windows: Vec<capture::WindowInfo>,
}
pub(super) struct SelectionSession {
    pub screens: Vec<Selection>,
    pub return_hidden: bool,
}

const SELECTION_PLACEMENT_FRAMES: u8 = 4;

fn overlay_id(display: &DisplayInfo) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("capture-screen", display.id))
}

fn overlay_geometry(display: &DisplayInfo, desktop_units_per_point: f32) -> (Pos2, Vec2) {
    (
        Pos2::new(display.x as f32, display.y as f32) / desktop_units_per_point,
        Vec2::new(display.width as f32, display.height as f32) / desktop_units_per_point,
    )
}

fn display_pointer_fraction(pointer: Pos2, display: &DisplayInfo) -> Option<Vec2> {
    // Quartz uses desktop points and Windows uses physical desktop pixels.
    // Half-open edges assign a cursor on a display seam to exactly one overlay.
    let local = pointer - Pos2::new(display.x as f32, display.y as f32);
    let size = Vec2::new(display.width as f32, display.height as f32);
    ((0.0..size.x).contains(&local.x) && (0.0..size.y).contains(&local.y)).then(|| local / size)
}

fn pointer_in_overlay(pointer: Pos2, display: &DisplayInfo, bounds: Rect) -> Option<Pos2> {
    Some(bounds.min + display_pointer_fraction(pointer, display)? * bounds.size())
}

// Events can be coalesced while the hidden parent is being repainted. A leave
// clears the hover, but must not discard an earlier press from the same frame.
fn update_pointer_presence(ready: &mut bool, events: &[egui::Event]) -> bool {
    let mut had_position = false;
    for event in events {
        match event {
            egui::Event::PointerGone => *ready = false,
            egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. } => {
                *ready = true;
                had_position = true;
            }
            _ => {}
        }
    }
    had_position
}

impl SelectionSession {
    pub fn new(
        ctx: &egui::Context,
        frames: Vec<CapturedFrame>,
        captured_at: DateTime<Local>,
        mode: CaptureMode,
        return_hidden: bool,
    ) -> Self {
        Self {
            return_hidden,
            screens: frames
                .into_iter()
                .map(|frame| Selection {
                    texture: load_texture(ctx, "selection", &frame.image),
                    display: frame.display,
                    image: frame.image,
                    windows: frame.windows,
                    captured_at,
                    mode,
                    return_hidden,
                    placement_frames: 0,
                    reveal_time: None,
                    shade_started_at: None,
                    drag: Default::default(),
                    multi: Default::default(),
                    window_id: None,
                    hover_cycle: 0,
                    magnifier: true,
                    magnifier_preview: Default::default(),
                    pointer_ready: false,
                    #[cfg(target_os = "macos")]
                    webcam: Default::default(),
                    #[cfg(target_os = "macos")]
                    video_target: Default::default(),
                })
                .collect(),
        }
    }

    pub fn close(&self, ctx: &egui::Context) {
        crate::capture_window::forget_overlay_reveals();
        for screen in &self.screens {
            ctx.send_viewport_cmd_to(overlay_id(&screen.display), ViewportCommand::Visible(false));
            ctx.send_viewport_cmd_to(overlay_id(&screen.display), ViewportCommand::Close);
        }
        ctx.request_repaint();
    }

    pub fn focus(&self, ctx: &egui::Context) {
        for screen in &self.screens {
            if screen.reveal_time.is_some() {
                crate::capture_window::reveal_after_paint(&crate::capture_window::title(
                    &screen.display,
                ));
            }
        }
        if let Some(screen) = self.screens.first()
            && screen.placement_frames >= SELECTION_PLACEMENT_FRAMES
        {
            ctx.send_viewport_cmd_to(overlay_id(&screen.display), ViewportCommand::Focus);
        }
    }
}

#[derive(Debug, PartialEq)]
enum SelectionAction {
    Cancel,
    Window(WindowCaptureRequest),
    Regions(Vec<selection::PixelRect>),
    Region(selection::PixelRect),
    Scroll(selection::PixelRect),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WindowCaptureRequest {
    pub id: u32,
    pub crop: Option<[f32; 4]>,
    pub scrolling: bool,
}

pub(super) struct WindowScrollArea {
    pub crop: Option<[f32; 4]>,
    pub bounds: Rect,
    pub return_hidden: bool,
}

pub(super) struct ScrollSession {
    display: DisplayInfo,
    rect: selection::PixelRect,
    capture_size: (u32, u32),
    captured_at: DateTime<Local>,
    pub stitcher: Option<Stitcher>,
    pub message: String,
    pub texture: Option<egui::TextureHandle>,
    pub window_id: Option<u32>,
    window_crop: Option<selection::PixelRect>,
    bounds: Rect,
    return_hidden: bool,
    reset_requested: bool,
    pub paused: bool,
    pub finish_requested: bool,
    pub single_frame_warning: bool,
    pub next_capture: std::time::Instant,
    placement_frames: u8,
    demo_offset: u32,
}

impl MyImageApp {
    pub(super) fn selection_ui(&mut self, ctx: &egui::Context) {
        let Some(mut session) = self.selection.take() else {
            return;
        };
        let mut chosen = None;
        let mut needs_placement_frame = false;
        let native_reveal = cfg!(target_os = "macos") && !self.demo;
        if !self.demo {
            crate::capture_window::prepare();
        }
        let desktop_pointer = if !self.demo
            && session.screens.iter().any(|screen| {
                screen.placement_frames >= SELECTION_PLACEMENT_FRAMES && !screen.pointer_ready
            }) {
            crate::capture_window::desktop_pointer()
        } else {
            None
        };
        let focus_index = if cfg!(target_os = "windows") {
            desktop_pointer
                .and_then(|pointer| {
                    session.screens.iter().position(|screen| {
                        display_pointer_fraction(pointer, &screen.display).is_some()
                    })
                })
                .unwrap_or(0)
        } else {
            0
        };
        for (index, screen) in session.screens.iter_mut().enumerate() {
            // Mode changes in one controller apply to every screen.
            if screen.mode != self.settings.capture_mode {
                screen.mode = self.settings.capture_mode;
                screen.drag = RegionDrag::default();
                screen.multi = MultiRegions::default();
                screen.window_id = None;
                screen.hover_cycle = 0;
            }
            #[cfg(target_os = "macos")]
            if screen.mode == CaptureMode::Video
                && screen.video_target != self.settings.video.target
            {
                screen.video_target = self.settings.video.target;
                screen.drag = RegionDrag::default();
                screen.window_id = None;
            }
            let placing = screen.placement_frames < SELECTION_PLACEMENT_FRAMES;
            needs_placement_frame |= placing;
            let first_reveal = !placing && screen.reveal_time.is_none();
            if index == 0
                && first_reveal
                && let Some(desktop) = &self.desktop
            {
                // Cmd+H may have hidden the application as well as its editor.
                // Unhide without showing or resizing the editor window.
                desktop.reveal_application();
            }
            let divisor = ctx.zoom_factor()
                * if cfg!(target_os = "windows") {
                    screen.display.scale_factor
                } else {
                    1.
                };
            let (position, size) = overlay_geometry(&screen.display, divisor);
            let title = crate::capture_window::title(&screen.display);
            let builder = egui::ViewportBuilder::default()
                .with_title(&title)
                .with_position(position)
                .with_inner_size(size)
                .with_min_inner_size(Vec2::ZERO)
                .with_decorations(false)
                .with_resizable(false)
                .with_taskbar(false)
                .with_has_shadow(false)
                .with_transparent(cfg!(target_os = "macos"))
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_active(false)
                // winit shows an NSWindow with makeKeyAndOrderFront. Let the
                // native post-paint hook order it over the current Space instead.
                .with_visible(!placing && !native_reveal);
            // Every display has a dedicated surface. The resident runtime
            // drains child repaint/input requests while the editor stays hidden.
            ctx.show_viewport_immediate(overlay_id(&screen.display), builder, |child, _| {
                #[cfg(target_os = "windows")]
                if placing && !self.demo {
                    // Initialize the actual HWND while its backing frame is hidden.
                    crate::capture_window::win32::prepare_overlay(&title);
                }
                if let Some(action) = self.selection_overlay_ui(child, screen, desktop_pointer) {
                    chosen = Some((index, action));
                }
                if index == focus_index && first_reveal && !native_reveal {
                    child.send_viewport_cmd(ViewportCommand::Focus);
                }
                crate::pointer_style::paint(child, self.settings.pointer_style);
            });
            if chosen.is_some() {
                break;
            }
        }
        if let Some((index, action)) = chosen {
            session.close(ctx);
            let screen = session.screens.remove(index);
            match action {
                SelectionAction::Cancel => {
                    self.restore_window(ctx, session.return_hidden);
                    self.status = "Capture canceled".into();
                }
                SelectionAction::Window(request) => {
                    #[cfg(target_os = "macos")]
                    if screen.mode == CaptureMode::Video {
                        let mut screen = screen;
                        self.start_window_recording(ctx, &mut screen, request.id);
                        return;
                    }
                    self.capture_selected_window(ctx, screen, request)
                }
                SelectionAction::Regions(regions) => {
                    self.queue_capture_output(ctx, screen.captured_at, move || {
                        selection::crop_regions(&screen.image, &regions)
                    })
                }
                SelectionAction::Region(rect) => self.accept_selection(ctx, screen, rect),
                SelectionAction::Scroll(rect) => self.start_scroll_session(ctx, screen, rect),
            }
        } else {
            self.selection = Some(session);
            if needs_placement_frame {
                // Both editor and overlays can be hidden during placement.
                // Schedule the next root update even if macOS suppresses
                // native redraws or egui coalesces child repaint requests.
                crate::runtime::wake_root(ctx);
            }
        }
    }

    fn selection_overlay_ui(
        &mut self,
        ctx: &egui::Context,
        selection: &mut Selection,
        desktop_pointer: Option<Pos2>,
    ) -> Option<SelectionAction> {
        let menu_was_open = egui::Popup::is_any_open(ctx);
        if ctx.input(|i| i.viewport().close_requested())
            || ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            return Some(SelectionAction::Cancel);
        }
        let placing_overlay = selection.placement_frames < SELECTION_PLACEMENT_FRAMES;
        if !placing_overlay && selection.reveal_time.is_none() && !self.demo {
            crate::capture_window::reveal_after_paint(&crate::capture_window::title(
                &selection.display,
            ));
        }
        if placing_overlay {
            let divisor = if cfg!(target_os = "windows") {
                ctx.pixels_per_point()
            } else {
                ctx.zoom_factor()
            };
            let (position, size) = overlay_geometry(&selection.display, divisor);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
            selection.placement_frames += 1;
        }
        if !placing_overlay {
            selection
                .reveal_time
                .get_or_insert_with(|| ctx.input(|i| i.time));
        }
        let mut selection_events = ctx.input(|i| i.events.clone());
        // Hidden placement frames can still receive stale native positions.
        let had_position = !placing_overlay
            && update_pointer_presence(&mut selection.pointer_ready, &selection_events);
        if selection.drag.active && selection.drag.pointer_left && !self.demo {
            selection.drag.recover_release(
                &mut selection_events,
                crate::capture_window::primary_button_down(),
                ctx.input(|i| i.modifiers),
            );
        }
        let can_drag = selection.pointer_ready || had_position || selection.drag.active;
        let mut mode = selection.mode;
        let old_mode = mode;
        #[cfg(target_os = "macos")]
        let old_target = self.settings.video.target;
        if !menu_was_open
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
            && matches!(mode, CaptureMode::Region | CaptureMode::Window)
        {
            mode = if mode == CaptureMode::Window {
                CaptureMode::Region
            } else {
                CaptureMode::Window
            };
            selection.drag = RegionDrag::default();
            selection.window_id = None;
        }
        selection.drag.completed = false;
        let mut destination_changed = false;
        let mut accepted = None;
        let mut cancel = false;
        let mut scroll = false;
        let mut accepted_regions = Vec::new();
        let mut window = None;
        // Place the controller before the canvas so its hit region takes priority.
        let area = egui::Area::new(egui::Id::new((
            "capture-now-controls",
            selection.display.id,
        )))
        .order(egui::Order::Foreground);
        let area = if let Some(rect) = selection.drag.bounds().filter(|_| !selection.drag.active) {
            area.fixed_pos(controls_position(
                ctx.content_rect(),
                rect,
                ctx.data(|data| data.get_temp::<Vec2>(egui::Id::new("capture-toolbar-size")))
                    .unwrap_or(Vec2::new(470., 46.)),
            ))
        } else {
            area.anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0., -24.))
        };
        let bar = area
            .show(ctx, |ui| {
                ui.push_id(selection.display.id, |ui| {
                    let look = self.settings.capture_theme.look();
                    egui::Frame::new()
                        .fill(look.palette.paper)
                        .stroke(egui::Stroke::new(1., look.palette.border))
                        .corner_radius(10)
                        .inner_margin(8)
                        .shadow(egui::Shadow {
                            offset: [0, 6],
                            blur: 18,
                            spread: 0,
                            color: Color32::from_black_alpha(90),
                        })
                        .show(ui, |ui| {
                            crate::capture_panel::apply_look(ui, &look);
                            ui.spacing_mut().item_spacing.x = 5.;
                            ui.spacing_mut().interact_size.y = 30.;
                            ui.set_max_width((ctx.content_rect().width() - 48.).max(240.));
                            ui.horizontal_wrapped(|ui| {
                                if mode != CaptureMode::Video {
                                    let mode_menu = ui.menu_button("     ▾", |ui| {
                                        for choice in CaptureMode::ALL.into_iter().filter(|&choice| choice != CaptureMode::Video) {
                                            if ui.selectable_value(&mut mode, choice, choice.label()).clicked() {
                                                ui.close();
                                            }
                                        }
                                    }).response;
                                    mode_menu.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, format!("Capture mode: {}", mode.label())));
                                    crate::capture_panel::mode_icon(ui.painter(), mode,
                                        Rect::from_center_size(mode_menu.rect.left_center() + Vec2::new(16., 0.), Vec2::splat(18.)), look.palette.ink);
                                    mode_menu.on_hover_text(format!("{} · Change capture mode (Space: Region / Window)", mode.label()));
                                }
                                #[cfg(target_os = "macos")]
                                if mode == CaptureMode::Video {
                                    destination_changed |= self.settings.video.target.ui(ui);
                                    selection.webcam.menu(ui);
                                    let response = ui.menu_button("     ▾", |ui| {
                                        ui.set_min_width(286.);
                                        let region = match self.settings.video.target {
                                            video_session::VideoTarget::Window => selection.window_id.and_then(|id| video_session::window_size(selection, id)),
                                            video_session::VideoTarget::FullScreen => Some([selection.image.width(), selection.image.height()]),
                                        };
                                        destination_changed |= video_session::preferences_ui(
                                            ui, &mut self.settings.video, &self.settings.output_dir, region,
                                        );
                                    }).response;
                                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Recording options"));
                                    appearance::icon(ui.painter(), response.rect.left_center() + Vec2::new(16., 0.),
                                        appearance::Icon::Settings, look.palette.ink, 17.);
                                    response.on_hover_text("Recording quality, maximum duration, and disk space");
                                }
                                if mode.makes_image() {
                                    let previous = self.settings.capture_destination;
                                    let destination_menu = ui.menu_button("     ▾", |ui| {
                                        for choice in capture::CaptureDestination::ALL {
                                            if ui.selectable_value(&mut self.settings.capture_destination, choice, choice.label()).on_hover_text(choice.hint()).clicked() { ui.close(); }
                                        }
                                    }).response;
                                    let destination = self.settings.capture_destination;
                                    destination_menu.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, format!("Destination: {}", destination.label())));
                                    appearance::icon(ui.painter(), destination_menu.rect.left_center() + Vec2::new(16., 0.),
                                        if destination == capture::CaptureDestination::Editor { appearance::Icon::Edit } else { appearance::Icon::Copy }, look.palette.ink, 18.);
                                    destination_menu.on_hover_text(format!("To {} · {}", destination.label(), destination.hint()));
                                    destination_changed |= previous != destination;
                                    egui::ComboBox::from_id_salt("capture-output-size")
                                        .width(62.).selected_text(format!("{}%", self.settings.capture_output_size.percent()))
                                        .show_ui(ui, |ui| {
                                            for size in crate::capture_size::OutputSize::ALL {
                                                destination_changed |= ui.selectable_value(&mut self.settings.capture_output_size, size, size.label()).changed();
                                            }
                                        }).response.on_hover_text("Output size after capture or copy. Selection stays at full resolution.");
                                }
                                if capture_icon(ui, "Magnifier", appearance::Icon::Search, selection.magnifier).clicked() {
                                    selection.magnifier = !selection.magnifier;
                                }
                                if capture_icon(ui, "Reset selection", appearance::Icon::Refresh, false).clicked() {
                                    selection.drag = RegionDrag::default();
                                    selection.multi = MultiRegions::default();
                                    selection.window_id = None;
                                }
                                if capture_icon(ui, "Cancel capture · Esc", appearance::Icon::Close, false).clicked() { cancel = true; }
                                if mode == CaptureMode::MultiRegion {
                                    ui.label(format!("{}/32", selection.multi.rects.len()));
                                    if ui.add_enabled_ui(selection.multi.selected.is_some(), |ui| capture_icon(ui, "Remove region · Delete", appearance::Icon::Trash, false)).inner.clicked() {
                                        selection.multi.remove_selected(&mut selection.drag);
                                    }
                                }
                                if matches!(mode, CaptureMode::Region | CaptureMode::Window) {
                                    let response = ui.add_enabled_ui(selection.drag.bounds().is_some(), |ui| {
                                        let response = ui.add_sized([30., 30.], egui::Button::new(""));
                                        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Scrolling capture"));
                                        crate::capture_panel::mode_icon(ui.painter(), CaptureMode::Scrolling,
                                            Rect::from_center_size(response.rect.center(), Vec2::splat(18.)), ui.style().interact(&response).fg_stroke.color);
                                        response.on_hover_text("Scroll the selected area · Scroll slowly, then choose Done")
                                    }).inner;
                                    if response.clicked() { scroll = true; }
                                }
                                #[cfg(target_os = "macos")]
                                let full_video = mode == CaptureMode::Video && self.settings.video.target == video_session::VideoTarget::FullScreen;
                                #[cfg(not(target_os = "macos"))]
                                let full_video = false;
                                let ready = mode == CaptureMode::FullScreen
                                    || full_video
                                    || selection.drag.bounds().is_some()
                                    || (mode == CaptureMode::MultiRegion
                                        && !selection.multi.rects.is_empty());
                                if ui
                                    .add_enabled_ui(ready, |ui| {
                                        appearance::capture_button(
                                            ui,
                                            if mode == CaptureMode::Scrolling {
                                                "Start scrolling"
                                            } else if mode == CaptureMode::Text {
                                                "Copy text"
                                            } else if mode == CaptureMode::Video {
                                                "Record"
                                            } else {
                                                self.settings.capture_destination.capture_label()
                                            },
                                            Vec2::new(0., 30.),
                                        )
                                    })
                                    .inner
                                    .clicked()
                                {
                                    accepted = Some(selection::PixelRect {
                                        x: 0,
                                        y: 0,
                                        width: 0,
                                        height: 0,
                                    });
                                }
                            });
                        });
                });
            })
            .response
            .rect;
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("capture-toolbar-size"), bar.size()));
        if mode != old_mode {
            selection.drag = RegionDrag::default();
            selection.multi = MultiRegions::default();
            selection.mode = mode;
            selection.window_id = None;
            selection.hover_cycle = 0;
            self.settings.capture_mode = mode;
        }
        #[cfg(target_os = "macos")]
        let target_mode = if mode == CaptureMode::Video {
            selection.video_target = self.settings.video.target;
            if old_target != selection.video_target {
                selection.drag = RegionDrag::default();
                selection.window_id = None;
            }
            selection.video_target.mode()
        } else {
            mode
        };
        #[cfg(not(target_os = "macos"))]
        let target_mode = mode;
        let menu_interaction = menu_was_open || egui::Popup::is_any_open(ctx);
        let mut confirm = scroll
            || accepted.take().is_some()
            || (!menu_interaction && ctx.input(|i| i.key_pressed(egui::Key::Enter)));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let bounds = ui.max_rect();
                let response = ui.allocate_rect(bounds, egui::Sense::click_and_drag());
                let pointer = ctx.input(|i| i.pointer.hover_pos()).filter(|p| {
                    selection.pointer_ready && bounds.contains(*p)
                });
                let pointer = pointer.or_else(|| {
                    if placing_overlay || selection.pointer_ready {
                        return None;
                    }
                    pointer_in_overlay(desktop_pointer?, &selection.display, bounds)
                });
                // A stationary pointer can paint guides on the first frame.
                // Only actual viewport input above enables a click or a drag.
                let pointer = pointer.filter(|_| !menu_interaction);
                if response.hovered() && target_mode != CaptureMode::Window {
                    ctx.set_cursor_icon(
                        pointer.map_or(egui::CursorIcon::Crosshair, |p| {
                            if mode == CaptureMode::MultiRegion && let Some(rect) = selection.multi.rects.iter().rev().find(|r| r.expand(7.).contains(p)) {
                                RegionDrag { start: Some(rect.min), end: Some(rect.max), ..Default::default() }.cursor(p)
                            } else { selection.drag.cursor(p) }
                        }),
                    );
                }
                let window_rect = |w: &capture::WindowInfo| {
                    Rect::from_min_size(
                        bounds.min
                            + Vec2::new(
                                (w.x - selection.display.x) as f32 / selection.display.width as f32
                                    * bounds.width(),
                                (w.y - selection.display.y) as f32
                                    / selection.display.height as f32
                                    * bounds.height(),
                            ),
                        Vec2::new(
                            w.width as f32 / selection.display.width as f32 * bounds.width(),
                            w.height as f32 / selection.display.height as f32 * bounds.height(),
                        ),
                    )
                };
                #[cfg(target_os = "macos")]
                let video_mode = mode == CaptureMode::Video;
                #[cfg(target_os = "macos")]
                let video_frame = |id| {
                    video_mode.then(|| {
                        if target_mode == CaptureMode::FullScreen {
                            Some(bounds)
                        } else {
                            selection.windows.iter().find(|w| Some(w.id) == id).map(window_rect)
                        }
                    }).flatten()
                };
                #[cfg(target_os = "macos")]
                if !menu_interaction && let Some(rect) = video_frame(selection.window_id) {
                    selection.webcam.drag(ctx, rect, bar, &mut selection_events);
                }
                if ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::PointerMoved(_)))
                }) {
                    selection.hover_cycle = 0;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                    selection.hover_cycle = selection.hover_cycle.wrapping_add(1);
                }
                let candidates: Vec<_> = pointer
                    .filter(|p| !bar.contains(*p))
                    .map(|p| {
                        selection
                            .windows
                            .iter()
                            .filter(|w| window_rect(w).contains(p))
                            .collect()
                    })
                    .unwrap_or_default();
                let hovered_window = (!candidates.is_empty())
                    .then(|| candidates[selection.hover_cycle % candidates.len()]);
                if target_mode == CaptureMode::Window && response.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if !placing_overlay && !menu_interaction && selection.pointer_ready && mode == CaptureMode::Region
                    && ctx.input(|i| i.events.iter().any(|e| matches!(e,
                        egui::Event::PointerButton {pos, button: egui::PointerButton::Primary, pressed: true, modifiers, ..}
                            if modifiers.command && bounds.contains(*pos) && !bar.contains(*pos))))
                {
                    selection.multi.promote(&selection.drag);
                    mode = CaptureMode::MultiRegion;
                    selection.mode = mode;
                }
                if mode == CaptureMode::MultiRegion && !menu_interaction && !placing_overlay {
                    if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                        selection.multi.remove_selected(&mut selection.drag);
                    }
                    if can_drag {
                        let windows: Vec<_> = selection.windows.iter().map(|w| (w.id, window_rect(w))).collect();
                        selection.multi.update(&mut selection.drag, &selection_events, bounds, bar, &windows);
                    }
                } else if target_mode == CaptureMode::Window {
                    if !placing_overlay && selection.pointer_ready && !menu_interaction
                        && response.clicked()
                        && (mode != CaptureMode::Video || selection_events.iter().any(|event| matches!(event, egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: false, .. })))
                        && let Some(w) = hovered_window
                    {
                        let rect = window_rect(w).intersect(bounds);
                        selection.drag.start = Some(rect.min);
                        selection.drag.end = Some(rect.max);
                        selection.window_id = Some(w.id);
                        selection.drag.completed = true;
                    }
                } else if target_mode == CaptureMode::FullScreen {
                    confirm |= mode != CaptureMode::Video && !placing_overlay && !menu_interaction && selection.pointer_ready && response.clicked()
                        && pointer.is_some_and(|p| !bar.contains(p));
                } else if !placing_overlay && can_drag && !menu_interaction && mode == CaptureMode::Scrolling {
                    let windows: Vec<_> = selection
                        .windows
                        .iter()
                        .map(|w| (w.id, window_rect(w)))
                        .collect();
                    selection.drag.update_window(
                        &selection_events,
                        bounds,
                        bar,
                        &windows,
                        &mut selection.window_id,
                    );
                } else if !placing_overlay && can_drag && !menu_interaction && mode == CaptureMode::Text {
                    selection.drag.update(
                        &selection_events,
                        bounds,
                        bar,
                    );
                } else if !placing_overlay && can_drag && !menu_interaction && mode == CaptureMode::Region {
                    let windows: Vec<_> = hovered_window
                        .into_iter()
                        .chain(
                            selection
                                .windows
                                .iter()
                                .filter(|w| Some(w.id) != hovered_window.map(|w| w.id)),
                        )
                        .map(|w| (w.id, window_rect(w)))
                        .collect();
                    selection.drag.update_smart_region(
                        &selection_events,
                        bounds,
                        bar,
                        &windows,
                    );
                }
                confirm |= !placing_overlay
                    && !menu_interaction
                    && capture_on_completion(self.settings.capture_on_release, mode, selection.drag.completed);
                let chosen = if mode == CaptureMode::Video && target_mode == CaptureMode::FullScreen {
                    Some(bounds)
                } else {
                    selection.drag.bounds().map(|r| r.intersect(bounds))
                };
                // Enter capture at the desktop's original brightness. Hover
                // outlines must not darken the entire screen before the user
                // makes a selection. Ease in the outside shade only once a
                // real region exists, and reset it when the selection clears.
                let has_selection = chosen.is_some()
                    || (mode == CaptureMode::MultiRegion && !selection.multi.rects.is_empty());
                let shade_alpha = if !placing_overlay && has_selection {
                    let now = ctx.input(|i| i.time);
                    let start = *selection.shade_started_at.get_or_insert(now);
                    let elapsed = now - start;
                    if elapsed < SELECTION_FADE_SECONDS {
                        ctx.request_repaint();
                    }
                    selection_shade_alpha(elapsed)
                } else {
                    selection.shade_started_at = None;
                    0
                };
                let selected = (target_mode == CaptureMode::FullScreen).then_some(bounds).or(chosen).or_else(|| {
                    ((mode.makes_image() || target_mode == CaptureMode::Window) && !selection.drag.active)
                        .then(|| hovered_window.map(|w| window_rect(w).intersect(bounds)))
                        .flatten()
                });
                let painter = ui.painter();
                #[cfg(target_os = "macos")]
                if !self.demo {
                    selection.webcam.update_preview(
                        ctx, &selection.display,
                        (!placing_overlay).then(|| video_frame(selection.window_id)).flatten(),
                        menu_interaction,
                    );
                }
                painter.image(
                    selection.texture.id(),
                    bounds,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                    Color32::WHITE,
                );
                let shade = Color32::from_black_alpha(shade_alpha);
                if mode == CaptureMode::MultiRegion {
                    if has_selection {
                        painter.rect_filled(bounds, 0, shade);
                    }
                    for (index, rect) in selection.multi.preview(&selection.drag).into_iter().enumerate() {
                        let uv = Rect::from_min_max(
                            ((rect.min - bounds.min) / bounds.size()).to_pos2(),
                            ((rect.max - bounds.min) / bounds.size()).to_pos2());
                        painter.image(selection.texture.id(), rect, uv, Color32::WHITE);
                        painter.rect_stroke(rect, 0, Stroke::new(2., appearance::palette(ui.ctx()).accent), egui::StrokeKind::Inside);
                        let badge = Rect::from_min_size(rect.min + Vec2::splat(4.), Vec2::new(25.,22.));
                        painter.rect_filled(badge, 4, appearance::palette(ui.ctx()).navy);
                        painter.text(badge.center(), egui::Align2::CENTER_CENTER, (index+1).to_string(), egui::FontId::proportional(12.), Color32::WHITE);
                    }
                    if let Some(rect) = chosen {
                        for p in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom(), rect.center_top(), rect.center_bottom(), rect.left_center(), rect.right_center()] {
                            painter.circle_filled(p, 4., Color32::WHITE);
                            painter.circle_stroke(p, 4., Stroke::new(1., appearance::palette(ui.ctx()).accent));
                        }
                    }
                } else if let Some(rect) = selected {
                    for outside in [
                        Rect::from_min_max(bounds.min, Pos2::new(bounds.right(), rect.top())),
                        Rect::from_min_max(Pos2::new(bounds.left(), rect.bottom()), bounds.max),
                        Rect::from_min_max(
                            Pos2::new(bounds.left(), rect.top()),
                            rect.left_bottom(),
                        ),
                        Rect::from_min_max(
                            rect.right_top(),
                            Pos2::new(bounds.right(), rect.bottom()),
                        ),
                    ] {
                        painter.rect_filled(outside, 0, shade);
                    }
                    painter.rect_stroke(
                        rect,
                        0,
                        Stroke::new(2., Color32::from_rgb(85, 232, 212)),
                        egui::StrokeKind::Inside,
                    );
                    for p in (target_mode != CaptureMode::Window && target_mode != CaptureMode::FullScreen && chosen.is_some())
                        .then_some([
                            rect.left_top(),
                            rect.right_top(),
                            rect.left_bottom(),
                            rect.right_bottom(),
                            rect.center_top(),
                            rect.center_bottom(),
                            rect.left_center(),
                            rect.right_center(),
                        ])
                        .into_iter()
                        .flatten()
                    {
                        painter.circle_filled(p, 4., Color32::WHITE);
                        painter.circle_stroke(p, 4., Stroke::new(1., appearance::palette(ui.ctx()).accent));
                    }
                    #[cfg(target_os = "macos")]
                    if mode == CaptureMode::Video && chosen.is_some() && !selection.drag.active {
                        if let Some(frame) = video_frame(selection.window_id) {
                            selection.webcam.paint(painter, frame);
                        }
                    }
                    let label = if target_mode == CaptureMode::Window || chosen.is_none() {
                        selection
                            .windows
                            .iter()
                            .find(|w| Some(w.id) == selection.window_id)
                            .or(hovered_window)
                            .map_or("Window".into(), |w| w.name.clone())
                    } else {
                        selection::selection_to_pixels(
                            [rect.left() - bounds.left(), rect.top() - bounds.top()],
                            [rect.right() - bounds.left(), rect.bottom() - bounds.top()],
                            [bounds.width(), bounds.height()],
                            [selection.image.width(), selection.image.height()],
                        )
                        .map_or_else(String::new, |r| format!("{} × {} px", r.width, r.height))
                    };
                    let label_rect = Rect::from_min_size(
                        Pos2::new(rect.left().max(8.), (rect.top() - 31.).max(8.)),
                        Vec2::new(300_f32.min(bounds.width() - 16.), 26.),
                    );
                    painter.rect_filled(label_rect, 5, appearance::palette(ui.ctx()).navy);
                    painter.with_clip_rect(label_rect.shrink(5.)).text(
                        label_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(12.),
                        Color32::WHITE,
                    );
                }
                if let Some(p) = pointer.filter(|p| !bar.contains(*p))
                    && chosen.is_none()
                {
                    let stroke = Stroke::new(1., Color32::from_white_alpha(170));
                    painter.line_segment(
                        [
                            Pos2::new(bounds.left(), p.y),
                            Pos2::new(bounds.right(), p.y),
                        ],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            Pos2::new(p.x, bounds.top()),
                            Pos2::new(p.x, bounds.bottom()),
                        ],
                        stroke,
                    );
                }
                if selection.magnifier
                    && !matches!(target_mode, CaptureMode::Window | CaptureMode::FullScreen)
                    && (chosen.is_none() || selection.drag.active)
                    && let Some(p) = pointer.filter(|p| !bar.contains(*p))
                {
                    selection
                        .magnifier_preview
                        .paint(painter, p, bounds, bar, &selection.image);
                }
                let hint = if mode == CaptureMode::FullScreen {
                    "Click this screen to capture it · Move to any other screen to choose it · Esc cancels"
                } else if mode == CaptureMode::MultiRegion {
                    "Drag to add regions · Drag a region to adjust · Delete removes · Enter captures"
                } else if mode == CaptureMode::Video {
                    #[cfg(target_os = "macos")]
                    let camera_error = selection.webcam.preview_error.as_deref();
                    #[cfg(not(target_os = "macos"))]
                    let camera_error: Option<&str> = None;
                    #[cfg(target_os = "macos")]
                    let free = selection.webcam.mode == video_session::Webcam::Free && chosen.is_some();
                    #[cfg(not(target_os = "macos"))]
                    let free = false;
                    if let Some(error) = camera_error {
                        error
                    } else if free {
                        "Drag the webcam circle to position it · Record or Enter starts video · Esc cancels"
                    } else if target_mode == CaptureMode::FullScreen {
                        "Record this display · Choose a webcam position · Esc cancels"
                    } else {
                        "Click a window to select it · Record or Enter starts video · Esc cancels"
                    }
                } else if mode == CaptureMode::Text && self.settings.capture_on_release {
                    "Drag and release to copy text · Esc cancels"
                } else if mode == CaptureMode::Text {
                    "Drag around text · Adjust edges · Enter copies text · Esc cancels"
                } else if self.settings.capture_on_release && mode == CaptureMode::Window {
                    "Click a window to capture · Tab cycles overlapping windows · Esc cancels"
                } else if self.settings.capture_on_release && mode == CaptureMode::Region {
                    if cfg!(target_os = "macos") { "Drag and release to capture · ⌘-drag adds regions · Esc cancels" } else { "Drag and release to capture · Ctrl-drag adds regions · Esc cancels" }
                } else if mode == CaptureMode::Window {
                    "Click a window · Tab cycles overlapping windows · Enter captures"
                } else if mode == CaptureMode::Scrolling {
                    "Click a window or drag an area · Adjust edges · Start scrolling"
                } else if cfg!(target_os = "macos") {
                    "Click a window or drag an area · ⌘-drag adds regions · Enter captures"
                } else {
                    "Click a window or drag an area · Ctrl-drag adds regions · Enter captures"
                };
                let hintrect = Rect::from_center_size(
                    Pos2::new(bounds.center().x, 36.),
                    Vec2::new(700_f32.min(bounds.width() - 16.), 30.),
                );
                painter.rect_filled(hintrect, 8, appearance::palette(ui.ctx()).navy);
                painter.text(
                    hintrect.center(),
                    egui::Align2::CENTER_CENTER,
                    hint,
                    egui::FontId::proportional(13.),
                    Color32::WHITE,
                );
                if confirm && !placing_overlay && !selection.drag.active {
                    if scroll && mode != CaptureMode::Window {
                        accepted = chosen.and_then(|rect| selection::selection_to_pixels(
                            [rect.left() - bounds.left(), rect.top() - bounds.top()],
                            [rect.right() - bounds.left(), rect.bottom() - bounds.top()],
                            [bounds.width(), bounds.height()],
                            [selection.image.width(), selection.image.height()]));
                    } else if target_mode == CaptureMode::FullScreen {
                        accepted = Some(selection::PixelRect { x: 0, y: 0,
                            width: selection.image.width(), height: selection.image.height() });
                    } else if mode == CaptureMode::MultiRegion {
                        accepted_regions = selection.multi.rects.iter().filter_map(|rect|
                            selection::selection_to_pixels(
                                [rect.left()-bounds.left(), rect.top()-bounds.top()],
                                [rect.right()-bounds.left(), rect.bottom()-bounds.top()],
                                [bounds.width(), bounds.height()],
                                [selection.image.width(), selection.image.height()])).collect();
                    } else if matches!(target_mode, CaptureMode::Window | CaptureMode::Scrolling) {
                        if let Some(w) = selection
                            .windows
                            .iter()
                            .find(|w| Some(w.id) == selection.window_id)
                            .or_else(|| {
                                (mode == CaptureMode::Window)
                                    .then_some(hovered_window)
                                    .flatten()
                            })
                        {
                            window = Some(WindowCaptureRequest {
                                id: w.id,
                                crop: if mode == CaptureMode::Scrolling {
                                    chosen.and_then(|r| window_selection(r, window_rect(w)))
                                } else {
                                    None
                                },
                                scrolling: scroll || mode == CaptureMode::Scrolling,
                            });
                        }
                    } else if let Some(rect) = chosen {
                        accepted = selection::selection_to_pixels(
                            [rect.left() - bounds.left(), rect.top() - bounds.top()],
                            [rect.right() - bounds.left(), rect.bottom() - bounds.top()],
                            [bounds.width(), bounds.height()],
                            [selection.image.width(), selection.image.height()],
                        );
                    }
                }
            });
        if mode != old_mode || destination_changed {
            self.persist_settings();
        }
        if cancel {
            Some(SelectionAction::Cancel)
        } else if let Some(request) = window {
            Some(SelectionAction::Window(request))
        } else if !accepted_regions.is_empty() {
            Some(SelectionAction::Regions(accepted_regions))
        } else {
            accepted.map(if scroll {
                SelectionAction::Scroll
            } else {
                SelectionAction::Region
            })
        }
    }

    fn capture_selected_window(
        &mut self,
        ctx: &egui::Context,
        selection: Selection,
        request: WindowCaptureRequest,
    ) {
        self.pending_capture = Some(selection.return_hidden);
        self.generation += 1;
        let generation = self.generation;
        let WindowCaptureRequest {
            id,
            crop,
            scrolling,
        } = request;
        let window = selection.windows.iter().find(|w| w.id == id);
        let bounds = window
            .map(|w| {
                let [l, t, r, b] = crop.unwrap_or([0., 0., 1., 1.]);
                Rect::from_min_max(
                    Pos2::new(
                        w.x as f32 + l * w.width as f32,
                        w.y as f32 + t * w.height as f32,
                    ),
                    Pos2::new(
                        w.x as f32 + r * w.width as f32,
                        w.y as f32 + b * w.height as f32,
                    ),
                )
            })
            .unwrap_or(Rect::NOTHING);
        // A moving pointer is not document content and can spoil an overlap.
        let pointer = self.settings.include_pointer && !scrolling;
        let demo = self.demo;
        let cancel = Arc::new(AtomicBool::new(false));
        self.capture_cancel = Some(cancel.clone());
        self.hidden.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // ScreenCaptureKit reads the target window directly on Mac. GDI
            // capture still gives the selection overlay time to leave the screen.
            if cfg!(target_os = "windows") {
                std::thread::sleep(Duration::from_millis(250));
            }
            if cancel.load(Ordering::Acquire) {
                return;
            }
            let result = if demo {
                let w = selection.windows.iter().find(|w| w.id == id).unwrap();
                let sx = selection.image.width() as f64 / selection.display.width as f64;
                let sy = selection.image.height() as f64 / selection.display.height as f64;
                selection::crop_selection(
                    &selection.image,
                    selection::PixelRect {
                        x: ((w.x - selection.display.x) as f64 * sx).round() as u32,
                        y: ((w.y - selection.display.y) as f64 * sy).round() as u32,
                        width: (w.width as f64 * sx).round() as u32,
                        height: (w.height as f64 * sy).round() as u32,
                    },
                    CaptureShape::Rectangle,
                    0,
                )
            } else {
                capture::capture_window(id, pointer)
            };
            let result = result.map(|image| {
                if demo && scrolling {
                    image::RgbaImage::from_fn(image.width(), image.height(), demo_scroll_pixel)
                } else {
                    image
                }
            });
            let _ = tx.send(WorkEvent::WindowCaptured(
                generation,
                selection.display,
                request,
                bounds,
                Local::now(),
                result,
            ));
            crate::runtime::wake_root(&ctx);
        });
    }

    pub(super) fn start_scroll_session(
        &mut self,
        ctx: &egui::Context,
        selection: Selection,
        rect: selection::PixelRect,
    ) {
        if rect.width < 32 || rect.height < 48 {
            self.error = Some("Select a scrolling area at least 32 × 48 pixels.".into());
            self.restore_window(ctx, false);
            return;
        }
        match selection::crop_selection(&selection.image, rect, CaptureShape::Rectangle, 0) {
            Ok(image) => {
                let bounds = display_selection_bounds(
                    &selection.display,
                    selection.image.dimensions(),
                    rect,
                );
                self.scroll_session = Some(ScrollSession {
                    capture_size: selection.image.dimensions(),
                    display: selection.display,
                    rect,
                    captured_at: selection.captured_at,
                    stitcher: Some(Stitcher::region(image)),
                    message: "Scroll down slowly. Choose Done when you have enough.".into(),
                    texture: None,
                    placement_frames: 0,
                    demo_offset: 0,
                    window_id: None,
                    window_crop: None,
                    bounds,
                    return_hidden: selection.return_hidden,
                    reset_requested: false,
                    paused: false,
                    finish_requested: false,
                    single_frame_warning: false,
                    next_capture: std::time::Instant::now() + Duration::from_millis(300),
                });
                self.restore_frames_left = 0;
                self.hidden.store(true, Ordering::Relaxed);
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                crate::runtime::wake_root(ctx);
            }
            Err(error) => {
                self.error = Some(error);
                self.restore_window(ctx, false);
            }
        }
    }

    pub(super) fn start_window_scroll(
        &mut self,
        ctx: &egui::Context,
        display: DisplayInfo,
        id: u32,
        area: WindowScrollArea,
        image: image::RgbaImage,
        at: DateTime<Local>,
    ) {
        let size = image.dimensions();
        let window_crop = area.crop.and_then(|[left, top, right, bottom]| {
            selection::selection_to_pixels([left, top], [right, bottom], [1., 1.], [size.0, size.1])
        });
        let rect = window_crop.unwrap_or(selection::PixelRect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        });
        if rect.width < 32 || rect.height < 48 {
            self.error = Some("Select a scrolling area at least 32 × 48 pixels.".into());
            self.restore_window(ctx, false);
            return;
        }
        let stitcher = if window_crop.is_some() {
            match selection::crop_selection(&image, rect, CaptureShape::Rectangle, 0) {
                Ok(area) => Stitcher::region(area),
                Err(error) => {
                    self.error = Some(error);
                    self.restore_window(ctx, false);
                    return;
                }
            }
        } else {
            Stitcher::window(image)
        };
        self.scroll_session = Some(ScrollSession {
            display,
            rect,
            capture_size: size,
            captured_at: at,
            stitcher: Some(stitcher),
            message: "Scroll each pane down in turn. Done combines them.".into(),
            texture: None,
            placement_frames: 0,
            demo_offset: 0,
            window_id: Some(id),
            window_crop,
            bounds: area.bounds,
            return_hidden: area.return_hidden,
            reset_requested: false,
            paused: false,
            finish_requested: false,
            single_frame_warning: false,
            next_capture: std::time::Instant::now() + Duration::from_millis(100),
        });
        self.restore_frames_left = 0;
        self.hidden.store(true, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        crate::runtime::wake_root(ctx);
    }

    pub(super) fn capture_scroll_section(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(session) = &mut self.scroll_session else {
            return;
        };
        let Some(mut stitcher) = session.stitcher.take() else {
            return;
        };
        let display = session.display.clone();
        let rect = session.rect;
        let capture_size = session.capture_size;
        let window_id = session.window_id;
        let window_crop = session.window_crop;
        let final_sample = session.finish_requested;
        let reset = std::mem::take(&mut session.reset_requested);
        let demo = self.demo;
        session.demo_offset += 1;
        let demo_offset = session.demo_offset.div_ceil(2) * rect.height / 3;
        let generation = self.generation;
        self.busy = true;
        if window_id.is_none() {
            self.hidden.store(true, Ordering::Relaxed);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        let tx = self.work_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let capture = || {
                if demo {
                    Ok(image::RgbaImage::from_fn(
                        capture_size.0,
                        capture_size.1,
                        |x, y| demo_scroll_pixel(x, y + demo_offset),
                    ))
                } else if let Some(id) = window_id {
                    capture::capture_window(id, false)
                } else {
                    capture::capture_display(display.id, false)
                }
            };
            let mut reset = reset;
            let mut observe = || {
                capture().and_then(|image| {
                    if image.dimensions() != capture_size {
                        return Err(
                            "The capture area changed size. Choose Done and start another capture."
                                .into(),
                        );
                    }
                    let area = if window_id.is_some() {
                        window_crop
                    } else {
                        Some(rect)
                    };
                    let image = match area {
                        Some(rect) => {
                            selection::crop_selection(&image, rect, CaptureShape::Rectangle, 0)?
                        }
                        None => image,
                    };
                    if std::mem::take(&mut reset) {
                        stitcher = if area.is_some() {
                            Stitcher::region(image)
                        } else {
                            Stitcher::window(image)
                        };
                        Ok(true)
                    } else {
                        stitcher.observe_window(image)
                    }
                })
            };
            let mut result = observe();
            if final_sample && result.is_ok() {
                std::thread::sleep(Duration::from_millis(250));
                result = observe().map(|added| added || result.unwrap_or(false));
            }
            let _ = tx.send(WorkEvent::ScrollSection(
                generation,
                Box::new(stitcher),
                result,
                final_sample,
            ));
            crate::runtime::wake_root(&ctx);
        });
    }
    pub(super) fn finish_scroll_capture(&mut self, ctx: &egui::Context) {
        let Some(session) = &mut self.scroll_session else {
            return;
        };
        let Some(stitcher) = &session.stitcher else {
            return;
        };
        if stitcher.sections == 1 && !session.single_frame_warning {
            session.single_frame_warning = true;
            session.finish_requested = false;
            session.paused = true;
            session.message = "No additional sections were captured. For a PDF, use continuous scrolling and move down in small steps. Resume to try again, or keep only the visible area.".into();
            return;
        }
        let Some(session) = self.scroll_session.take() else {
            return;
        };
        session.close(ctx);
        self.generation += 1;
        if let Some(stitcher) = session.stitcher {
            self.save_captured_image(ctx, stitcher.image, session.captured_at);
        }
    }

    pub(super) fn cancel_capture(&mut self, ctx: &egui::Context) {
        if let Some(session) = self.scroll_session.take() {
            session.close(ctx);
            self.generation += 1;
            self.busy = false;
            self.restore_window(ctx, session.return_hidden);
            self.status = "Scrolling capture canceled".into();
        } else if self.selection.is_some() {
            self.cancel_selection(ctx);
        } else if let Some(return_hidden) = self.pending_capture.take() {
            self.generation += 1;
            if let Some(cancel) = self.capture_cancel.take() {
                cancel.store(true, Ordering::Release);
            }
            self.restore_window(ctx, return_hidden);
            self.status = "Capture canceled".into();
        }
    }

    pub(super) fn scroll_session_ui(&mut self, ctx: &egui::Context) {
        let Some(session) = &mut self.scroll_session else {
            return;
        };
        let divisor = ctx.zoom_factor()
            * if cfg!(target_os = "windows") {
                session.display.scale_factor
            } else {
                1.
            };
        let (origin, size) = overlay_geometry(&session.display, divisor);
        let screen = Rect::from_min_size(origin, size);
        let selected =
            Rect::from_min_max(session.bounds.min / divisor, session.bounds.max / divisor)
                .intersect(screen);
        let control_size = Vec2::new(460_f32.min(screen.width() - 16.), 76.);
        let controls = Rect::from_min_size(
            controls_position(screen, selected, control_size),
            control_size,
        );
        let preview_size = Vec2::new(148., 224.);
        let preview = scroll_preview_rect(screen, selected, controls, preview_size);
        let placing = session.placement_frames < 4;
        let look = self.settings.capture_theme.look();
        let mut finish = false;
        let mut cancel = false;
        // Four thin, click-through windows outline the selected area without
        // intercepting wheel events or covering the scrolling document.
        for (index, edge) in scroll_edges(selected).into_iter().enumerate() {
            show_scroll_viewport(ctx, index + 2, edge, placing, true, |child| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(Color32::from_rgb(85, 232, 212)))
                    .show(child, |_| {});
            });
        }
        show_scroll_viewport(ctx, 0, controls, placing, false, |child| {
            cancel |=
                child.input(|i| i.key_pressed(egui::Key::Escape) || i.viewport().close_requested());
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(look.palette.paper)
                        .stroke(Stroke::new(1., look.palette.border))
                        .inner_margin(8),
                )
                .show(child, |ui| {
                    crate::capture_panel::apply_look(ui, &look);
                    ui.spacing_mut().item_spacing.x = 8.;
                    ui.horizontal(|ui| {
                        finish = ui
                            .add_enabled(
                                !session.finish_requested,
                                appearance::primary(
                                    child,
                                    if session.single_frame_warning {
                                        "Keep visible area"
                                    } else {
                                        "Done"
                                    },
                                )
                                .min_size(Vec2::new(80., 30.)),
                            )
                            .on_hover_text(self.settings.capture_destination.finish_label())
                            .clicked();
                        ui.label(if session.finish_requested {
                            "Finishing…"
                        } else if session.paused {
                            "Paused"
                        } else {
                            "Scroll slowly ↓"
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            cancel |= capture_icon(
                                ui,
                                "Cancel capture · Esc",
                                appearance::Icon::Close,
                                false,
                            )
                            .clicked();
                            if ui
                                .add_enabled_ui(!session.finish_requested, |ui| {
                                    capture_icon(
                                        ui,
                                        "Restart from the current visible area",
                                        appearance::Icon::Refresh,
                                        false,
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                session.reset_requested = true;
                                session.paused = false;
                                session.single_frame_warning = false;
                                session.message = "Starting again from the current area…".into();
                                session.next_capture = std::time::Instant::now();
                            }
                            let label = if session.paused {
                                "Resume collecting"
                            } else {
                                "Pause collecting"
                            };
                            if ui
                                .add_enabled_ui(!session.finish_requested, |ui| {
                                    capture_icon(
                                        ui,
                                        label,
                                        if session.paused {
                                            appearance::Icon::Play
                                        } else {
                                            appearance::Icon::Pause
                                        },
                                        session.paused,
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                session.paused = !session.paused;
                                session.single_frame_warning = false;
                                session.message = if session.paused {
                                    "Paused. Done keeps the accepted sections."
                                } else {
                                    "Scroll down slowly. Sections are collected automatically."
                                }
                                .into();
                            }
                        });
                    });
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&session.message)
                                .small()
                                .color(look.palette.ink),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&session.message);
                });
        });
        if let Some(stitcher) = &session.stitcher
            && session.texture.is_none()
        {
            let thumbnail = image::imageops::thumbnail(&stitcher.image, 264, 352);
            session.texture = Some(load_texture(ctx, "scroll-preview", &thumbnail));
        }
        show_scroll_viewport(ctx, 1, preview, placing, true, |child| {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(look.palette.paper)
                        .stroke(Stroke::new(1., look.palette.border))
                        .inner_margin(8),
                )
                .show(child, |ui| {
                    crate::capture_panel::apply_look(ui, &look);
                    ui.vertical_centered(|ui| {
                        let pane_count = session.stitcher.as_ref().map_or(0, Stitcher::pane_count);
                        if pane_count >= 2 {
                            ui.small(format!("Preview · {pane_count} panes"));
                        } else {
                            ui.small("Preview");
                        }
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(132., 176.), egui::Sense::hover());
                        if let Some(texture) = &session.texture {
                            let size = texture.size_vec2();
                            let scale = (rect.width() / size.x).min(rect.height() / size.y);
                            ui.painter().image(
                                texture.id(),
                                Rect::from_center_size(rect.center(), size * scale),
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                Color32::WHITE,
                            );
                        }
                        if let Some(stitcher) = &session.stitcher {
                            ui.small(format!(
                                "{} × {} · {} parts",
                                stitcher.image.width(),
                                stitcher.image.height(),
                                stitcher.sections
                            ));
                        } else {
                            ui.small("Collecting…");
                        }
                    });
                });
        });
        if placing {
            session.placement_frames += 1;
            ctx.request_repaint();
        }
        if !placing
            && !self.demo
            && let Err(error) = crate::capture_window::exclude_scroll_controls()
        {
            session.paused = true;
            session.message = error;
        }
        if cancel {
            self.cancel_capture(ctx);
            return;
        }
        if finish {
            if session.paused && !self.busy && !session.reset_requested {
                self.finish_scroll_capture(ctx);
            } else {
                session.finish_requested = true;
                session.paused = true;
                session.message = "Finishing the current section…".into();
                if !self.busy {
                    self.capture_scroll_section(ctx);
                }
            }
            return;
        }
        if !placing && !session.paused && !session.finish_requested {
            let next = !self.busy && std::time::Instant::now() >= session.next_capture;
            ctx.request_repaint_after(Duration::from_millis(100));
            if next {
                self.capture_scroll_section(ctx);
            }
        }
    }
}

fn capture_icon(
    ui: &mut egui::Ui,
    label: &str,
    icon: appearance::Icon,
    selected: bool,
) -> egui::Response {
    let response = ui.add_sized([30., 30.], egui::Button::new("").selected(selected));
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    appearance::icon(
        ui.painter(),
        response.rect.center(),
        icon,
        ui.style().interact(&response).fg_stroke.color,
        18.,
    );
    response.on_hover_text(label)
}

fn scroll_id(index: usize) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("scroll-capture", index))
}
impl ScrollSession {
    pub fn captures_whole_window(&self) -> bool {
        self.window_id.is_some() && self.window_crop.is_none()
    }
    pub fn close(&self, ctx: &egui::Context) {
        for index in 0..6 {
            ctx.send_viewport_cmd_to(scroll_id(index), ViewportCommand::Visible(false));
            ctx.send_viewport_cmd_to(scroll_id(index), ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
    pub fn focus(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd_to(scroll_id(0), ViewportCommand::Focus);
    }
}

fn show_scroll_viewport(
    ctx: &egui::Context,
    index: usize,
    rect: Rect,
    placing: bool,
    passthrough: bool,
    mut content: impl FnMut(&egui::Context),
) {
    let title = format!("{} Scroll — {}", crate::branding::NAME, index);
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_position(rect.min)
        .with_inner_size(rect.size())
        .with_min_inner_size(Vec2::ZERO)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_has_shadow(false)
        .with_transparent(cfg!(target_os = "macos"))
        .with_mouse_passthrough(passthrough)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_active(false)
        .with_visible(!placing);
    ctx.show_viewport_immediate(scroll_id(index), builder, |child, _| {
        if placing {
            child.send_viewport_cmd(ViewportCommand::OuterPosition(rect.min));
            child.send_viewport_cmd(ViewportCommand::InnerSize(rect.size()));
        }
        content(child);
    });
}

fn display_selection_bounds(
    display: &DisplayInfo,
    size: (u32, u32),
    rect: selection::PixelRect,
) -> Rect {
    let scale = Vec2::new(
        display.width as f32 / size.0 as f32,
        display.height as f32 / size.1 as f32,
    );
    Rect::from_min_size(
        Pos2::new(display.x as f32, display.y as f32)
            + Vec2::new(rect.x as f32, rect.y as f32) * scale,
        Vec2::new(rect.width as f32, rect.height as f32) * scale,
    )
}

fn scroll_edges(rect: Rect) -> [Rect; 4] {
    let width = 2.;
    [
        Rect::from_min_size(
            rect.min - Vec2::splat(width),
            Vec2::new(rect.width() + 2. * width, width),
        ),
        Rect::from_min_size(
            rect.left_bottom() + Vec2::new(-width, 0.),
            Vec2::new(rect.width() + 2. * width, width),
        ),
        Rect::from_min_size(
            rect.min - Vec2::new(width, 0.),
            Vec2::new(width, rect.height()),
        ),
        Rect::from_min_size(rect.right_top(), Vec2::new(width, rect.height())),
    ]
}

fn scroll_preview_rect(screen: Rect, selected: Rect, controls: Rect, size: Vec2) -> Rect {
    let below = Pos2::new(selected.right() - size.x, selected.bottom() + 12.);
    let candidates = [
        Pos2::new(selected.right() + 12., selected.bottom() - size.y),
        Pos2::new(selected.left() - size.x - 12., selected.bottom() - size.y),
        below,
        Pos2::new(selected.right() - size.x, selected.top() - size.y - 12.),
    ];
    for point in candidates {
        let rect = Rect::from_min_size(point, size);
        if screen.shrink(8.).contains_rect(rect) && !rect.intersects(controls.expand(8.)) {
            return rect;
        }
    }
    // Full-display selections leave no outside space. These capture-excluded,
    // click-through controls remain usable without entering the result.
    let x = (screen.right() - size.x - 8.).max(screen.left() + 8.);
    let bottom = screen.bottom() - size.y - 8.;
    let candidate = Rect::from_min_size(Pos2::new(x, bottom), size);
    if candidate.intersects(controls.expand(8.)) {
        Rect::from_min_size(
            Pos2::new(x, (controls.top() - size.y - 8.).max(screen.top() + 8.)),
            size,
        )
    } else {
        candidate
    }
}

pub(super) fn demo_scroll_pixel(x: u32, y: u32) -> image::Rgba<u8> {
    let n = x
        .wrapping_mul(1973)
        .wrapping_add(y.wrapping_mul(9277))
        .wrapping_mul(26699);
    image::Rgba([
        (n % 251) as u8,
        ((n >> 8) % 251) as u8,
        ((n >> 16) % 251) as u8,
        255,
    ])
}

/// Keep confirmation controls close to the selection and inside the display.
fn controls_position(bounds: Rect, selected: Rect, size: Vec2) -> Pos2 {
    let x = (selected.center().x - size.x / 2.).clamp(
        bounds.left() + 8.,
        (bounds.right() - size.x - 8.).max(bounds.left() + 8.),
    );
    let below = selected.bottom() + 12.;
    let y = if below + size.y <= bounds.bottom() - 8. {
        below
    } else if selected.top() - 12. - size.y >= bounds.top() + 8. {
        selected.top() - 12. - size.y
    } else {
        bounds.bottom() - size.y - 12.
    };
    Pos2::new(x, y.max(bounds.top() + 8.))
}

/// Ratios remain valid across logical/display pixels and native window capture scale.
fn window_selection(area: Rect, window: Rect) -> Option<[f32; 4]> {
    let area = area.intersect(window);
    if area.min.distance(window.min) < 1. && area.max.distance(window.max) < 1. {
        return None;
    }
    Some([
        (area.left() - window.left()) / window.width(),
        (area.top() - window.top()) / window.height(),
        (area.right() - window.left()) / window.width(),
        (area.bottom() - window.top()) / window.height(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_hud_keeps_editor_hidden_and_passes_document_input_through() {
        use std::{cell::RefCell, rc::Rc};
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        let screen = test_screens(&ctx, CaptureMode::Region).screens.remove(0);
        app.start_scroll_session(
            &ctx,
            screen,
            selection::PixelRect {
                x: 80,
                y: 80,
                width: 400,
                height: 320,
            },
        );
        app.scroll_session.as_mut().unwrap().paused = true;
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let recorded = rendered.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            recorded
                .borrow_mut()
                .push((viewport.ids.this, viewport.builder.clone()));
            let size = viewport.builder.inner_size.unwrap();
            let _ = ctx.run(
                egui::RawInput {
                    viewport_id: viewport.ids.this,
                    viewports: [(viewport.ids.this, egui::ViewportInfo::default())]
                        .into_iter()
                        .collect(),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| (viewport.viewport_ui_cb)(ctx),
            );
        });
        for frame in 0..6 {
            rendered.borrow_mut().clear();
            let output = ctx.run(egui::RawInput::default(), |ctx| app.scroll_session_ui(ctx));
            assert!(app.hidden.load(Ordering::Relaxed));
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .is_empty()
            );
            let windows = rendered.borrow();
            assert_eq!(windows.len(), 6);
            for (id, builder) in windows.iter() {
                assert_eq!(builder.active, Some(false));
                assert_eq!(builder.visible, Some(frame >= 4));
                assert_eq!(builder.mouse_passthrough, Some(*id != scroll_id(0)));
            }
        }
        app.cancel_capture(&ctx);
        ctx.set_embed_viewports(true);
    }

    #[test]
    fn escape_cancels_capture_even_with_an_open_popup() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let mut session = test_screens(&ctx, CaptureMode::Region);
        let screen = &mut session.screens[0];
        egui::Popup::open_id(&ctx, egui::Id::new("mode-menu"));
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(
            overlay_frame(&mut app, &ctx, screen, vec![key(egui::Key::Escape)]),
            Some(SelectionAction::Cancel)
        );
    }

    #[test]
    fn capture_toolbar_stays_compact_in_each_mode() {
        for mode in CaptureMode::ALL {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let mut screens = test_screens(&ctx, mode).screens;
            let screen = &mut screens[0];
            overlay_frame(&mut app, &ctx, screen, vec![]);
            let size = ctx
                .data(|data| data.get_temp::<Vec2>(egui::Id::new("capture-toolbar-size")))
                .unwrap();
            assert!(size.x < 620. && size.y < 60., "{mode:?}: {size:?}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn full_display_video_waits_for_confirmation_and_uses_retina_dimensions() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.capture_mode = CaptureMode::Video;
        app.settings.capture_on_release = true;
        app.settings.video.target = video_session::VideoTarget::FullScreen;
        let mut screen = test_screens(&ctx, CaptureMode::Video).screens.remove(1);
        let point = Pos2::new(250., 220.);
        let click = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(
            overlay_frame(
                &mut app,
                &ctx,
                &mut screen,
                vec![egui::Event::PointerMoved(point), click(true), click(false)]
            ),
            None
        );
        assert_eq!(
            overlay_frame(&mut app, &ctx, &mut screen, vec![key(egui::Key::Enter)]),
            Some(SelectionAction::Region(selection::PixelRect {
                x: 0,
                y: 0,
                width: 1600,
                height: 1200
            }))
        );
    }

    #[test]
    fn scroll_geometry_handles_retina_and_negative_display_origins() {
        let ctx = egui::Context::default();
        let screen = test_screens(&ctx, CaptureMode::Region).screens.remove(1);
        let rect = selection::PixelRect {
            x: 200,
            y: 100,
            width: 600,
            height: 400,
        };
        let bounds = display_selection_bounds(&screen.display, screen.image.dimensions(), rect);
        assert_eq!(
            bounds,
            Rect::from_min_size(Pos2::new(-700., -550.), Vec2::new(300., 200.))
        );
        for edge in scroll_edges(bounds) {
            assert_eq!(edge.intersect(bounds).area(), 0.);
        }
        let desktop = Rect::from_min_size(Pos2::new(-800., -600.), Vec2::new(800., 600.));
        let controls = Rect::from_min_size(
            controls_position(desktop, bounds, Vec2::new(460., 76.)),
            Vec2::new(460., 76.),
        );
        let preview = scroll_preview_rect(desktop, bounds, controls, Vec2::new(148., 224.));
        assert!(desktop.contains_rect(controls));
        assert!(desktop.contains_rect(preview));
        assert!(!preview.intersects(controls));
    }

    fn wait_for_scroll_sample(app: &mut MyImageApp, ctx: &egui::Context) {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while app.busy && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        }
        assert!(!app.busy, "scroll sample timed out");
    }

    #[test]
    fn region_scroll_collects_and_cancels_from_editor_or_background() {
        for return_hidden in [false, true] {
            let mode = CaptureMode::Region;
            let ctx = egui::Context::default();
            ctx.set_embed_viewports(false);
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
            app.settings.capture_mode = mode;
            let mut screen = test_screens(&ctx, mode).screens.remove(0);
            screen.return_hidden = return_hidden;
            screen.image = image::RgbaImage::from_fn(800, 600, demo_scroll_pixel);
            let rect = selection::PixelRect {
                x: 100,
                y: 100,
                width: 320,
                height: 300,
            };
            app.start_scroll_session(&ctx, screen, rect);
            assert!(app.hidden.load(Ordering::Relaxed));
            assert_eq!(app.settings.capture_mode, mode);
            app.capture_scroll_section(&ctx);
            wait_for_scroll_sample(&mut app, &ctx);
            let session = app.scroll_session.as_ref().unwrap();
            assert_eq!(session.window_id, None);
            assert_eq!(session.return_hidden, return_hidden);
            let stitcher = session.stitcher.as_ref().unwrap();
            assert_eq!(stitcher.image.dimensions(), (320, 400));
            assert_eq!(stitcher.sections, 2);
            assert!(!session.paused);

            app.scroll_session.as_mut().unwrap().reset_requested = true;
            app.capture_scroll_section(&ctx);
            wait_for_scroll_sample(&mut app, &ctx);
            let stitcher = app
                .scroll_session
                .as_ref()
                .unwrap()
                .stitcher
                .as_ref()
                .unwrap();
            assert_eq!(stitcher.image.dimensions(), (320, 300));
            assert_eq!(stitcher.sections, 1);
            assert_eq!(*stitcher.image.get_pixel(0, 0), demo_scroll_pixel(100, 200));

            let generation = app.generation;
            let stale = app
                .scroll_session
                .as_mut()
                .unwrap()
                .stitcher
                .take()
                .unwrap();
            app.busy = true;
            app.desktop_tx.send(DesktopEvent::CancelCapture).unwrap();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                for index in 0..6 {
                    ctx.show_viewport_deferred(
                        scroll_id(index),
                        egui::ViewportBuilder::default(),
                        |_, _| {},
                    );
                }
                app.handle_events(ctx);
            });
            assert!(app.scroll_session.is_none());
            assert!(!app.busy);
            for index in 0..6 {
                assert!(
                    output.viewport_output[&scroll_id(index)]
                        .commands
                        .contains(&ViewportCommand::Close)
                );
            }
            app.work_tx
                .send(WorkEvent::ScrollSection(
                    generation,
                    Box::new(stale),
                    Ok(true),
                    true,
                ))
                .unwrap();
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
            assert!(app.scroll_session.is_none());
            assert!(app.document.is_none());
            assert_eq!(app.settings.capture_mode, mode);
        }
    }

    #[test]
    fn window_scroll_button_starts_collection_without_changing_mode() {
        for return_hidden in [false, true] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
            app.settings.capture_mode = CaptureMode::Window;
            app.settings.capture_on_release = false;
            let mut screen = test_screens(&ctx, CaptureMode::Window).screens.remove(0);
            screen.return_hidden = return_hidden;
            screen.windows.push(capture::WindowInfo {
                id: 42,
                name: "Scrolling document".into(),
                x: 100,
                y: 80,
                width: 400,
                height: 300,
            });
            for _ in 0..3 {
                overlay_frame(&mut app, &ctx, &mut screen, vec![]);
            }
            let p = Pos2::new(200., 180.);
            for pressed in [true, false] {
                assert!(
                    overlay_frame(
                        &mut app,
                        &ctx,
                        &mut screen,
                        vec![egui::Event::PointerMoved(p), pointer_button(p, pressed)],
                    )
                    .is_none()
                );
            }
            assert_eq!(screen.window_id, Some(42));
            let mut button = None;
            for _ in 0..3 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                        ..Default::default()
                    },
                    |ctx| {
                        assert!(app.selection_overlay_ui(ctx, &mut screen, None).is_none());
                    },
                );
                button = output
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .find_map(|(_, node)| {
                        (node.label() == Some("Scrolling capture"))
                            .then(|| node.bounds())
                            .flatten()
                    });
            }
            let button = button.expect("selected Window must offer scrolling");
            let p = Pos2::new(
                (button.x0 + button.x1) as f32 / 2.,
                (button.y0 + button.y1) as f32 / 2.,
            );
            overlay_frame(
                &mut app,
                &ctx,
                &mut screen,
                vec![egui::Event::PointerMoved(p), pointer_button(p, true)],
            );
            let action = overlay_frame(&mut app, &ctx, &mut screen, vec![pointer_button(p, false)]);
            let Some(SelectionAction::Window(request)) = action else {
                panic!("Window scrolling lost its native target: {action:?}");
            };
            assert_eq!(request.id, 42);
            assert!(request.scrolling);
            assert_eq!(request.crop, None);
            app.capture_selected_window(&ctx, screen, request);
            let until = std::time::Instant::now() + Duration::from_secs(5);
            while app.pending_capture.is_some() && std::time::Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
            }
            let session = app
                .scroll_session
                .as_ref()
                .expect("Window scroll must start without changing the saved capture mode");
            assert_eq!(session.window_id, Some(42));
            assert_eq!(session.window_crop, None);
            assert_eq!(session.capture_size, (400, 300));
            assert_eq!(session.return_hidden, return_hidden);
            assert!(
                app.document.is_none(),
                "starting scrolling must not deliver a still image"
            );
            assert_eq!(app.settings.capture_mode, CaptureMode::Window);
            app.capture_scroll_section(&ctx);
            wait_for_scroll_sample(&mut app, &ctx);
            let session = app.scroll_session.as_ref().unwrap();
            assert!(!session.paused);
            assert_eq!(session.stitcher.as_ref().unwrap().sections, 2);
            app.cancel_capture(&ctx);
            assert!(app.scroll_session.is_none());
            assert_eq!(app.hidden.load(Ordering::Relaxed), return_hidden);
            assert_eq!(app.settings.capture_mode, CaptureMode::Window);
        }
    }

    #[test]
    fn reopening_during_scrolling_only_focuses_its_controller() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        let screen = test_screens(&ctx, CaptureMode::Region).screens.remove(0);
        app.start_scroll_session(
            &ctx,
            screen,
            selection::PixelRect {
                x: 0,
                y: 0,
                width: 320,
                height: 300,
            },
        );
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        app.desktop_tx.send(DesktopEvent::Reopen).unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            for index in 0..6 {
                ctx.show_viewport_deferred(
                    scroll_id(index),
                    egui::ViewportBuilder::default(),
                    |_, _| {},
                );
            }
            app.handle_events(ctx);
        });
        assert!(app.hidden.load(Ordering::Relaxed));
        assert!(
            !output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Visible(true))
        );
        assert!(
            output.viewport_output[&scroll_id(0)]
                .commands
                .contains(&ViewportCommand::Focus)
        );
    }

    #[test]
    fn dedicated_capture_keeps_editor_hidden_and_accepts_first_drag_during_fade() {
        use std::{cell::RefCell, rc::Rc};
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        app.settings.capture_mode = CaptureMode::Region;
        app.guide_open = false;
        app.pending_capture = Some(true);
        app.generation = 7;
        let mut frames = test_screens(&ctx, CaptureMode::Region).screens;
        let screen = frames.remove(0);
        app.work_tx
            .send(WorkEvent::Captured(
                7,
                Local::now(),
                Ok(vec![CapturedFrame {
                    display: screen.display,
                    image: screen.image,
                    windows: vec![],
                }]),
            ))
            .unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        assert!(app.selection.is_some());
        assert!(app.hidden.load(Ordering::Relaxed));
        assert!(
            !output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Visible(true))
        );

        // Use production presentation for the synthetic child windows.
        app.demo = false;
        let render_frame = Rc::new(RefCell::new(0));
        let frame_for_renderer = render_frame.clone();
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let rendered_for_renderer = rendered.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            let frame = *frame_for_renderer.borrow();
            let _ = ctx.run(
                egui::RawInput {
                    viewport_id: viewport.ids.this,
                    viewports: [(
                        viewport.ids.this,
                        egui::ViewportInfo {
                            focused: Some(false),
                            ..Default::default()
                        },
                    )]
                    .into_iter()
                    .collect(),
                    time: Some(10. + f64::from(frame) * 0.01),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events: match frame {
                        5 => vec![egui::Event::PointerMoved(Pos2::new(100., 150.))],
                        6 => vec![pointer_button(Pos2::new(100., 150.), true)],
                        7 => vec![egui::Event::PointerMoved(Pos2::new(300., 350.))],
                        _ => vec![],
                    },
                    ..Default::default()
                },
                |ctx| (viewport.viewport_ui_cb)(ctx),
            );
            rendered_for_renderer
                .borrow_mut()
                .push((viewport.ids.this, viewport.builder.visible));
        });
        for frame in 0..8 {
            *render_frame.borrow_mut() = frame;
            rendered.borrow_mut().clear();
            let output = ctx.run(egui::RawInput::default(), |ctx| app.selection_ui(ctx));
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            // Capturing must never reveal, focus, restyle or resize the editor.
            assert!(
                commands.is_empty(),
                "Unexpected editor commands: {commands:?}"
            );
            assert!(app.hidden.load(Ordering::Relaxed));
            let screen = &app.selection.as_ref().unwrap().screens[0];
            let rendered = rendered.borrow();
            assert!(!rendered.is_empty());
            assert!(
                rendered
                    .iter()
                    .all(|&(id, visible)| id == overlay_id(&screen.display)
                        && visible == Some(frame >= 4 && !cfg!(target_os = "macos")))
            );
            assert_eq!(
                output.viewport_output[&overlay_id(&screen.display)]
                    .commands
                    .contains(&ViewportCommand::Focus),
                frame >= 4 && !cfg!(target_os = "macos")
            );
            assert_eq!(screen.reveal_time.is_some(), frame >= 4);
            if frame == 7 {
                assert!(screen.drag.active);
                assert_eq!(
                    screen.drag.bounds(),
                    Some(Rect::from_min_max(
                        Pos2::new(100., 150.),
                        Pos2::new(300., 350.)
                    ))
                );
            }
        }
        crate::capture_window::forget_overlay_reveals();
        ctx.set_embed_viewports(true);
    }

    #[test]
    fn selection_dimming_is_bounded_monotonic_and_finishes_promptly() {
        assert_eq!(selection_shade_alpha(-1.), 0);
        assert_eq!(selection_shade_alpha(0.), 0);
        assert_eq!(selection_shade_alpha(SELECTION_FADE_SECONDS), 145);
        assert_eq!(selection_shade_alpha(10.), 145);
        let levels: Vec<_> = (0..=16)
            .map(|frame| selection_shade_alpha(f64::from(frame) * 0.01))
            .collect();
        assert!(levels.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!((70..=75).contains(&selection_shade_alpha(0.08)));
    }

    #[test]
    fn capture_start_and_window_hover_never_dim_the_desktop() {
        for mode in CaptureMode::ALL {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
            app.settings.capture_mode = mode;
            let mut screen = test_screens(&ctx, mode).screens.remove(0);
            screen.windows.push(capture::WindowInfo {
                id: 42,
                name: "Test window".into(),
                x: 200,
                y: 120,
                width: 300,
                height: 250,
            });
            for (time, events) in [
                (10., vec![]),
                (10.3, vec![]),
                (10.4, vec![egui::Event::PointerMoved(Pos2::new(250., 200.))]),
                (10.7, vec![]),
            ] {
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        assert!(app.selection_overlay_ui(ctx, &mut screen, None).is_none());
                    },
                );
                assert!(
                    screen.shade_started_at.is_none(),
                    "Unexpected idle shade in {mode:?}"
                );
                assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::epaint::Shape::Rect(rect) if rect.fill == Color32::from_black_alpha(145))),
                    "Desktop darkened before selection in {mode:?}");
            }
        }
    }

    #[test]
    fn drawing_dims_only_outside_the_region_and_reset_restores_brightness() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        app.settings.capture_mode = CaptureMode::Region;
        let mut screen = test_screens(&ctx, CaptureMode::Region).screens.remove(0);
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        for (time, events) in [
            (20., vec![pointer_button(Pos2::new(100., 150.), true)]),
            (20.1, vec![egui::Event::PointerMoved(Pos2::new(300., 350.))]),
            (20.3, vec![pointer_button(Pos2::new(300., 350.), false)]),
        ] {
            let output = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(bounds),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    assert!(app.selection_overlay_ui(ctx, &mut screen, None).is_none());
                },
            );
            if time == 20.3 {
                let selected = screen.drag.bounds().unwrap();
                let shaded: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Rect(rect)
                            if rect.fill == Color32::from_black_alpha(145) =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(shaded.len(), 4);
                assert!(shaded.iter().all(|r| !r.contains(selected.center())));
                assert_eq!(
                    shaded.iter().map(|r| r.area()).sum::<f32>(),
                    bounds.area() - selected.area()
                );
            }
        }
        screen.drag = RegionDrag::default();
        overlay_frame(&mut app, &ctx, &mut screen, vec![]);
        assert!(screen.shade_started_at.is_none());
    }

    fn test_screens(ctx: &egui::Context, mode: CaptureMode) -> SelectionSession {
        let frames = [
            (1, 0, 0, 1., [240, 10, 20, 255]),
            (2, -800, -600, 2., [10, 20, 240, 255]),
        ]
        .into_iter()
        .map(|(id, x, y, scale, color)| CapturedFrame {
            display: DisplayInfo {
                id,
                name: format!("Display {id}"),
                x,
                y,
                width: 800,
                height: 600,
                scale_factor: scale,
                is_primary: id == 1,
            },
            image: image::RgbaImage::from_pixel(
                (800. * scale) as u32,
                (600. * scale) as u32,
                image::Rgba(color),
            ),
            windows: Vec::new(),
        })
        .collect();
        let mut session = SelectionSession::new(ctx, frames, Local::now(), mode, false);
        for screen in &mut session.screens {
            screen.placement_frames = 5;
            screen.magnifier = false;
        }
        session
    }

    fn overlay_frame(
        app: &mut MyImageApp,
        ctx: &egui::Context,
        screen: &mut Selection,
        events: Vec<egui::Event>,
    ) -> Option<SelectionAction> {
        let mut action = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                events,
                ..Default::default()
            },
            |ctx| {
                action = app.selection_overlay_ui(ctx, screen, None);
            },
        );
        action
    }

    fn pointer_button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn either_display_selects_its_own_pixels_at_native_resolution() {
        for index in 0..2 {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let mut session = test_screens(&ctx, CaptureMode::Region);
            let screen = &mut session.screens[index];
            let start = Pos2::new(100., 150.);
            let end = Pos2::new(300., 270.);
            overlay_frame(&mut app, &ctx, screen, vec![]);
            overlay_frame(
                &mut app,
                &ctx,
                screen,
                vec![
                    egui::Event::PointerMoved(start),
                    pointer_button(start, true),
                ],
            );
            overlay_frame(&mut app, &ctx, screen, vec![egui::Event::PointerMoved(end)]);
            assert_eq!(
                overlay_frame(&mut app, &ctx, screen, vec![pointer_button(end, false)]),
                None
            );
            let action = overlay_frame(&mut app, &ctx, screen, vec![key(egui::Key::Enter)]);
            let Some(SelectionAction::Region(rect)) = action else {
                panic!("Expected region: {action:?}");
            };
            let scale = (index + 1) as u32;
            assert_eq!(
                (rect.x, rect.y, rect.width, rect.height),
                (100 * scale, 150 * scale, 200 * scale, 120 * scale)
            );
            let cropped =
                selection::crop_selection(&screen.image, rect, CaptureShape::Rectangle, 0).unwrap();
            assert_eq!(
                cropped.get_pixel(0, 0).0,
                if index == 0 {
                    [240, 10, 20, 255]
                } else {
                    [10, 20, 240, 255]
                }
            );
            assert!(session.screens[1 - index].drag.bounds().is_none());
        }
    }

    #[test]
    fn selection_reaches_reveal_with_only_explicit_background_wakes() {
        let ctx = egui::Context::default();
        let hidden = Arc::new(AtomicBool::new(true));
        let mut app = MyImageApp::with_context(&ctx, false, hidden.clone(), true);
        app.settings.capture_mode = CaptureMode::Region;
        let pending = Arc::new(AtomicBool::new(false));
        crate::runtime::install_root_waker(&ctx, pending.clone(), |_| {});
        let mut session = test_screens(&ctx, CaptureMode::Region);
        for screen in &mut session.screens {
            screen.placement_frames = 0;
        }
        app.selection = Some(session);

        // No native window or repaint callback advances these frames. Each
        // placement frame must queue the next one, including the reveal.
        for frame in 0..=SELECTION_PLACEMENT_FRAMES {
            if frame > 0 {
                assert!(pending.swap(false, Ordering::AcqRel));
            }
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.selection_ui(ctx));
            assert!(hidden.load(Ordering::Relaxed));
            assert!(
                app.selection
                    .as_ref()
                    .unwrap()
                    .screens
                    .iter()
                    .all(|screen| {
                        screen.reveal_time.is_some() == (frame == SELECTION_PLACEMENT_FRAMES)
                    })
            );
        }
        assert!(!pending.load(Ordering::Acquire));
    }

    #[test]
    fn native_reopen_preserves_pending_capture_and_active_overlays() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        app.guide_open = false;
        app.view = View::Capture;
        app.generation = 42;
        app.pending_capture = Some(true);
        let cancel = Arc::new(AtomicBool::new(false));
        app.capture_cancel = Some(cancel.clone());
        app.desktop_tx.send(DesktopEvent::Reopen).unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        assert_eq!(app.pending_capture, Some(true));
        assert_eq!(app.generation, 42);
        assert!(!cancel.load(Ordering::Acquire));
        assert!(app.hidden.load(Ordering::Relaxed));
        assert!(
            !output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&ViewportCommand::Visible(true))
        );

        app.pending_capture = None;
        let session = test_screens(&ctx, CaptureMode::Region);
        let ids: Vec<_> = session
            .screens
            .iter()
            .map(|screen| screen.display.id)
            .collect();
        app.selection = Some(session);
        for placement in [0, 4] {
            app.selection.as_mut().unwrap().screens[0].placement_frames = placement;
            app.desktop_tx.send(DesktopEvent::Reopen).unwrap();
            let overlay = overlay_id(&app.selection.as_ref().unwrap().screens[0].display);
            let mut overlay_focused = false;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                app.handle_events(ctx);
                overlay_focused |= ctx.viewport_for(overlay, |viewport| {
                    viewport.commands.contains(&ViewportCommand::Focus)
                });
            });
            assert_eq!(
                app.selection
                    .as_ref()
                    .unwrap()
                    .screens
                    .iter()
                    .map(|screen| screen.display.id)
                    .collect::<Vec<_>>(),
                ids
            );
            assert_eq!(app.generation, 42);
            assert!(!cancel.load(Ordering::Acquire));
            assert!(matches!(app.view, View::Capture));
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            assert!(!commands.contains(&ViewportCommand::Focus));
            assert_eq!(overlay_focused, placement == 4);
            assert!(!commands.contains(&ViewportCommand::Decorations(true)));
        }

        // The explicit Editor command still leaves capture when requested.
        app.desktop_tx.send(DesktopEvent::Show).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        assert!(app.selection.is_none());
        assert!(cancel.load(Ordering::Acquire));
        assert!(matches!(app.view, View::Editor));
        app.hidden.store(true, Ordering::Relaxed);
        app.desktop_tx.send(DesktopEvent::Reopen).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.handle_events(ctx));
        assert!(!app.hidden.load(Ordering::Relaxed));
    }

    #[test]
    fn full_screen_click_chooses_either_screen_and_its_native_size() {
        for index in 0..2 {
            let ctx = egui::Context::default();
            let mut app =
                MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
            let mut session = test_screens(&ctx, CaptureMode::FullScreen);
            let screen = &mut session.screens[index];
            overlay_frame(&mut app, &ctx, screen, vec![]);
            overlay_frame(&mut app, &ctx, screen, vec![]);
            let pos = Pos2::new(400., 300.);
            assert_eq!(
                overlay_frame(
                    &mut app,
                    &ctx,
                    screen,
                    vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)]
                ),
                None
            );
            let action = overlay_frame(&mut app, &ctx, screen, vec![pointer_button(pos, false)]);
            assert_eq!(
                action,
                Some(SelectionAction::Region(selection::PixelRect {
                    x: 0,
                    y: 0,
                    width: screen.image.width(),
                    height: screen.image.height(),
                }))
            );
        }
    }

    #[test]
    fn overlay_geometry_preserves_negative_origins_and_mixed_scaling() {
        let ctx = egui::Context::default();
        let session = test_screens(&ctx, CaptureMode::Region);
        let display = &session.screens[1].display;
        // macOS uses Quartz points; Windows uses physical pixels / per-screen DPI.
        for divisor in [1., 1.25, 2., 2.3] {
            let (position, size) = overlay_geometry(display, divisor);
            assert!((position * divisor - Pos2::new(-800., -600.)).length() < 0.001);
            assert!((size * divisor - Vec2::new(800., 600.)).length() < 0.001);
        }
    }

    #[test]
    fn stationary_cursor_paints_first_guides_only_on_its_display_without_starting_selection() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(true)), true);
        let mut session = test_screens(&ctx, CaptureMode::Region);
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(400., 300.));
        let pointer = Pos2::new(-600., -450.);
        for screen in &mut session.screens {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(bounds),
                    ..Default::default()
                },
                |ctx| {
                    assert!(
                        app.selection_overlay_ui(ctx, screen, Some(pointer))
                            .is_none()
                    );
                },
            );
            let guides: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::LineSegment { points, stroke } = &shape.shape
                        && stroke.color == Color32::from_white_alpha(170)
                    {
                        Some(*points)
                    } else {
                        None
                    }
                })
                .collect();
            if screen.display.is_primary {
                assert!(guides.is_empty());
            } else {
                assert_eq!(
                    guides,
                    [
                        [Pos2::new(0., 75.), Pos2::new(400., 75.)],
                        [Pos2::new(100., 0.), Pos2::new(100., 300.)],
                    ]
                );
                assert_eq!(
                    pointer_in_overlay(Pos2::ZERO, &screen.display, bounds),
                    None
                );
            }
            assert!(!screen.pointer_ready && !screen.drag.active);
            assert!(screen.drag.bounds().is_none());
        }
    }

    #[test]
    fn coalesced_pointer_leave_does_not_drop_a_region_press_or_release() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let mut session = test_screens(&ctx, CaptureMode::Region);
        let screen = &mut session.screens[0];
        overlay_frame(&mut app, &ctx, screen, vec![]);
        let start = Pos2::new(150., 120.);
        let end = Pos2::new(450., 320.);
        overlay_frame(
            &mut app,
            &ctx,
            screen,
            vec![
                pointer_button(start, true),
                egui::Event::PointerMoved(end),
                egui::Event::PointerGone,
            ],
        );
        assert_eq!(screen.drag.bounds(), Some(Rect::from_two_pos(start, end)));
        assert!(screen.drag.active);
        assert!(!screen.pointer_ready);
        overlay_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerGone, pointer_button(end, false)],
        );
        assert!(screen.pointer_ready);
        assert!(screen.drag.completed);
        assert!(!screen.drag.active);
        assert_eq!(screen.drag.bounds(), Some(Rect::from_two_pos(start, end)));
    }

    #[test]
    fn leaving_overlay_clears_hover_but_drag_can_resume_and_finish() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let mut session = test_screens(&ctx, CaptureMode::Region);
        let screen = &mut session.screens[0];
        overlay_frame(&mut app, &ctx, screen, vec![]);
        let pos = Pos2::new(100., 150.);
        overlay_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)],
        );
        assert!(screen.drag.active && screen.pointer_ready);
        overlay_frame(&mut app, &ctx, screen, vec![egui::Event::PointerGone]);
        assert!(screen.drag.active && !screen.pointer_ready);
        let end = Pos2::new(700., 599.);
        overlay_frame(&mut app, &ctx, screen, vec![egui::Event::PointerMoved(end)]);
        assert!(screen.drag.active);
        assert_eq!(screen.drag.bounds(), Some(Rect::from_min_max(pos, end)));
        overlay_frame(&mut app, &ctx, screen, vec![pointer_button(end, false)]);
        assert!(!screen.drag.active);
        assert!(screen.drag.completed);
        assert_eq!(
            overlay_frame(&mut app, &ctx, screen, vec![key(egui::Key::Escape)]),
            Some(SelectionAction::Cancel)
        );
    }

    #[test]
    fn region_drag_crosses_window_edges_and_includes_screen_top_and_bottom() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.capture_mode = CaptureMode::Region;
        let mut session = test_screens(&ctx, CaptureMode::Region);
        let screen = &mut session.screens[0];
        screen.windows.push(capture::WindowInfo {
            id: 9,
            name: "Underlying window".into(),
            x: 50,
            y: 50,
            width: 400,
            height: 300,
        });
        overlay_frame(&mut app, &ctx, screen, vec![]);
        let start = Pos2::new(100., 150.);
        overlay_frame(&mut app, &ctx, screen, vec![pointer_button(start, true)]);
        let end = Pos2::new(799., 599.);
        overlay_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerMoved(end), pointer_button(end, false)],
        );
        assert_eq!(screen.drag.bounds(), Some(Rect::from_min_max(start, end)));

        screen.drag = RegionDrag::default();
        let top = Pos2::new(0., 0.);
        overlay_frame(&mut app, &ctx, screen, vec![pointer_button(top, true)]);
        overlay_frame(
            &mut app,
            &ctx,
            screen,
            vec![egui::Event::PointerMoved(end), pointer_button(end, false)],
        );
        assert_eq!(screen.drag.bounds(), Some(Rect::from_min_max(top, end)));
    }

    #[test]
    fn losing_focus_stops_drag_without_accepting_partial_capture() {
        let mut drag = RegionDrag::default();
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        drag.update(
            &[
                pointer_button(Pos2::new(10., 10.), true),
                egui::Event::PointerMoved(Pos2::new(100., 100.)),
            ],
            bounds,
            Rect::NOTHING,
        );
        drag.update(
            &[
                egui::Event::WindowFocused(false),
                egui::Event::PointerMoved(Pos2::new(700., 500.)),
            ],
            bounds,
            Rect::NOTHING,
        );
        assert!(!drag.active);
        assert!(!drag.completed);
        assert_eq!(
            drag.bounds(),
            Some(Rect::from_min_max(
                Pos2::new(10., 10.),
                Pos2::new(100., 100.)
            ))
        );
    }

    #[test]
    fn missed_release_finishes_at_last_drag_point_before_a_new_press() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        let mut drag = RegionDrag::default();
        drag.update(
            &[
                pointer_button(Pos2::new(10., 10.), true),
                egui::Event::PointerMoved(Pos2::new(100., 100.)),
                egui::Event::PointerGone,
            ],
            bounds,
            Rect::NOTHING,
        );
        let mut events = vec![];
        drag.recover_release(&mut events, Some(true), egui::Modifiers::NONE);
        assert!(events.is_empty());
        drag.recover_release(&mut events, Some(false), egui::Modifiers::NONE);
        assert_eq!(events.len(), 1);
        drag.update(&events, bounds, Rect::NOTHING);
        assert!(!drag.active && drag.completed);
        assert_eq!(
            drag.bounds(),
            Some(Rect::from_min_max(
                Pos2::new(10., 10.),
                Pos2::new(100., 100.)
            ))
        );

        drag.active = true;
        let mut events = vec![pointer_button(Pos2::new(500., 400.), true)];
        drag.recover_release(&mut events, Some(false), egui::Modifiers::NONE);
        drag.update(&events, bounds, Rect::NOTHING);
        assert!(drag.active && !drag.completed);
        assert_eq!(drag.start, Some(Pos2::new(500., 400.)));

        let mut events = vec![egui::Event::WindowFocused(false)];
        drag.recover_release(&mut events, Some(false), egui::Modifiers::NONE);
        assert_eq!(events.len(), 1);
        let mut events = vec![pointer_button(Pos2::new(600., 500.), false)];
        drag.recover_release(&mut events, Some(false), egui::Modifiers::NONE);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn cancel_on_secondary_closes_all_overlays_and_restores_only_editor() {
        use std::{cell::RefCell, rc::Rc};
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        let session = test_screens(&ctx, CaptureMode::Region);
        let ids: Vec<_> = session
            .screens
            .iter()
            .map(|s| overlay_id(&s.display))
            .collect();
        let secondary = ids[1];
        let shown = Rc::new(RefCell::new(Vec::new()));
        let shown_for_renderer = shown.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            shown_for_renderer.borrow_mut().push(viewport.ids.this);
            let _ = ctx.run(
                egui::RawInput {
                    viewport_id: viewport.ids.this,
                    viewports: [(viewport.ids.this, egui::ViewportInfo::default())]
                        .into_iter()
                        .collect(),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events: if viewport.ids.this == secondary {
                        vec![key(egui::Key::Escape)]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ctx| (viewport.viewport_ui_cb)(ctx),
            );
        });
        app.selection = Some(session);
        let output = ctx.run(egui::RawInput::default(), |ctx| app.selection_ui(ctx));
        assert_eq!(*shown.borrow(), ids);
        assert!(app.selection.is_none());
        assert_eq!(app.status, "Capture canceled");
        for id in ids {
            let commands = &output.viewport_output[&id].commands;
            assert!(commands.contains(&ViewportCommand::Visible(false)));
            assert!(commands.contains(&ViewportCommand::Close));
            assert!(!commands.contains(&ViewportCommand::Decorations(true)));
        }
        let editor_commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(editor_commands.contains(&ViewportCommand::Visible(true)));
        assert_eq!(
            editor_commands.contains(&ViewportCommand::Decorations(true)),
            !cfg!(target_os = "macos")
        );
        if cfg!(target_os = "macos") {
            assert!(!editor_commands.iter().any(|command| matches!(
                command,
                ViewportCommand::InnerSize(_) | ViewportCommand::OuterPosition(_)
            )));
            assert_eq!(app.restore_frames_left, 0);
        }
        ctx.set_embed_viewports(true);
    }

    #[test]
    fn finish_without_scroll_keeps_session_open_and_warns_instead_of_saving() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.displays.push(DisplayInfo {
            id: 1,
            name: "Test display".into(),
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            scale_factor: 1.,
            is_primary: true,
        });
        app.start_window_scroll(
            &ctx,
            app.displays[0].clone(),
            42,
            WindowScrollArea {
                crop: Some([0., 0., 1., 1.]),
                bounds: Rect::from_min_size(Pos2::new(20., 30.), Vec2::new(300., 400.)),
                return_hidden: false,
            },
            image::RgbaImage::new(300, 400),
            Local::now(),
        );
        app.scroll_session.as_mut().unwrap().finish_requested = true;
        app.finish_scroll_capture(&ctx);
        let session = app
            .scroll_session
            .as_ref()
            .expect("single frame must not silently save");
        assert!(session.single_frame_warning);
        assert!(session.paused);
        assert!(!session.finish_requested);
        assert_eq!(session.stitcher.as_ref().unwrap().sections, 1);
        assert!(app.document.is_none());
        assert!(!app.busy);
        assert!(session.message.contains("No additional sections"));
    }
    #[test]
    fn fast_capture_only_finishes_completed_single_selections_when_enabled() {
        for mode in CaptureMode::ALL {
            assert!(!capture_on_completion(false, mode, true));
            assert!(!capture_on_completion(true, mode, false));
            assert_eq!(
                capture_on_completion(true, mode, true),
                matches!(
                    mode,
                    CaptureMode::Region | CaptureMode::Window | CaptureMode::Text
                )
            );
        }
    }

    #[test]
    fn text_capture_stays_a_free_region_with_space_and_command_drag() {
        let ctx = egui::Context::default();
        let mut app = MyImageApp::with_context(&ctx, false, Arc::new(AtomicBool::new(false)), true);
        app.settings.capture_mode = CaptureMode::Text;
        let image = image::RgbaImage::new(800, 600);
        let mut screen = Selection {
            display: DisplayInfo {
                id: 1,
                name: "Test display".into(),
                x: 0,
                y: 0,
                width: 800,
                height: 600,
                scale_factor: 1.,
                is_primary: true,
            },
            texture: load_texture(&ctx, "text-capture-test", &image),
            image,
            captured_at: Local::now(),
            drag: Default::default(),
            multi: Default::default(),
            mode: CaptureMode::Text,
            return_hidden: true,
            placement_frames: 4,
            reveal_time: None,
            shade_started_at: None,
            windows: vec![capture::WindowInfo {
                id: 9,
                name: "Underlying window".into(),
                x: 50,
                y: 50,
                width: 600,
                height: 400,
            }],
            window_id: None,
            hover_cycle: 0,
            magnifier: false,
            magnifier_preview: Default::default(),
            pointer_ready: true,
            #[cfg(target_os = "macos")]
            webcam: Default::default(),
            #[cfg(target_os = "macos")]
            video_target: Default::default(),
        };
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.selection_overlay_ui(ctx, &mut screen, None);
                },
            )
        };
        frame(vec![]);
        let output = frame(vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!labels.contains(&"Capture"));
        assert!(labels.contains(&"Copy text"));
        let start = egui::pos2(100., 100.);
        let end = egui::pos2(300., 220.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        frame(vec![egui::Event::PointerMoved(end)]);
        frame(vec![button(end, false)]);
        let selection = &screen;
        assert_eq!(selection.mode, CaptureMode::Text);
        assert_eq!(
            selection.drag.bounds(),
            Some(Rect::from_min_max(start, end))
        );
        assert!(selection.multi.rects.is_empty());
        assert_eq!(app.settings.capture_mode, CaptureMode::Text);
        assert_eq!(
            app.settings.capture_destination,
            capture::CaptureDestination::Editor
        );
        assert!(app.document.is_none());
    }

    #[test]
    fn multiple_regions_add_adjust_remove_and_keep_preview_order() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(800.));
        let mut multi = MultiRegions::default();
        let mut drag = RegionDrag::default();
        let gesture =
            |multi: &mut MultiRegions, drag: &mut RegionDrag, a: Pos2, b: Pos2, command: bool| {
                let button = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers {
                        command,
                        ..Default::default()
                    },
                };
                multi.update(
                    drag,
                    &[
                        button(a, true),
                        egui::Event::PointerMoved(b),
                        button(b, false),
                    ],
                    bounds,
                    Rect::NOTHING,
                    &[],
                );
            };
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(50., 50.),
            egui::pos2(150., 150.),
            false,
        );
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(400., 400.),
            egui::pos2(300., 300.),
            false,
        );
        assert_eq!(multi.rects.len(), 2);
        assert_eq!(multi.rects[1].size(), Vec2::splat(100.));
        // Grabbing the first region selects it without rearranging or adding.
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(100., 100.),
            egui::pos2(200., 100.),
            false,
        );
        assert_eq!(multi.selected, Some(0));
        assert_eq!(multi.rects[0].min, egui::pos2(150., 50.));
        assert_eq!(multi.preview(&drag), multi.rects);
        // Resize a corner; other regions remain untouched.
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(250., 150.),
            egui::pos2(280., 180.),
            false,
        );
        assert_eq!(multi.rects[0].size(), Vec2::splat(130.));
        assert_eq!(multi.rects[1].min, egui::pos2(300., 300.));
        // Modifier starts an overlapping region rather than moving the first.
        gesture(
            &mut multi,
            &mut drag,
            egui::pos2(180., 80.),
            egui::pos2(350., 250.),
            true,
        );
        assert_eq!(multi.rects.len(), 3);
        multi.remove_selected(&mut drag);
        assert_eq!(multi.rects.len(), 2);
        assert!(drag.bounds().is_none());
        assert_eq!(multi.preview(&drag), multi.rects);
    }

    #[test]
    fn multiple_regions_ignore_controls_and_bound_region_count() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(800.));
        let controls = Rect::from_min_size(Pos2::ZERO, Vec2::splat(40.));
        let mut multi = MultiRegions::default();
        let mut drag = RegionDrag::default();
        let events = |a, b| {
            [
                egui::Event::PointerButton {
                    pos: a,
                    pressed: true,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::PointerButton {
                    pos: b,
                    pressed: false,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::COMMAND,
                },
            ]
        };
        multi.update(
            &mut drag,
            &events(egui::pos2(10., 10.), egui::pos2(200., 200.)),
            bounds,
            controls,
            &[],
        );
        assert!(multi.rects.is_empty());
        for _ in 0..40 {
            multi.update(
                &mut drag,
                &events(egui::pos2(100., 100.), egui::pos2(200., 200.)),
                bounds,
                controls,
                &[],
            );
        }
        assert_eq!(multi.rects.len(), 32);
        assert!(!drag.active);
    }

    #[test]
    fn smart_region_clicks_front_window_but_drags_and_handles_remain_free() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let front = Rect::from_min_max(egui::pos2(100., 100.), egui::pos2(500., 400.));
        let back = Rect::from_min_max(egui::pos2(50., 50.), egui::pos2(900., 650.));
        let windows = [(1, front), (2, back)];
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let click = egui::pos2(200., 200.);
        let mut d = RegionDrag::default();
        d.update_smart_region(
            &[button(click, true), button(click, false)],
            bounds,
            Rect::NOTHING,
            &windows,
        );
        assert_eq!(d.bounds(), Some(front));
        // Resizing after a snapped click must not select a different window.
        d.update_smart_region(
            &[
                button(front.right_bottom(), true),
                egui::Event::PointerMoved(egui::pos2(700., 500.)),
                button(egui::pos2(700., 500.), false),
            ],
            bounds,
            Rect::NOTHING,
            &windows,
        );
        assert_eq!(d.bounds().unwrap().max, egui::pos2(700., 500.));
        d = RegionDrag::default();
        d.update_smart_region(
            &[
                button(click, true),
                egui::Event::PointerMoved(egui::pos2(800., 600.)),
                button(egui::pos2(800., 600.), false),
            ],
            bounds,
            Rect::NOTHING,
            &windows,
        );
        assert_eq!(
            d.bounds(),
            Some(Rect::from_min_max(click, egui::pos2(800., 600.)))
        );
        d = RegionDrag::default();
        d.update_smart_region(
            &[button(click, true), button(click, false)],
            bounds,
            front,
            &windows,
        );
        assert!(
            d.bounds().is_none(),
            "Clicks on controls never select windows"
        );
    }

    #[test]
    fn capture_confirmation_stays_near_selection_and_inside_screen() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let size = Vec2::new(600., 62.);
        for selection in [
            Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(300., 200.)),
            Rect::from_min_max(egui::pos2(800., 600.), egui::pos2(990., 690.)),
            bounds,
        ] {
            let panel = Rect::from_min_size(controls_position(bounds, selection, size), size);
            assert!(bounds.contains_rect(panel));
            if selection != bounds {
                assert!(!panel.intersects(selection));
            }
        }
    }

    #[test]
    fn scrolling_window_click_drag_and_adjust_share_the_same_boundary() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 700.));
        let window = Rect::from_min_max(egui::pos2(100., 80.), egui::pos2(900., 650.));
        let windows = [(7, window)];
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let gesture = |drag: &mut RegionDrag, selected: &mut Option<u32>, a, b| {
            drag.update_window(
                &[
                    button(a, true),
                    egui::Event::PointerMoved(b),
                    button(b, false),
                ],
                bounds,
                Rect::NOTHING,
                &windows,
                selected,
            )
        };
        let mut drag = RegionDrag::default();
        let mut selected = None;
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(200., 200.),
            egui::pos2(200., 200.),
        );
        assert_eq!(selected, Some(7));
        assert_eq!(drag.bounds(), Some(window));
        assert_eq!(window_selection(drag.bounds().unwrap(), window), None);
        drag = RegionDrag::default();
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(300., 222.5),
            egui::pos2(700., 507.5),
        );
        assert_eq!(
            window_selection(drag.bounds().unwrap(), window),
            Some([0.25, 0.25, 0.75, 0.75])
        );
        let pixels =
            selection::selection_to_pixels([0.25, 0.25], [0.75, 0.75], [1., 1.], [1600, 1140])
                .unwrap();
        assert_eq!(
            (pixels.x, pixels.y, pixels.width, pixels.height),
            (400, 285, 800, 570)
        );
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(700., 350.),
            egui::pos2(1200., 350.),
        );
        assert_eq!(drag.bounds().unwrap().right(), 900.);
        gesture(
            &mut drag,
            &mut selected,
            egui::pos2(500., 350.),
            egui::pos2(-200., -200.),
        );
        assert_eq!(drag.bounds().unwrap().min, window.min);
        assert_eq!(selected, Some(7));
    }
    #[test]
    fn capture_selection_can_move_resize_and_stay_within_display() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        let controls = Rect::from_min_size(Pos2::new(100., 520.), Vec2::new(600., 70.));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let mut drag = RegionDrag::default();
        let gesture = |a, b, drag: &mut RegionDrag| {
            drag.update(
                &[
                    button(a, true),
                    egui::Event::PointerMoved(b),
                    button(b, false),
                ],
                bounds,
                controls,
            )
        };
        gesture(Pos2::new(100., 100.), Pos2::new(300., 300.), &mut drag);
        gesture(Pos2::new(200., 200.), Pos2::new(250., 230.), &mut drag);
        assert_eq!(
            drag.bounds(),
            Some(Rect::from_min_max(
                Pos2::new(150., 130.),
                Pos2::new(350., 330.)
            ))
        );
        gesture(Pos2::new(350., 230.), Pos2::new(430., 230.), &mut drag);
        assert_eq!(drag.bounds().unwrap().right(), 430.);
        gesture(Pos2::new(150., 130.), Pos2::new(80., 60.), &mut drag);
        assert_eq!(drag.bounds().unwrap().min, Pos2::new(80., 60.));
        gesture(
            drag.bounds().unwrap().center(),
            Pos2::new(-500., -500.),
            &mut drag,
        );
        assert_eq!(drag.bounds().unwrap().min, Pos2::ZERO);
        let rect = drag.bounds().unwrap();
        gesture(
            controls.center(),
            controls.center() + Vec2::splat(20.),
            &mut drag,
        );
        assert_eq!(drag.bounds(), Some(rect));
        gesture(Pos2::new(600., 100.), Pos2::new(950., 400.), &mut drag);
        assert_eq!(drag.bounds().unwrap().right(), 800.);
    }
    #[test]
    fn region_drag_handles_coalesced_events_and_ignores_controller_clicks() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        let bar = Rect::from_min_size(Pos2::new(100., 500.), Vec2::new(600., 80.));
        let a = Pos2::new(90., 120.);
        let b = Pos2::new(600., 450.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        for timing in 0..3 {
            let mut drag = RegionDrag::default();
            let events = [
                button(a, true),
                egui::Event::PointerMoved(b),
                button(b, false),
            ];
            if timing == 0 {
                drag.update(&events, bounds, bar);
            } else {
                drag.update(&events[..1], bounds, bar);
                if timing == 1 {
                    drag.update(&events[1..], bounds, bar);
                } else {
                    for event in &events[1..] {
                        drag.update(std::slice::from_ref(event), bounds, bar);
                    }
                }
            }
            assert_eq!(drag.bounds(), Some(Rect::from_two_pos(a, b)));
            drag.update(
                &[
                    button(bar.center(), true),
                    button(bar.center(), false),
                    egui::Event::PointerMoved(Pos2::ZERO),
                ],
                bounds,
                bar,
            );
            assert_eq!(drag.bounds(), Some(Rect::from_two_pos(a, b)));
        }
    }
}
