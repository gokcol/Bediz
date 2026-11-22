//! The capture selector owns webcam placement. The recorder owns the file
//! and native streams until finalization completes.
use super::*;
#[path = "video/options.rs"]
mod options;
pub(crate) use options::{VideoPreferences, VideoTarget, preferences_ui};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    hash::{BuildHasher, RandomState},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr::NonNull,
    time::Instant,
};

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Webcam {
    #[default]
    Off,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Free,
    Random,
}

impl Webcam {
    const ALL: [Self; 7] = [
        Self::Off,
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
        Self::Free,
        Self::Random,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
            Self::Free => "Free position",
            Self::Random => "Random position",
        }
    }

    fn position(self, free: Vec2, random: Vec2) -> Option<Vec2> {
        match self {
            Self::Off => None,
            Self::TopLeft => Some(Vec2::ZERO),
            Self::TopRight => Some(Vec2::X),
            Self::BottomLeft => Some(Vec2::Y),
            Self::BottomRight => Some(Vec2::splat(1.)),
            Self::Free => Some(free),
            Self::Random => Some(random),
        }
    }
}

pub(super) struct WebcamSetup {
    pub mode: Webcam,
    free: Vec2,
    random: Vec2,
    drag_offset: Option<Vec2>,
    preview: Option<WebcamPreview>,
    pub preview_error: Option<String>,
}

impl Default for WebcamSetup {
    fn default() -> Self {
        Self {
            mode: Webcam::Off,
            free: Vec2::splat(0.5),
            random: Vec2::splat(0.5),
            drag_offset: None,
            preview: None,
            preview_error: None,
        }
    }
}

impl WebcamSetup {
    pub fn menu(&mut self, ui: &mut egui::Ui) {
        let response = ui
            .menu_button("     ▾", |ui| {
                ui.strong("Camera overlay");
                for mode in Webcam::ALL {
                    if ui
                        .selectable_value(&mut self.mode, mode, mode.label())
                        .clicked()
                    {
                        self.drag_offset = None;
                        if mode == Webcam::Random {
                            let bits = RandomState::new().hash_one("bediz-webcam-position");
                            self.random = Vec2::new(bits as u32 as f32, (bits >> 32) as u32 as f32)
                                / u32::MAX as f32;
                        }
                        ui.close();
                    }
                }
            })
            .response;
        let label = format!("Camera overlay: {}", self.mode.label());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), &label)
        });
        appearance::icon(
            ui.painter(),
            response.rect.left_center() + Vec2::new(16., 0.),
            Icon::Camera,
            if self.mode == Webcam::Off {
                ui.visuals().weak_text_color()
            } else {
                appearance::palette(ui.ctx()).accent
            },
            18.,
        );
        response.on_hover_text(label);
    }

    fn bubble_geometry(rect: Rect) -> (Pos2, Vec2, f32) {
        // Match Recorder.swift; x and y express travel inside the selected
        // frame, with the circle kept clear of its edges at all positions.
        let smaller = rect.width().min(rect.height());
        let diameter = smaller * 0.22;
        let inset = smaller * 0.035;
        (
            rect.min + Vec2::splat(inset + diameter * 0.5),
            rect.size() - Vec2::splat(2. * inset + diameter),
            diameter * 0.5,
        )
    }

    pub fn drag(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        bar: Rect,
        events: &mut Vec<egui::Event>,
    ) {
        if self.mode != Webcam::Free {
            return;
        }
        let (origin, travel, radius) = Self::bubble_geometry(rect);
        let bubble = origin + travel * self.free;
        if ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| p.distance(bubble) <= radius && !bar.contains(p))
            || self.drag_offset.is_some()
        {
            ctx.set_cursor_icon(if self.drag_offset.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        events.retain(|event| match *event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } if pos.distance(bubble) <= radius && !bar.contains(pos) => {
                self.drag_offset = Some(bubble - pos);
                false
            }
            egui::Event::PointerMoved(pos) => {
                let Some(offset) = self.drag_offset else {
                    return true;
                };
                self.free = ((pos + offset - origin) / travel).clamp(Vec2::ZERO, Vec2::splat(1.));
                false
            }
            egui::Event::PointerButton {
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            }
            | egui::Event::PointerGone
            | egui::Event::WindowFocused(false)
                if self.drag_offset.take().is_some() =>
            {
                false
            }
            _ => true,
        });
    }

    pub fn paint(&self, painter: &egui::Painter, rect: Rect) {
        let Some(position) = self.mode.position(self.free, self.random) else {
            return;
        };
        let (origin, travel, radius) = Self::bubble_geometry(rect);
        let center = origin + travel * position;
        let palette = appearance::palette(painter.ctx());
        painter.circle_filled(center, radius, palette.navy.gamma_multiply(0.92));
        painter.circle_stroke(
            center,
            radius,
            Stroke::new(1.4, Color32::from_white_alpha(190)),
        );
        appearance::icon(
            painter,
            center,
            Icon::Video,
            Color32::WHITE,
            (radius * 1.12).min(28.),
        );
    }

    fn native_position(&self) -> Vec2 {
        self.mode
            .position(self.free, self.random)
            .unwrap_or(Vec2::splat(-1.))
    }

    pub fn update_preview(
        &mut self,
        ctx: &egui::Context,
        screen: &DisplayInfo,
        frame: Option<Rect>,
        menu_open: bool,
    ) {
        let Some(frame) = frame.filter(|_| self.mode != Webcam::Off) else {
            self.preview = None;
            self.preview_error = None;
            return;
        };
        let position = self.native_position();
        let preview = self.preview.get_or_insert_with(WebcamPreview::new);
        let (status, message) = preview.status();
        self.preview_error = (status < 0).then_some(message);
        if menu_open || status <= 0 {
            preview.hide();
        } else {
            let scale = ctx.zoom_factor();
            let target = Rect::from_min_size(
                egui::pos2(screen.x as f32, screen.y as f32) + frame.min.to_vec2() * scale,
                frame.size() * scale,
            );
            preview.show(target, position, None);
        }
        if status == 0 {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }
}

struct WebcamPreview(NonNull<c_void>);

impl WebcamPreview {
    fn new() -> Self {
        // SAFETY: Native preview creation and use are on the AppKit main thread.
        Self(NonNull::new(unsafe { bediz_webcam_create() }).expect("Retained Swift preview"))
    }

    fn show(&self, target: Rect, position: Vec2, window: Option<u32>) {
        // SAFETY: The native preview owns one retained reference until Drop.
        unsafe {
            bediz_webcam_show(
                self.0.as_ptr(),
                f64::from(target.min.x),
                f64::from(target.min.y),
                f64::from(target.width()),
                f64::from(target.height()),
                f64::from(position.x),
                f64::from(position.y),
                window.unwrap_or(0),
            );
        }
    }

    fn hide(&self) {
        self.show(Rect::ZERO, Vec2::ZERO, None);
    }

    fn status(&self) -> (i32, String) {
        let mut message = [0 as c_char; 512];
        // SAFETY: The probe writes a terminated string within this buffer.
        let status =
            unsafe { bediz_webcam_status(self.0.as_ptr(), message.as_mut_ptr(), message.len()) };
        let message = unsafe { CStr::from_ptr(message.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (status, message)
    }
}

impl Drop for WebcamPreview {
    fn drop(&mut self) {
        // SAFETY: Our owned preview is always closed on the AppKit main thread.
        unsafe { bediz_webcam_release(self.0.as_ptr()) };
    }
}

struct LiveWebcam {
    preview: WebcamPreview,
    target: Rect,
    position: Vec2,
    window: Option<u32>,
}

unsafe extern "C" {
    fn bediz_webcam_create() -> *mut c_void;
    fn bediz_webcam_show(
        handle: *const c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        camera_x: f64,
        camera_y: f64,
        window: u32,
    );
    fn bediz_webcam_status(handle: *const c_void, message: *mut c_char, capacity: usize) -> i32;
    fn bediz_webcam_release(handle: *const c_void);
    fn bediz_record_start(
        path: *const c_char,
        display: u32,
        window: u32,
        cursor: i32,
        webcam: *const c_void,
        camera_x: f64,
        camera_y: f64,
        output_width: u32,
        output_height: u32,
        bits_per_second: u32,
        max_seconds: u32,
        disk_reserve: u64,
        audio_source: u32,
        audio_bits_per_second: u32,
        voice_processing: i32,
        countdown_seconds: u32,
    ) -> *mut c_void;
    fn bediz_record_status(handle: *mut c_void, out: *mut c_char, capacity: usize) -> i32;
    fn bediz_record_command(handle: *mut c_void, command: i32);
    fn bediz_record_release(handle: *mut c_void);
    fn bediz_play_video(path: *const c_char);
    fn bediz_record_thumbnail(
        movie: *const c_char,
        poster: *const c_char,
        width: isize,
        height: isize,
    ) -> i32;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Starting,
    Countdown,
    Recording,
    Paused,
    Saving,
    Finished,
    Canceled,
    Failed,
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum Command {
    Pause = 1,
    Resume = 2,
    Stop = 3,
}

pub(super) struct Recording {
    native: NonNull<c_void>,
    staging: tempfile::TempDir,
    destination: PathBuf,
    position: Pos2,
    return_hidden: bool,
    placement_frames: u8,
    active_started: Option<Instant>,
    elapsed: Duration,
    stopping: bool,
    webcam: Option<LiveWebcam>,
}

impl Drop for Recording {
    fn drop(&mut self) {
        // SAFETY: The pointer owns one retained Swift recorder. Its native
        // queues keep themselves alive while finishing, independently of Rust.
        unsafe { bediz_record_release(self.native.as_ptr()) };
    }
}

impl Recording {
    fn new(
        settings: &Settings,
        screen: &mut Selection,
        region: selection::PixelRect,
        window: Option<u32>,
        position: Pos2,
    ) -> Result<Self, String> {
        if region.width < 16 || region.height < 16 {
            return Err("Choose a window or display at least 16 × 16 pixels to record.".into());
        }
        let root = settings.output_dir.join("Recordings");
        options::check_start_space(&root)?;
        let plan = settings.video.plan(region.width, region.height);
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("Could not create the Recordings folder: {error}"))?;
        let staging = tempfile::Builder::new()
            .prefix(".bediz-recording-")
            .tempdir_in(&root)
            .map_err(|error| format!("Could not prepare a temporary recording: {error}"))?;
        let destination = root.join(format!(
            "Recording {}.mp4",
            Local::now().format("%Y-%m-%d %H-%M-%S-%3f")
        ));
        let path = CString::new(staging.path().join("recording.mp4").as_os_str().as_bytes())
            .map_err(|_| "The recording path contains an invalid character.")?;
        let camera = screen.webcam.native_position();
        let display = &screen.display;
        let webcam = (screen.webcam.mode != Webcam::Off).then(|| LiveWebcam {
            preview: screen
                .webcam
                .preview
                .take()
                .unwrap_or_else(WebcamPreview::new),
            target: Rect::from_min_size(
                egui::pos2(display.x as f32, display.y as f32),
                egui::vec2(display.width as f32, display.height as f32),
            ),
            position: camera,
            window,
        });
        let webcam_ptr = webcam.as_ref().map_or(std::ptr::null(), |webcam| {
            webcam.preview.0.as_ptr().cast_const()
        });
        // SAFETY: Swift copies the UTF-8 path while this call is active, and
        // returns a retained handle for the UI to poll and release.
        let native = unsafe {
            bediz_record_start(
                path.as_ptr(),
                screen.display.id,
                window.unwrap_or(0),
                i32::from(settings.include_pointer),
                webcam_ptr,
                f64::from(camera.x),
                f64::from(camera.y),
                plan.width,
                plan.height,
                plan.bits_per_second,
                plan.max_seconds,
                options::DISK_RESERVE,
                plan.audio_source,
                plan.audio_bits_per_second,
                i32::from(plan.voice_processing),
                plan.countdown_seconds,
            )
        };
        Ok(Self {
            native: NonNull::new(native).ok_or("The native recorder could not be started.")?,
            staging,
            destination,
            position,
            return_hidden: screen.return_hidden,
            placement_frames: 0,
            active_started: None,
            elapsed: Duration::ZERO,
            stopping: false,
            webcam,
        })
    }

    fn status(&self) -> (Status, String) {
        let mut message = [0 as c_char; 1024];
        // SAFETY: The recorder is retained, and the caller owns this buffer.
        let status = unsafe {
            bediz_record_status(self.native.as_ptr(), message.as_mut_ptr(), message.len())
        };
        let status = match status {
            0 => Status::Starting,
            1 => Status::Recording,
            2 => Status::Paused,
            3 => Status::Saving,
            4 => Status::Finished,
            5 => Status::Canceled,
            6 => Status::Countdown,
            _ => Status::Failed,
        };
        // The zeroed buffer and the Swift bridge both guarantee termination.
        let message = unsafe { CStr::from_ptr(message.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        (status, message)
    }

    fn control(&mut self, command: Command) {
        if self.stopping {
            return;
        }
        self.stopping = matches!(command, Command::Stop);
        // SAFETY: The recorder enqueues the command before returning.
        unsafe { bediz_record_command(self.native.as_ptr(), command as i32) };
    }

    fn duration(&mut self, status: Status) -> Duration {
        if status == Status::Recording && !self.stopping {
            self.elapsed
                + self
                    .active_started
                    .get_or_insert_with(Instant::now)
                    .elapsed()
        } else {
            if let Some(started) = self.active_started.take() {
                self.elapsed += started.elapsed();
            }
            self.elapsed
        }
    }

    fn publish(&self) -> Result<(), String> {
        // Publish a finalized MP4 atomically and never replace an existing recording.
        let source = self.staging.path().join("recording.mp4");
        std::fs::hard_link(source, &self.destination)
            .map_err(|error| format!("Could not save {}: {error}", self.destination.display()))
    }

    pub fn focus(&mut self, ctx: &egui::Context) {
        // Keep the current Space even if the active app is in native full screen.
        crate::capture_window::reveal_after_paint(&title());
        crate::runtime::wake_root(ctx);
    }
}

fn title() -> String {
    format!("{} Capture — Recording", crate::branding::NAME)
}

fn viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("bediz-recording-controller")
}

pub(super) fn window_size(screen: &Selection, id: u32) -> Option<[u32; 2]> {
    let window = screen.windows.iter().find(|w| w.id == id)?;
    let scale = f64::from(screen.image.width()) / f64::from(screen.display.width);
    Some([
        (f64::from(window.width) * scale).round() as u32,
        (f64::from(window.height) * scale).round() as u32,
    ])
}

impl MyImageApp {
    pub(super) fn start_window_recording(
        &mut self,
        ctx: &egui::Context,
        screen: &mut Selection,
        id: u32,
    ) {
        if let Some([width, height]) = window_size(screen, id) {
            let region = selection::PixelRect {
                x: 0,
                y: 0,
                width,
                height,
            };
            self.start_recording(ctx, screen, region, Some(id));
        } else {
            self.error = Some("The window chosen for recording is no longer available.".into());
            self.restore_window(ctx, false);
        }
    }

    pub(super) fn start_recording(
        &mut self,
        ctx: &egui::Context,
        screen: &mut Selection,
        region: selection::PixelRect,
        window: Option<u32>,
    ) {
        let scale = ctx.zoom_factor();
        let display = &screen.display;
        let w = display.width as f32 / scale;
        let h = display.height as f32 / scale;
        let selection_bottom = (region.y + region.height) as f32 / screen.image.height() as f32 * h;
        let y = if selection_bottom + 68. <= h {
            selection_bottom + 8.
        } else {
            (h - 82.).max(32.)
        };
        let result = Recording::new(
            &self.settings,
            screen,
            region,
            window,
            Pos2::new(
                display.x as f32 / scale + ((w - 230.) / 2.).max(8.),
                display.y as f32 / scale + y,
            ),
        );
        match result {
            Ok(recording) => {
                self.recording = Some(recording);
                crate::runtime::wake_root(ctx);
            }
            Err(error) => {
                self.error = Some(error);
                self.restore_window(ctx, false);
            }
        }
    }

    pub(super) fn recording_ui(&mut self, ctx: &egui::Context) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        let (status, message) = recording.status();
        if matches!(status, Status::Finished | Status::Failed | Status::Canceled) {
            let recording = self.recording.take().unwrap();
            crate::capture_window::forget_overlay_reveals();
            ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Close);
            match status {
                Status::Finished => match recording.publish() {
                    Ok(()) => {
                        self.remember_recent(recording.destination.clone());
                        self.status = format!(
                            "Saved {}",
                            recording
                                .destination
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                        );
                        if !message.is_empty() {
                            self.status.push_str(" · ");
                            self.status.push_str(&message);
                        }
                        self.restore_window(ctx, false);
                    }
                    Err(error) => {
                        let staging = recording.staging.path().to_owned();
                        // Keep the finished MP4 available if the final publish
                        // failed (e.g. the destination volume became read-only).
                        let mut recording = recording;
                        recording.staging.disable_cleanup(true);
                        self.error = Some(format!(
                            "{error}. The finished video is at {}.",
                            staging.join("recording.mp4").display()
                        ));
                        self.restore_window(ctx, false);
                    }
                },
                Status::Failed => {
                    self.error = Some(message);
                    self.restore_window(ctx, false);
                }
                _ => {
                    self.status = "Recording canceled".into();
                    self.restore_window(ctx, recording.return_hidden);
                }
            }
            return;
        }
        let placing = recording.placement_frames < 3;
        let first_reveal = recording.placement_frames == 3;
        if recording.stopping || status == Status::Saving {
            recording.webcam = None;
        } else if let Some(webcam) = &recording.webcam {
            webcam
                .preview
                .show(webcam.target, webcam.position, webcam.window);
        }
        let time = recording.duration(status).as_secs();
        let look = self.settings.capture_theme.look();
        let title = title();
        crate::capture_window::prepare();
        ctx.show_viewport_immediate(
            viewport(),
            egui::ViewportBuilder::default()
                .with_title(&title)
                .with_position(recording.position)
                .with_inner_size([230., 48.])
                .with_resizable(false)
                .with_taskbar(false)
                .with_decorations(false)
                .with_has_shadow(false)
                .with_transparent(true)
                .with_active(false)
                .with_visible(false),
            |child, _| {
                if child.input(|i| {
                    i.viewport().close_requested()
                        || (matches!(status, Status::Starting | Status::Countdown)
                            && i.key_pressed(egui::Key::Escape))
                }) {
                    child.send_viewport_cmd(ViewportCommand::CancelClose);
                    recording.control(Command::Stop);
                }
                if placing {
                    child.send_viewport_cmd(ViewportCommand::OuterPosition(recording.position));
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(child, |ui| {
                        egui::Frame::new()
                            .fill(look.palette.paper)
                            .stroke(Stroke::new(1., look.palette.border))
                            .corner_radius(10.)
                            .inner_margin(egui::Margin::symmetric(9, 6))
                            .show(ui, |ui| {
                                crate::capture_panel::apply_look(ui, &look);
                                ui.set_min_width(210.);
                                ui.horizontal(|ui| {
                                    let live = status == Status::Recording && !recording.stopping;
                                    let red = Color32::from_rgb(231, 56, 59);
                                    let (dot, _) = ui.allocate_exact_size(
                                        Vec2::splat(12.),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        dot.center(),
                                        4.,
                                        if live { red } else { look.palette.muted },
                                    );
                                    let message = if recording.stopping || status == Status::Saving
                                    {
                                        "Saving…".into()
                                    } else if status == Status::Countdown {
                                        message.clone()
                                    } else if status == Status::Starting {
                                        "Starting…".into()
                                    } else {
                                        format!(
                                            "{:02}:{:02}:{:02}",
                                            time / 3600,
                                            time / 60 % 60,
                                            time % 60
                                        )
                                    };
                                    ui.add_sized(
                                        [93., 30.],
                                        egui::Label::new(
                                            egui::RichText::new(message)
                                                .monospace()
                                                .color(look.palette.ink),
                                        ),
                                    );
                                    let pause = status == Status::Paused;
                                    ui.add_enabled_ui(
                                        matches!(status, Status::Recording | Status::Paused)
                                            && !recording.stopping,
                                        |ui| {
                                            if appearance::icon_button(
                                                ui,
                                                if pause {
                                                    "Resume recording"
                                                } else {
                                                    "Pause recording"
                                                },
                                                if pause { Icon::Play } else { Icon::Pause },
                                            )
                                            .clicked()
                                            {
                                                recording.control(if pause {
                                                    Command::Resume
                                                } else {
                                                    Command::Pause
                                                });
                                            }
                                        },
                                    );
                                    ui.add_enabled_ui(
                                        status != Status::Saving && !recording.stopping,
                                        |ui| {
                                            if appearance::icon_button(
                                                ui,
                                                if matches!(
                                                    status,
                                                    Status::Starting | Status::Countdown
                                                ) {
                                                    "Cancel recording"
                                                } else {
                                                    "Stop and save recording"
                                                },
                                                Icon::Stop,
                                            )
                                            .clicked()
                                            {
                                                recording.control(Command::Stop);
                                            }
                                        },
                                    );
                                });
                            });
                    });
                if first_reveal {
                    crate::capture_window::reveal_after_paint(&title);
                }
            },
        );
        recording.placement_frames = recording.placement_frames.saturating_add(1);
        if placing {
            crate::runtime::wake_root(ctx);
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

pub(super) fn play(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "This recording is no longer available at {}.",
            path.display()
        ));
    }
    let native_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "The video path contains an invalid character.")?;
    // SAFETY: This runs on AppKit's main thread. Swift copies the path synchronously.
    unsafe { bediz_play_video(native_path.as_ptr()) };
    Ok(())
}

pub(crate) fn thumbnail(path: &Path, width: u32, height: u32) -> Result<image::RgbaImage, String> {
    let staging = tempfile::tempdir().map_err(|error| error.to_string())?;
    let poster = staging.path().join("preview.jpg");
    let input = CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let output = CString::new(poster.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    // SAFETY: Both paths remain owned by this worker for the synchronous call.
    let decoded = unsafe {
        bediz_record_thumbnail(
            input.as_ptr(),
            output.as_ptr(),
            width as isize,
            height as isize,
        )
    };
    if decoded == 0 {
        return Err("Could not read the recording preview.".into());
    }
    image::open(poster)
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webcam_corner_presets_and_custom_position_stay_inside_the_capture() {
        let bounds = Rect::from_min_size(Pos2::new(50., 180.), Vec2::new(420., 780.));
        let free = Vec2::new(0.37, 0.81);
        let random = Vec2::new(0.55, 0.14);
        let (origin, travel, radius) = WebcamSetup::bubble_geometry(bounds);
        assert_eq!(Webcam::Off.position(free, random), None);
        assert_eq!(Webcam::Free.position(free, random), Some(free));
        for (mode, anchor) in [
            (Webcam::TopLeft, Vec2::ZERO),
            (Webcam::TopRight, Vec2::X),
            (Webcam::BottomLeft, Vec2::Y),
            (Webcam::BottomRight, Vec2::splat(1.)),
            (Webcam::Free, free),
            (Webcam::Random, random),
        ] {
            assert_eq!(mode.position(free, random), Some(anchor));
            let bubble = Rect::from_center_size(origin + travel * anchor, Vec2::splat(radius * 2.));
            assert!(bounds.contains_rect(bubble));
        }
    }

    #[test]
    fn dragging_the_webcam_consumes_pointer_events_and_clamps_to_the_region() {
        let ctx = egui::Context::default();
        let mut setup = WebcamSetup {
            mode: Webcam::Free,
            ..WebcamSetup::default()
        };
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(500., 300.));
        let (origin, travel, _) = WebcamSetup::bubble_geometry(rect);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let start = origin + travel * setup.free;
        let outside = Rect::from_min_size(rect.right_bottom(), Vec2::splat(40.));
        let mut events = vec![
            pointer(start, true),
            egui::Event::PointerMoved(Pos2::new(560., -18.)),
            pointer(Pos2::new(560., -18.), false),
        ];
        setup.drag(&ctx, rect, outside, &mut events);
        assert_eq!(setup.native_position(), Vec2::X);
        assert!(events.is_empty());
        assert!(setup.drag_offset.is_none());
    }
}
